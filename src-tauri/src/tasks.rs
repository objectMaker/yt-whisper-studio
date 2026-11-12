use crate::cookies;
use crate::settings::Settings;
use crate::tools;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub tasks: Mutex<HashMap<String, Task>>,
    pub pids: Mutex<HashMap<String, u32>>,
    pub browser_lock: Mutex<()>,
    /// 被用户暂停的资源下载（键为 kind:id）
    pub canceled_assets: Mutex<HashSet<String>>,
    /// 任务列表有变更，等待落盘
    pub tasks_dirty: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings: Mutex::new(settings),
            tasks: Mutex::new(HashMap::new()),
            pids: Mutex::new(HashMap::new()),
            browser_lock: Mutex::new(()),
            canceled_assets: Mutex::new(HashSet::new()),
            tasks_dirty: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub url: Option<String>,
    pub stage: String,
    pub progress: f64,
    pub speed: String,
    pub eta: String,
    pub message: String,
    pub file_path: Option<String>,
    pub srt_path: Option<String>,
    pub error: Option<String>,
    pub model: Option<String>,
    pub created_at: u64,
    /// 下载开始时间（毫秒时间戳）
    pub download_started_at: Option<u64>,
    /// 视频下载耗时（秒）
    pub download_seconds: Option<f64>,
    /// 音频转码耗时（秒）
    pub audio_seconds: Option<f64>,
    /// 字幕转录耗时（秒）
    pub transcribe_seconds: Option<f64>,
    /// 成品文件大小（字节）
    pub file_size: Option<u64>,
    /// 下载任务创建时选定的处理选项，供重试/继续时复用
    #[serde(default)]
    pub extract_mp3: bool,
    #[serde(default)]
    pub transcribe: bool,
    pub log: Vec<String>,
}

impl Task {
    fn new(id: String, kind: &str) -> Self {
        Self {
            id,
            kind: kind.into(),
            title: String::new(),
            url: None,
            stage: "queued".into(),
            progress: 0.0,
            speed: String::new(),
            eta: String::new(),
            message: "排队中".into(),
            file_path: None,
            srt_path: None,
            error: None,
            model: None,
            created_at: now_ms(),
            download_started_at: None,
            download_seconds: None,
            audio_seconds: None,
            transcribe_seconds: None,
            file_size: None,
            extract_mp3: false,
            transcribe: false,
            log: Vec::new(),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadOptions {
    #[serde(default)]
    pub extract_mp3: Option<bool>,
    #[serde(default)]
    pub transcribe: Option<bool>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TranscribeOptions {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn uid() -> String {
    format!("t{}", uuid::Uuid::new_v4().simple())
}

pub fn patch<F: FnOnce(&mut Task)>(app: &AppHandle, id: &str, f: F) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut map = state.tasks.lock().unwrap();
        match map.get_mut(id) {
            Some(t) => {
                f(t);
                Some(t.clone())
            }
            None => None,
        }
    };
    if let Some(t) = snapshot {
        state
            .tasks_dirty
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = app.emit("task-updated", t);
    }
}

fn push_log(task: &mut Task, line: &str) {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return;
    }
    task.log.push(trimmed.to_string());
    if task.log.len() > 200 {
        let overflow = task.log.len() - 200;
        task.log.drain(0..overflow);
    }
}

fn is_canceled(app: &AppHandle, id: &str) -> bool {
    let state = app.state::<AppState>();
    let map = state.tasks.lock().unwrap();
    map.get(id)
        .map(|t| t.stage == "canceled" || t.stage == "paused")
        .unwrap_or(false)
}

pub fn cancel(app: &AppHandle, id: &str) {
    // 下载任务用「暂停」语义：保留 .part，稍后可继续；转录任务没有断点，直接取消。
    let is_download = {
        let state = app.state::<AppState>();
        let map = state.tasks.lock().unwrap();
        map.get(id).map(|t| t.kind == "download").unwrap_or(false)
    };
    patch(app, id, |t| {
        t.speed.clear();
        t.eta.clear();
        if is_download {
            t.stage = "paused".into();
            t.message = "已暂停，可点击继续".into();
        } else {
            t.stage = "canceled".into();
            t.message = "正在取消…".into();
        }
    });
    let state = app.state::<AppState>();
    let pid = state.pids.lock().unwrap().get(id).copied();
    if let Some(pid) = pid {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.output();
    }
}

fn tasks_file(app: &AppHandle) -> PathBuf {
    crate::tools::app_data_dir(app).join("tasks.json")
}

/// 把当前任务列表落盘，供下次启动恢复。
pub fn save_tasks(app: &AppHandle) {
    let list = {
        let state = app.state::<AppState>();
        let map = state.tasks.lock().unwrap();
        let mut list: Vec<Task> = map.values().cloned().collect();
        list.sort_by_key(|t| t.created_at);
        list
    };
    let Ok(text) = serde_json::to_string(&list) else {
        return;
    };
    let path = tasks_file(app);
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// 启动时恢复历史任务。上次未完成的任务标记为可继续/可重试，而不是一直转圈。
pub fn restore_tasks(app: &AppHandle) {
    let Ok(text) = std::fs::read_to_string(tasks_file(app)) else {
        return;
    };
    let Ok(list) = serde_json::from_str::<Vec<Task>>(&text) else {
        return;
    };
    let state = app.state::<AppState>();
    let mut map = state.tasks.lock().unwrap();
    for mut t in list {
        let was_active = matches!(
            t.stage.as_str(),
            "queued" | "preparing" | "downloading" | "extracting" | "transcribing"
        );
        if was_active {
            if t.kind == "download" {
                t.stage = "paused".into();
                t.message = "上次未完成，可点击继续".into();
            } else {
                t.stage = "error".into();
                t.message = "上次未完成，可点击重试".into();
            }
            t.speed.clear();
            t.eta.clear();
        }
        map.insert(t.id.clone(), t);
    }
}

fn reset_for_retry(app: &AppHandle, id: &str) {
    patch(app, id, |t| {
        t.stage = "queued".into();
        t.progress = 0.0;
        t.speed.clear();
        t.eta.clear();
        t.error = None;
        t.srt_path = None;
        t.download_started_at = None;
        t.download_seconds = None;
        t.audio_seconds = None;
        t.transcribe_seconds = None;
        t.message = "准备重新开始…".into();
        push_log(t, "— 重新开始 —");
    });
}

/// 失败/暂停后的「继续」：下载走 yt-dlp 断点续传，转录重新执行。
pub fn retry(app: &AppHandle, id: &str) -> Result<(), String> {
    let task = {
        let state = app.state::<AppState>();
        let map = state.tasks.lock().unwrap();
        map.get(id).cloned()
    }
    .ok_or_else(|| "任务不存在".to_string())?;
    if matches!(
        task.stage.as_str(),
        "queued" | "preparing" | "downloading" | "extracting" | "transcribing"
    ) {
        return Err("任务正在进行中".into());
    }
    let app2 = app.clone();
    let id2 = id.to_string();
    if task.kind == "download" {
        let url = task.url.clone().ok_or_else(|| "任务缺少链接".to_string())?;
        let opts = DownloadOptions {
            extract_mp3: Some(task.extract_mp3),
            transcribe: Some(task.transcribe),
        };
        reset_for_retry(app, id);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = do_download(&app2, &id2, &url, &opts).await {
                if !is_canceled(&app2, &id2) {
                    patch(&app2, &id2, |t| {
                        t.stage = "error".into();
                        t.error = Some(e.clone());
                        t.message = e;
                    });
                }
            }
        });
    } else {
        let path = task
            .file_path
            .clone()
            .ok_or_else(|| "任务缺少源文件".to_string())?;
        let opts = TranscribeOptions {
            model: task.model.clone(),
            language: None,
        };
        reset_for_retry(app, id);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = do_transcribe(&app2, &id2, Path::new(&path), &opts).await {
                if is_canceled(&app2, &id2) {
                    patch(&app2, &id2, |t| {
                        t.stage = "canceled".into();
                        t.message = "已取消".into();
                    });
                } else {
                    patch(&app2, &id2, |t| {
                        t.stage = "error".into();
                        t.error = Some(e.clone());
                        t.message = e;
                    });
                }
            }
        });
    }
    Ok(())
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(reader: R, tx: UnboundedSender<String>) {
    tokio::spawn(async move {
        // 不能用 lines()：子进程在中文 Windows 上按 GBK 输出（out gbk），
        // 标题里的全角字符不是合法 UTF-8，严格解码会直接结束读取、关闭管道，
        // 导致 yt-dlp 写进度时报 [Errno 22] 并退出 120。这里按字节读、宽松解码。
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    while matches!(buf.last(), Some(b'\n' | b'\r')) {
                        buf.pop();
                    }
                    let line = String::from_utf8_lossy(&buf).to_string();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

async fn run_process<F: FnMut(&str)>(
    app: &AppHandle,
    task_id: &str,
    exe: &Path,
    args: &[String],
    mut on_line: F,
) -> Result<i32, String> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // 强制 Python 系工具（yt-dlp）用 UTF-8 输出，避免中文 Windows 下回落到 GBK
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法启动 {}: {e}", exe.display()))?;
    if let Some(pid) = child.id() {
        app.state::<AppState>()
            .pids
            .lock()
            .unwrap()
            .insert(task_id.to_string(), pid);
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    if let Some(out) = child.stdout.take() {
        spawn_reader(out, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        spawn_reader(err, tx.clone());
    }
    drop(tx);
    while let Some(line) = rx.recv().await {
        on_line(&line);
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    app.state::<AppState>()
        .pids
        .lock()
        .unwrap()
        .remove(task_id);
    Ok(status.code().unwrap_or(-1))
}

struct TempGuard(Vec<PathBuf>);

impl Drop for TempGuard {
    fn drop(&mut self) {
        for p in &self.0 {
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(p);
            } else {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// 转录用的临时目录：放在最终字幕文件所在的文件夹里（和输出同盘），
/// 不放在系统 Temp 里。里面放转 16k 的 wav 和 whisper 的输出文件；
/// 任务结束（成功、失败或取消）时由 TempGuard 整个删掉。
/// 这样 whisper 的 srt 可以直接重命名为最终字幕，不用跨盘复制，
/// 也不会在 Temp 里留下 wav/srt。
fn transcribe_tmpdir(anchor: &Path) -> PathBuf {
    match anchor.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => dir.join(format!(".ytw-tmp-{}", uuid::Uuid::new_v4().simple())),
        None => std::env::temp_dir().join(format!("ytw-{}", uuid::Uuid::new_v4().simple())),
    }
}

fn sanitize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '⧸' | '／' | '\\' | '|' | '?' | '*' | '<' | '>' | '"' | ':' => out.push('-'),
            c if (c as u32) < 0x20 => out.push('-'),
            _ => out.push(ch),
        }
    }
    let joined = out.split_whitespace().collect::<Vec<_>>().join(" ");
    joined
        .trim_end_matches(['.', ' '])
        .to_string()
}

/// 清洗目录名与文件名（yt-dlp 会把标题中的 / 替换成 ⧸ 等非法字符）
fn sanitize_output_path(dir: &Path, file: &Path) -> (PathBuf, PathBuf) {
    let mut dir_out = dir.to_path_buf();
    let mut file_out = file.to_path_buf();
    if let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str())) {
        let clean = sanitize_name(name);
        if clean != name {
            let target = parent.join(&clean);
            if !target.exists() && std::fs::rename(dir, &target).is_ok() {
                dir_out = target;
                if let Ok(rel) = file.strip_prefix(dir) {
                    file_out = dir_out.join(rel);
                }
            }
        }
    }
    if let (Some(parent), Some(name)) = (
        file_out.parent(),
        file_out.file_name().and_then(|n| n.to_str()),
    ) {
        let clean = sanitize_name(name);
        if clean != name {
            let target = parent.join(&clean);
            if !target.exists() && std::fs::rename(&file_out, &target).is_ok() {
                file_out = target;
            }
        }
    }
    (dir_out, file_out)
}

fn prepare_cookie_file(app: &AppHandle, settings: &Settings) -> Option<PathBuf> {
    if !settings.cookies_file.is_empty() {
        let p = PathBuf::from(&settings.cookies_file);
        if p.exists() {
            return Some(p);
        }
    }
    if !settings.use_embedded_cookies {
        return None;
    }
    let webview = app.get_webview("yt-browser")?;
    match cookies::read_youtube_cookies(&webview) {
        Ok(list) if !list.is_empty() => {
            let path = std::env::temp_dir().join(format!("ytw-{}.txt", uuid::Uuid::new_v4().simple()));
            std::fs::write(&path, cookies::to_netscape(&list)).ok()?;
            if cfg!(debug_assertions) {
                let _ = std::fs::copy(&path, std::env::temp_dir().join("ytw-cookies-debug.txt"));
            }
            Some(path)
        }
        _ => None,
    }
}

/// 读取 Windows 系统代理（IE/Edge 设置），让 yt-dlp 与内置浏览器走同一条网络路径。
/// 带 Cookie 下载时 yt-dlp 需要联网获取 EJS 签名求解脚本，直连 GitHub 常常失败。
fn system_proxy() -> Option<String> {
    crate::net::system_proxy()
}

/// 带 Cookie 下载需要执行 YouTube 的 n 签名挑战，必须有 JS 运行时 + EJS 求解脚本。
/// 优先 deno（脚本从 GitHub 拉取），没有 deno 时退回 node（脚本走 npm 包）。
fn js_solver_args() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    if which::which("deno").is_ok() {
        v.extend(["--js-runtimes", "deno", "--remote-components", "ejs:github"].map(String::from));
    } else if which::which("node").is_ok() {
        v.extend(
            ["--no-js-runtimes", "--js-runtimes", "node", "--remote-components", "ejs:npm"]
                .map(String::from),
        );
    } else {
        // 交给 yt-dlp 报错，同时给用户可读的提示
        v.extend(["--remote-components", "ejs:github"].map(String::from));
    }
    v
}

fn backend_label(backend: &str) -> &'static str {
    match backend {
        "cuda" => "NVIDIA CUDA",
        "vulkan" => "AMD/Intel Vulkan",
        _ => "CPU",
    }
}

/// 依据设置与硬件挑选 whisper 可执行文件；未安装对应加速版时回退 CPU
fn whisper_backend(app: &AppHandle, settings: &Settings) -> (String, PathBuf) {
    let order: Vec<&str> = match settings.compute.as_str() {
        "cpu" => vec!["cpu"],
        _ => match crate::hardware::system().recommended.as_str() {
            "cuda" => vec!["cuda", "vulkan", "cpu"],
            "vulkan" => vec!["vulkan", "cuda", "cpu"],
            _ => vec!["cpu"],
        },
    };
    for b in order {
        let id = match b {
            "cuda" => "whisper-cuda",
            "vulkan" => "whisper-vulkan",
            _ => "whisper",
        };
        let local = tools::whisper_dir_for(app, b).join("whisper-cli.exe");
        if local.exists() {
            return (b.to_string(), local);
        }
        if b == "cpu" {
            if let Some(p) = tools::resolve_exe(app, id) {
                return ("cpu".into(), p);
            }
        }
    }
    ("cpu".into(), PathBuf::from("whisper-cli"))
}

fn parse_download_line(line: &str) -> Option<(f64, String, String)> {
    let idx = line.find("[download]")?;
    let rest = &line[idx + 10..];
    let pct_pos = rest.find('%')?;
    let pct = rest[..pct_pos].trim().parse::<f64>().ok()?;
    let speed = rest
        .find(" at ")
        .map(|i| rest[i + 4..].split_whitespace().next().unwrap_or("").to_string())
        .unwrap_or_default();
    let eta = rest
        .find("ETA ")
        .map(|i| rest[i + 4..].trim().to_string())
        .unwrap_or_default();
    Some((pct, speed, eta))
}

fn parse_whisper_progress(line: &str) -> Option<f64> {
    let idx = line.find("progress =")?;
    let rest = line[idx + 10..].trim();
    let pct_pos = rest.find('%')?;
    rest[..pct_pos].trim().parse::<f64>().ok()
}

/// 生成 16k 单声道 wav，供 whisper.cpp 使用
async fn to_wav(app: &AppHandle, task_id: &str, ffmpeg: &Path, input: &Path, out: &Path) -> Result<(), String> {
    let args = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().to_string(),
        "-vn".into(),
        "-ac".into(),
        "1".into(),
        "-ar".into(),
        "16000".into(),
        "-c:a".into(),
        "pcm_s16le".into(),
        out.to_string_lossy().to_string(),
    ];
    let code = run_process(app, task_id, ffmpeg, &args, |_| {}).await?;
    if code != 0 {
        return Err(format!("ffmpeg 音频转换失败（退出码 {code}）"));
    }
    Ok(())
}

async fn run_whisper(
    app: &AppHandle,
    task_id: &str,
    whisper: &Path,
    model: &Path,
    wav: &Path,
    language: &str,
    base_progress: f64,
    span: f64,
    cpu_only: bool,
) -> Result<PathBuf, String> {
    let prefix = wav.with_extension("");
    let of = prefix.to_string_lossy().to_string();
    let mut args = vec![
        "-m".to_string(),
        model.to_string_lossy().to_string(),
        "-f".to_string(),
        wav.to_string_lossy().to_string(),
        "-osrt".to_string(),
        "-of".to_string(),
        of,
        "-pp".to_string(),
        "-t".to_string(),
        std::thread::available_parallelism()
            .map(|v| v.get().min(16))
            .unwrap_or(4)
            .to_string(),
    ];
    if cpu_only {
        args.push("-ng".into());
    }
    if !language.is_empty() && language != "auto" {
        args.push("-l".into());
        args.push(language.to_string());
    }
    let mut last_emit = 0f64;
    let code = run_process(app, task_id, whisper, &args, |line| {
        if let Some(pct) = parse_whisper_progress(line) {
            if pct - last_emit >= 0.8 {
                last_emit = pct;
                let overall = base_progress + span * pct / 100.0;
                patch(app, task_id, |t| {
                    t.progress = overall.min(99.0);
                    t.message = format!("Whisper 转录中 {:.0}%", pct);
                });
            }
        } else if line.contains("error") || line.contains("failed") {
            patch(app, task_id, |t| push_log(t, line));
        }
    })
    .await?;
    if code != 0 {
        return Err(format!("whisper.cpp 转录失败（退出码 {code}）"));
    }
    let srt = prefix.with_extension("srt");
    if srt.exists() {
        Ok(srt)
    } else {
        Err("未生成 SRT 字幕文件".into())
    }
}

pub fn start_download(app: AppHandle, url: String, opts: DownloadOptions) -> Result<Task, String> {
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("无效的链接".into());
    }
    let id = uid();
    let mut task = Task::new(id.clone(), "download");
    task.url = Some(url.clone());
    task.title = url.clone();
    {
        let state = app.state::<AppState>();
        let settings = state.settings.lock().unwrap().clone();
        task.extract_mp3 = opts.extract_mp3.unwrap_or(settings.extract_mp3);
        task.transcribe = opts.transcribe.unwrap_or(settings.transcribe);
    }
    let snapshot = task.clone();
    {
        let state = app.state::<AppState>();
        state.tasks.lock().unwrap().insert(id.clone(), task);
    }
    let _ = app.emit("task-updated", snapshot.clone());
    save_tasks(&app);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = do_download(&app, &id, &url, &opts).await {
            if is_canceled(&app, &id) {
                patch(&app, &id, |t| {
                    if t.stage != "paused" {
                        t.stage = "canceled".into();
                        t.message = "已取消".into();
                    }
                });
            } else {
                patch(&app, &id, |t| {
                    t.stage = "error".into();
                    t.error = Some(e.clone());
                    t.message = e;
                });
            }
        }
    });
    Ok(snapshot)
}

async fn do_download(app: &AppHandle, task_id: &str, url: &str, opts: &DownloadOptions) -> Result<(), String> {
    if is_canceled(app, task_id) {
        return Ok(());
    }
    let settings = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        s
    };
    let extract_mp3 = opts.extract_mp3.unwrap_or(settings.extract_mp3);
    let transcribe = opts.transcribe.unwrap_or(settings.transcribe);

    let ytdlp = tools::resolve_exe(app, "yt-dlp")
        .ok_or_else(|| "未找到 yt-dlp，请先在「工具链」页面一键安装".to_string())?;

    let out_dir = PathBuf::from(&settings.output_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建输出目录失败: {e}"))?;
    // Windows 单条路径上限 260 字符，标题过长时 yt-dlp 会在创建目录/文件时报
    // "[Errno 22] Invalid argument"。按输出目录长度动态截断标题，并附加视频 ID 防重名。
    let out_len = out_dir.to_string_lossy().chars().count();
    let per = (200usize.saturating_sub(out_len) / 2).clamp(24, 100);
    let out_template = out_dir
        .join(format!("%(title).{per}B [%(id)s]"))
        .join(format!("%(title).{per}B [%(id)s].%(ext)s"));
    let path_file = std::env::temp_dir().join(format!("ytw-path-{}.txt", uuid::Uuid::new_v4().simple()));
    let mut guards = vec![path_file.clone()];

    let cookie_file = prepare_cookie_file(app, &settings);
    if let Some(p) = &cookie_file {
        guards.push(p.clone());
    }
    let _guard = TempGuard(guards);

    patch(app, task_id, |t| {
        t.stage = "downloading".into();
        t.message = "正在解析并下载…".into();
        if t.download_started_at.is_none() {
            t.download_started_at = Some(now_ms());
        }
        push_log(
            t,
            if cookie_file.is_some() {
                "已复用内置浏览器 YouTube 会话 Cookie"
            } else {
                "未使用 Cookie（仅能下载公开视频）"
            },
        );
    });

    let fmt = if settings.quality == "best" {
        "bv*+ba/b".to_string()
    } else {
        format!("bv*[height<={}]+ba/b", settings.quality)
    };

    // 必须使用登录 Cookie：匿名请求会触发 YouTube 风控甚至封 IP。
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-playlist".into(),
        "--no-mtime".into(),
        "--windows-filenames".into(),
        "-f".into(),
        fmt.clone(),
        "--merge-output-format".into(),
        "mp4".into(),
        "-o".into(),
        out_template.to_string_lossy().to_string(),
        // 先写目标路径：已下载过（无移动）时也能拿到文件位置
        "--print-to-file".into(),
        "%(filename)s".into(),
        path_file.to_string_lossy().to_string(),
        "--print-to-file".into(),
        "after_move:filepath".into(),
        path_file.to_string_lossy().to_string(),
    ];
    args.extend(js_solver_args());
    if let Some(proxy) = system_proxy() {
        args.push("--proxy".into());
        args.push(proxy.clone());
        patch(app, task_id, |t| {
            push_log(t, &format!("已检测到系统代理，yt-dlp 走 {proxy}"));
        });
    } else {
        patch(app, task_id, |t| {
            push_log(t, "未检测到系统代理，yt-dlp 直连（若开启 TUN 模式仍可正常联网）");
        });
    }
    if let Some(p) = cookie_file.as_deref() {
        args.push("--cookies".into());
        args.push(p.to_string_lossy().to_string());
    }
    args.push(url.to_string());

    // yt-dlp 的 HttpFD 只重试 TransportError / HTTP 5xx 等错误；下载中途
    // 套接字抛出的 OSError（如 [Errno 22] Invalid argument）会直接导致
    // 整个下载失败（退出码 120）。这类错误大多是网络/代理的瞬间中断，
    // 而 yt-dlp 支持断点续传（.part 文件），所以在应用层重试几次，
    // 每次失败后等待几秒再继续，未完成的部分会接着下，不会从头重下。
    const MAX_ATTEMPTS: usize = 3;
    let download_clock = Instant::now();
    let mut code = -1;
    let mut tail = String::new();
    let mut attempt = 0;
    while attempt < MAX_ATTEMPTS {
        attempt += 1;
        let mut last_emit = 0f64;
        let mut recent: Vec<String> = Vec::new();
        // Python 解释器退出时往已关闭的 stdout 刷缓存会打印一行无害的
        // "Exception ignored while flushing sys.stdout"（及随后的一行
        // OSError），它不是下载失败的原因，不收入任务日志，免得误导用户。
        let mut skip_flush_noise = false;
        code = run_process(app, task_id, &ytdlp, &args, |line| {
            if let Some((pct, speed, eta)) = parse_download_line(line) {
                if pct - last_emit >= 0.5 || pct >= 100.0 {
                    last_emit = pct;
                    patch(app, task_id, |t| {
                        t.progress = (pct * 0.7).min(70.0);
                        t.speed = speed.clone();
                        t.eta = eta.clone();
                        t.message = format!("下载中 {pct:.1}%");
                    });
                }
            } else {
                if line.starts_with("Exception ignored while flushing sys.stdout") {
                    skip_flush_noise = true;
                    return;
                }
                if skip_flush_noise {
                    skip_flush_noise = false;
                    if line.trim_start().starts_with("OSError:") {
                        return;
                    }
                }
                patch(app, task_id, |t| push_log(t, line));
                recent.push(line.to_string());
                if recent.len() > 40 {
                    recent.remove(0);
                }
            }
        })
        .await?;
        if is_canceled(app, task_id) {
            return Ok(());
        }
        if code == 0 {
            break;
        }
        let errors: Vec<String> = recent
            .iter()
            .filter(|l| l.to_ascii_lowercase().contains("error"))
            .cloned()
            .collect();
        let picked: Vec<String> = if errors.is_empty() {
            recent.iter().rev().take(2).cloned().collect()
        } else {
            errors.iter().rev().take(2).cloned().collect()
        };
        tail = picked.into_iter().rev().collect::<Vec<_>>().join(" | ");
        if attempt < MAX_ATTEMPTS {
            let wait_secs = if attempt == 1 { 5 } else { 15 };
            patch(app, task_id, |t| {
                t.message = format!("下载中断（退出码 {code}），{wait_secs} 秒后重试（第 {attempt}/{MAX_ATTEMPTS} 次）…");
                push_log(
                    t,
                    &format!("下载中断，{wait_secs} 秒后自动重试（第 {attempt}/{MAX_ATTEMPTS} 次），已下载的部分会断点续传"),
                );
            });
            tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
            if is_canceled(app, task_id) {
                return Ok(());
            }
        }
    }

    if code != 0 {
        let hint = if tail.contains("Errno 22") || tail.contains("Invalid argument") {
            "；也可能是 Windows 路径过长或文件名非法，可尝试缩短输出目录"
        } else {
            ""
        };
        if tail.is_empty() {
            return Err(format!(
                "yt-dlp 退出码 {code}（已重试 {MAX_ATTEMPTS} 次），请检查链接/网络/登录态；输出目录：{}{hint}",
                out_dir.display()
            ));
        }
        return Err(format!(
            "yt-dlp 退出码 {code}：{tail}（已重试 {MAX_ATTEMPTS} 次；输出目录：{}；可能是网络临时中断，可稍后重试或检查代理{hint}）",
            out_dir.display()
        ));
    }

    patch(app, task_id, |t| {
        t.download_seconds = Some(download_clock.elapsed().as_secs_f64());
    });

    let raw_path = std::fs::read_to_string(&path_file)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .last()
        .ok_or_else(|| "未获取到下载文件路径".to_string())?;
    let mut file = PathBuf::from(&raw_path);
    if !file.exists() {
        return Err("下载完成但文件不存在".into());
    }
    let dir = file.parent().map(|p| p.to_path_buf()).unwrap_or(out_dir.clone());
    let (_, cleaned) = sanitize_output_path(&dir, &file);
    file = cleaned;
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video")
        .to_string();
    let file_size = std::fs::metadata(&file).ok().map(|m| m.len());
    patch(app, task_id, |t| {
        t.file_path = Some(file.to_string_lossy().to_string());
        t.title = stem.clone();
        t.file_size = file_size;
        t.progress = 72.0;
        t.message = "视频下载完成".into();
        t.speed.clear();
        t.eta.clear();
    });

    let ffmpeg = tools::resolve_exe(app, "ffmpeg")
        .ok_or_else(|| "未找到 ffmpeg，请先在「工具链」页面一键安装".to_string())?;

    let mut audio_for_whisper: Option<PathBuf> = None;
    if extract_mp3 {
        patch(app, task_id, |t| {
            t.stage = "extracting".into();
            t.progress = 74.0;
            t.message = "正在提取 MP3…".into();
        });
        let mp3 = file.with_extension("mp3");
        let args = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-i".into(),
            file.to_string_lossy().to_string(),
            "-vn".into(),
            "-c:a".into(),
            "libmp3lame".into(),
            "-q:a".into(),
            "2".into(),
            mp3.to_string_lossy().to_string(),
        ];
        let audio_clock = Instant::now();
        match run_process(app, task_id, &ffmpeg, &args, |_| {}).await {
            Ok(0) => {
                let elapsed = audio_clock.elapsed().as_secs_f64();
                patch(app, task_id, |t| {
                    t.progress = 77.0;
                    t.audio_seconds = Some(elapsed);
                    push_log(t, &format!("MP3 已生成: {}", mp3.display()));
                    t.message = "MP3 提取完成".into();
                });
                audio_for_whisper = Some(mp3);
            }
            Ok(code) => patch(app, task_id, |t| {
                push_log(t, &format!("ffmpeg 提取 MP3 失败（退出码 {code}），跳过"));
            }),
            Err(e) => patch(app, task_id, |t| push_log(t, &e)),
        }
    }
    if is_canceled(app, task_id) {
        return Ok(());
    }

    if transcribe {
        let model_id = settings.model.clone();
        let model = match tools::model_path(app, &model_id) {
            Some(p) if p.exists() => p,
            _ if settings.auto_download_model => {
                patch(app, task_id, |t| {
                    t.stage = "extracting".into();
                    t.message = format!("正在下载 Whisper 模型 {model_id}…");
                });
                tools::ensure_model(app, &model_id)
                    .await
                    .map_err(|e| format!("模型准备失败: {e}"))?
            }
            _ => return Err(format!("Whisper 模型 {model_id} 未安装")),
        };
        let (backend, whisper) = whisper_backend(app, &settings);
        patch(app, task_id, |t| {
            t.stage = "transcribing".into();
            t.progress = 80.0;
            t.model = Some(model_id.clone());
            t.message = format!("正在转录音频（{}）…", backend_label(&backend));
            push_log(t, &format!("转录后端: {}", backend_label(&backend)));
        });
        let source = audio_for_whisper.unwrap_or_else(|| file.clone());
        let tmpdir = transcribe_tmpdir(&file);
        std::fs::create_dir_all(&tmpdir).map_err(|e| e.to_string())?;
        let _tmp_guard = TempGuard(vec![tmpdir.clone()]);
        let wav = tmpdir.join("audio.wav");
        to_wav(app, task_id, &ffmpeg, &source, &wav).await?;
        if is_canceled(app, task_id) {
            return Ok(());
        }
        let transcribe_clock = Instant::now();
        let srt = run_whisper(
            app,
            task_id,
            &whisper,
            &model,
            &wav,
            &settings.language,
            80.0,
            19.0,
            backend == "cpu",
        )
        .await?;
        let transcribe_secs = transcribe_clock.elapsed().as_secs_f64();
        patch(app, task_id, |t| {
            t.transcribe_seconds = Some(transcribe_secs);
        });
        let final_srt = file.with_file_name(format!("{stem}.srt"));
        if srt != final_srt {
            if std::fs::rename(&srt, &final_srt).is_err() {
                let _ = std::fs::copy(&srt, &final_srt);
            }
        }
        patch(app, task_id, |t| {
            t.srt_path = Some(final_srt.to_string_lossy().to_string());
            push_log(t, &format!("字幕已生成: {}", final_srt.display()));
        });
    }
    if is_canceled(app, task_id) {
        return Ok(());
    }

    let final_dir = file.parent().map(|p| p.to_path_buf());
    patch(app, task_id, |t| {
        t.stage = "done".into();
        t.progress = 100.0;
        t.message = "全部完成".into();
    });
    if settings.open_folder {
        if let Some(dir) = final_dir {
            let _ = std::process::Command::new("explorer")
                .arg(dir)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
    }
    Ok(())
}

pub fn start_transcribe(app: AppHandle, paths: Vec<String>, opts: TranscribeOptions) -> Result<Vec<Task>, String> {
    if paths.is_empty() {
        return Err("请先选择媒体文件".into());
    }
    let mut created = Vec::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if !path.exists() {
            continue;
        }
        let id = uid();
        let mut task = Task::new(id.clone(), "transcribe");
        task.title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("media")
            .to_string();
        task.file_path = Some(path.to_string_lossy().to_string());
        task.file_size = std::fs::metadata(&path).ok().map(|m| m.len());
        let snapshot = task.clone();
        {
            let state = app.state::<AppState>();
            state.tasks.lock().unwrap().insert(id.clone(), task);
        }
        let _ = app.emit("task-updated", snapshot.clone());
        created.push(snapshot);
        let app2 = app.clone();
        let opts2 = opts.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = do_transcribe(&app2, &id, &path, &opts2).await {
                if is_canceled(&app2, &id) {
                    patch(&app2, &id, |t| {
                        t.stage = "canceled".into();
                        t.message = "已取消".into();
                    });
                } else {
                    patch(&app2, &id, |t| {
                        t.stage = "error".into();
                        t.error = Some(e.clone());
                        t.message = e;
                    });
                }
            }
        });
    }
    save_tasks(&app);
    Ok(created)
}

async fn do_transcribe(
    app: &AppHandle,
    task_id: &str,
    input: &Path,
    opts: &TranscribeOptions,
) -> Result<(), String> {
    if is_canceled(app, task_id) {
        return Ok(());
    }
    let settings = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        s
    };
    let ffmpeg = tools::resolve_exe(app, "ffmpeg")
        .ok_or_else(|| "未找到 ffmpeg，请先在「工具链」页面一键安装".to_string())?;
    let (backend, whisper) = whisper_backend(app, &settings);
    let model_id = opts.model.clone().unwrap_or(settings.model);
    let model = match tools::model_path(app, &model_id) {
        Some(p) if p.exists() => p,
        _ if settings.auto_download_model => {
            patch(app, task_id, |t| {
                t.stage = "preparing".into();
                t.message = format!("正在下载 Whisper 模型 {model_id}…");
            });
            tools::ensure_model(app, &model_id)
                .await
                .map_err(|e| format!("模型准备失败: {e}"))?
        }
        _ => return Err(format!("Whisper 模型 {model_id} 未安装")),
    };
    let language = opts
        .language
        .clone()
        .unwrap_or_else(|| settings.language.clone());
    let tmpdir = transcribe_tmpdir(input);
    std::fs::create_dir_all(&tmpdir).map_err(|e| e.to_string())?;
    let _tmp_guard = TempGuard(vec![tmpdir.clone()]);
    let wav = tmpdir.join("audio.wav");
    patch(app, task_id, |t| {
        t.stage = "extracting".into();
        t.progress = 5.0;
        t.message = "正在解析音频…".into();
        t.model = Some(model_id.clone());
    });
    let audio_clock = Instant::now();
    to_wav(app, task_id, &ffmpeg, input, &wav).await?;
    let audio_secs = audio_clock.elapsed().as_secs_f64();
    patch(app, task_id, |t| {
        t.audio_seconds = Some(audio_secs);
    });
    if is_canceled(app, task_id) {
        return Ok(());
    }
    patch(app, task_id, |t| {
        t.stage = "transcribing".into();
        t.progress = 12.0;
        t.message = format!("Whisper 转录中（{}）…", backend_label(&backend));
        push_log(t, &format!("转录后端: {}", backend_label(&backend)));
    });
    let transcribe_clock = Instant::now();
    let srt = run_whisper(
        app,
        task_id,
        &whisper,
        &model,
        &wav,
        &language,
        12.0,
        87.0,
        backend == "cpu",
    )
    .await?;
    let transcribe_secs = transcribe_clock.elapsed().as_secs_f64();
    patch(app, task_id, |t| {
        t.transcribe_seconds = Some(transcribe_secs);
    });
    if is_canceled(app, task_id) {
        return Ok(());
    }
    let final_srt = input.with_extension("srt");
    if srt != final_srt {
        if std::fs::rename(&srt, &final_srt).is_err() {
            std::fs::copy(&srt, &final_srt).map_err(|e| e.to_string())?;
        }
    }
    patch(app, task_id, |t| {
        t.stage = "done".into();
        t.progress = 100.0;
        t.srt_path = Some(final_srt.to_string_lossy().to_string());
        t.message = "转录完成".into();
        push_log(t, &format!("字幕已生成: {}", final_srt.display()));
    });
    if settings.open_folder {
        if let Some(dir) = input.parent() {
            let mut cmd = std::process::Command::new("explorer");
            cmd.arg(dir);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.spawn();
        }
    }
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub has_mp3_sibling: bool,
}

const AUDIO_EXTS: &[&str] = &[
    "mp3", "m4a", "wav", "flac", "ogg", "opus", "aac", "mp4", "mov", "webm", "mkv",
];

pub fn scan_folder(folder: &Path) -> Vec<MediaFile> {
    let mut files = Vec::new();
    collect_media(folder, &mut files, 0);
    let mp3_stems: Vec<String> = files
        .iter()
        .filter(|f| f.to_lowercase().ends_with(".mp3"))
        .filter_map(|f| {
            Path::new(f)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase())
        })
        .collect();
    files
        .into_iter()
        .filter_map(|f| {
            let path = PathBuf::from(&f);
            let name = path.file_name()?.to_str()?.to_string();
            let ext = path.extension()?.to_str()?.to_lowercase();
            let stem = path.file_stem()?.to_str()?.to_lowercase();
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let has_mp3_sibling = mp3_stems.contains(&stem) && ext != "mp3";
            Some(MediaFile {
                path: f,
                name,
                size,
                has_mp3_sibling,
            })
        })
        .collect()
}

fn collect_media(dir: &Path, out: &mut Vec<String>, depth: u32) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_media(&p, out, depth + 1);
        } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if AUDIO_EXTS.contains(&ext.to_lowercase().as_str()) {
                out.push(p.to_string_lossy().to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_tmpdir_lives_next_to_output() {
        let anchor = std::env::temp_dir()
            .join("title [abc]")
            .join("title [abc].mp4");
        let dir = transcribe_tmpdir(&anchor);
        assert_eq!(
            dir.parent().unwrap(),
            std::env::temp_dir().join("title [abc]")
        );
        assert!(dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".ytw-tmp-"));
    }

    #[test]
    fn transcribe_tmpdir_falls_back_without_parent() {
        let dir = transcribe_tmpdir(Path::new("video.mp4"));
        assert_eq!(dir.parent().unwrap(), std::env::temp_dir());
    }

    #[test]
    fn temp_guard_removes_dir_and_files() {
        let base =
            std::env::temp_dir().join(format!("ytw-guard-test-{}", uuid::Uuid::new_v4().simple()));
        let dir = base.join(".ytw-tmp-x");
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("audio.wav");
        let srt = dir.join("audio.srt");
        std::fs::write(&wav, b"wav").unwrap();
        std::fs::write(&srt, b"srt").unwrap();
        {
            let _g = TempGuard(vec![dir.clone()]);
        }
        assert!(!dir.exists());
        assert!(!wav.exists());
        assert!(!srt.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn temp_guard_removes_plain_files() {
        let f = std::env::temp_dir().join(format!(
            "ytw-guard-file-{}.txt",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&f, b"x").unwrap();
        {
            let _g = TempGuard(vec![f.clone()]);
        }
        assert!(!f.exists());
    }
}
