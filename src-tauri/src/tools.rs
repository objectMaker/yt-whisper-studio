use futures_util::StreamExt;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct ModelDef {
    pub id: &'static str,
    pub file: &'static str,
    pub label: &'static str,
    pub bytes: u64,
}

pub const MODELS: &[ModelDef] = &[
    ModelDef { id: "tiny.en", file: "ggml-tiny.en.bin", label: "Tiny (英文 · 最快)", bytes: 77_704_715 },
    ModelDef { id: "base.en", file: "ggml-base.en.bin", label: "Base (英文 · 推荐)", bytes: 147_964_211 },
    ModelDef { id: "small.en", file: "ggml-small.en.bin", label: "Small (英文 · 均衡)", bytes: 488_049_691 },
    ModelDef { id: "medium.en", file: "ggml-medium.en.bin", label: "Medium (英文 · 精确)", bytes: 1_533_774_784 },
    ModelDef { id: "small", file: "ggml-small.bin", label: "Small (多语言 · 轻量)", bytes: 487_601_967 },
    ModelDef { id: "medium", file: "ggml-medium.bin", label: "Medium (多语言 · 均衡)", bytes: 1_533_763_059 },
    ModelDef { id: "large-v3-turbo", file: "ggml-large-v3-turbo.bin", label: "Large v3 Turbo (多语言 · 最准)", bytes: 1_624_555_275 },
    ModelDef { id: "large-v3-turbo-q5_0", file: "ggml-large-v3-turbo-q5_0.bin", label: "Large v3 Turbo Q5 (多语言 · 快且省)", bytes: 574_041_195 },
];

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub file: String,
    pub size: u64,
    pub installed: bool,
    pub path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub yt_dlp: ToolInfo,
    pub ffmpeg: ToolInfo,
    pub whisper: ToolInfo,
    pub whisper_cuda: ToolInfo,
    pub whisper_vulkan: ToolInfo,
    pub models: Vec<ModelInfo>,
    pub bin_dir: String,
    pub models_dir: String,
    pub ready: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AssetProgress {
    pub kind: String,
    pub id: String,
    pub percent: f64,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub message: String,
}

pub fn app_data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn bin_dir(app: &AppHandle) -> PathBuf {
    app_data_dir(app).join("bin")
}

pub fn whisper_dir(app: &AppHandle) -> PathBuf {
    bin_dir(app).join("whisper")
}

/// 不同加速后端各自一个目录，互不覆盖
pub fn whisper_dir_for(app: &AppHandle, backend: &str) -> PathBuf {
    match backend {
        "cuda" => bin_dir(app).join("whisper-cuda"),
        "vulkan" => bin_dir(app).join("whisper-vulkan"),
        _ => whisper_dir(app),
    }
}

pub fn models_dir(app: &AppHandle) -> PathBuf {
    app_data_dir(app).join("models")
}

pub fn model_path(app: &AppHandle, id: &str) -> Option<PathBuf> {
    MODELS
        .iter()
        .find(|m| m.id == id)
        .map(|m| models_dir(app).join(m.file))
}

pub fn resolve_exe(app: &AppHandle, id: &str) -> Option<PathBuf> {
    let local = match id {
        "yt-dlp" => bin_dir(app).join("yt-dlp.exe"),
        "ffmpeg" => bin_dir(app).join("ffmpeg.exe"),
        "whisper" => whisper_dir(app).join("whisper-cli.exe"),
        "whisper-cuda" => whisper_dir_for(app, "cuda").join("whisper-cli.exe"),
        "whisper-vulkan" => whisper_dir_for(app, "vulkan").join("whisper-cli.exe"),
        _ => return None,
    };
    if local.exists() {
        return Some(local);
    }
    let name = match id {
        "yt-dlp" => "yt-dlp",
        "ffmpeg" => "ffmpeg",
        _ => "whisper-cli",
    };
    which::which(name).ok()
}

fn probe_version(id: &str, path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    match id {
        "yt-dlp" => {
            cmd.arg("--version");
        }
        "ffmpeg" => {
            cmd.arg("-version");
        }
        _ => {
            cmd.arg("--help");
        }
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&out.stderr).to_string();
    }
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let mut v = line.trim().to_string();
    if v.len() > 90 {
        v.truncate(90);
    }
    Some(v)
}

fn tool_info(app: &AppHandle, id: &str, name: &str) -> ToolInfo {
    let path = resolve_exe(app, id);
    let local = match id {
        "yt-dlp" => bin_dir(app).join("yt-dlp.exe"),
        "ffmpeg" => bin_dir(app).join("ffmpeg.exe"),
        "whisper" => whisper_dir(app).join("whisper-cli.exe"),
        "whisper-cuda" => whisper_dir_for(app, "cuda").join("whisper-cli.exe"),
        "whisper-vulkan" => whisper_dir_for(app, "vulkan").join("whisper-cli.exe"),
        _ => PathBuf::new(),
    };
    let source = path.as_ref().map(|p| {
        if p == &local {
            "app".to_string()
        } else {
            "system".to_string()
        }
    });
    let version = path.as_deref().and_then(|p| probe_version(id, p));
    ToolInfo {
        id: id.to_string(),
        name: name.to_string(),
        path: path.map(|p| p.to_string_lossy().to_string()),
        version,
        source,
    }
}

pub fn status(app: &AppHandle) -> ToolchainStatus {
    let yt_dlp = tool_info(app, "yt-dlp", "yt-dlp");
    let ffmpeg = tool_info(app, "ffmpeg", "ffmpeg");
    let whisper = tool_info(app, "whisper", "whisper.cpp (CPU)");
    let whisper_cuda = tool_info(app, "whisper-cuda", "whisper.cpp (NVIDIA CUDA)");
    let whisper_vulkan = tool_info(app, "whisper-vulkan", "whisper.cpp (AMD/Intel Vulkan)");
    let models = MODELS
        .iter()
        .map(|m| {
            let path = models_dir(app).join(m.file);
            ModelInfo {
                id: m.id.to_string(),
                label: m.label.to_string(),
                file: m.file.to_string(),
                size: m.bytes,
                installed: path.exists(),
                path: path.to_string_lossy().to_string(),
            }
        })
        .collect::<Vec<_>>();
    let ready = yt_dlp.path.is_some() && ffmpeg.path.is_some() && whisper.path.is_some();
    ToolchainStatus {
        yt_dlp,
        ffmpeg,
        whisper,
        whisper_cuda,
        whisper_vulkan,
        models,
        bin_dir: bin_dir(app).to_string_lossy().to_string(),
        models_dir: models_dir(app).to_string_lossy().to_string(),
        ready,
    }
}

fn asset_cancel_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

fn is_asset_canceled(app: &AppHandle, kind: &str, id: &str) -> bool {
    app.state::<crate::tasks::AppState>()
        .canceled_assets
        .lock()
        .map(|set| set.contains(&asset_cancel_key(kind, id)))
        .unwrap_or(false)
}

fn clear_asset_cancel(app: &AppHandle, kind: &str, id: &str) {
    if let Ok(mut set) = app.state::<crate::tasks::AppState>().canceled_assets.lock() {
        set.remove(&asset_cancel_key(kind, id));
    }
}

async fn download_to(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    kind: &str,
    id: &str,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("download");
    let mut builder = reqwest::Client::builder()
        .user_agent("YT-Whisper-Studio/0.1")
        .redirect(reqwest::redirect::Policy::limited(12));
    if let Some(proxy) = crate::net::system_proxy() {
        if let Ok(p) = reqwest::Proxy::all(&proxy) {
            builder = builder.proxy(p);
        }
    }
    let client = builder.build().map_err(|e| e.to_string())?;

    // 断点续传：保留 .download 文件，重试时从已有大小继续
    let mut resume_from = fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = req.send().await.map_err(|e| format!("下载失败: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = fs::remove_file(&tmp);
        return Err("断点位置无效，已重置下载".into());
    }
    if !status.is_success() {
        return Err(format!("下载失败: HTTP {status}"));
    }
    let partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !partial {
        resume_from = 0;
    }
    let total = resp.content_length().unwrap_or(0) + resume_from;
    let mut file = if resume_from > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp)
            .await
            .map_err(|e| e.to_string())?
    } else {
        tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| e.to_string())?
    };
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = resume_from;
    let started = Instant::now();
    let mut last_emit = Instant::now() - Duration::from_secs(2);
    while let Some(chunk) = stream.next().await {
        if is_asset_canceled(app, kind, id) {
            let _ = file.flush().await;
            let _ = app.emit(
                "asset-progress",
                AssetProgress {
                    kind: kind.to_string(),
                    id: id.to_string(),
                    percent: if total > 0 {
                        downloaded as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    },
                    downloaded,
                    total,
                    speed: 0,
                    message: "已暂停".into(),
                },
            );
            return Err("已暂停，可点击继续".into());
        }
        let chunk = chunk.map_err(|e| format!("下载中断: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(200) {
            last_emit = Instant::now();
            let secs = started.elapsed().as_secs_f64().max(0.001);
            let _ = app.emit(
                "asset-progress",
                AssetProgress {
                    kind: kind.to_string(),
                    id: id.to_string(),
                    percent: if total > 0 {
                        downloaded as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    },
                    downloaded,
                    total,
                    speed: (downloaded.saturating_sub(resume_from) as f64 / secs) as u64,
                    message: String::new(),
                },
            );
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "asset-progress",
        AssetProgress {
            kind: kind.to_string(),
            id: id.to_string(),
            percent: 100.0,
            downloaded,
            total: if total > 0 { total } else { downloaded },
            speed: 0,
            message: String::new(),
        },
    );
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解压失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file_recursive(&p, name) {
                return Some(found);
            }
        } else if p.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

pub async fn install_tool(app: AppHandle, id: String) -> Result<(), String> {
    let bin = bin_dir(&app);
    tokio::fs::create_dir_all(&bin)
        .await
        .map_err(|e| e.to_string())?;
    match id.as_str() {
        "yt-dlp" => {
            clear_asset_cancel(&app, "tool", "yt-dlp");
            let dest = bin.join("yt-dlp.exe");
            let mut last_err = String::new();
            for url in crate::net::github_candidates(
                "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
            ) {
                match download_to(&app, &url, &dest, "tool", "yt-dlp").await {
                    Ok(()) => return Ok(()),
                    Err(e) if is_asset_canceled(&app, "tool", "yt-dlp") => return Err(e),
                    Err(e) => last_err = e,
                }
            }
            return Err(last_err);
        }
        "ffmpeg" => {
            clear_asset_cancel(&app, "tool", "ffmpeg");
            let zip_path = bin.join("ffmpeg.zip");
            let base_urls = [
                "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
                "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
            ];
            let mut last_err = String::new();
            let mut ok = false;
            for base in base_urls {
                for url in crate::net::github_candidates(base) {
                    match download_to(&app, &url, &zip_path, "tool", "ffmpeg").await {
                        Ok(()) => {
                            ok = true;
                            break;
                        }
                        Err(e) if is_asset_canceled(&app, "tool", "ffmpeg") => return Err(e),
                        Err(e) => last_err = e,
                    }
                }
                if ok {
                    break;
                }
            }
            if !ok {
                return Err(last_err);
            }
            let tmp = bin.join("_ffmpeg_tmp");
            let _ = fs::remove_dir_all(&tmp);
            let zip_clone = zip_path.clone();
            let tmp_clone = tmp.clone();
            tokio::task::spawn_blocking(move || extract_zip(&zip_clone, &tmp_clone))
                .await
                .map_err(|e| e.to_string())??;
            let source = find_file_recursive(&tmp, "ffmpeg.exe")
                .ok_or_else(|| "压缩包中未找到 ffmpeg.exe".to_string())?;
            fs::copy(&source, bin.join("ffmpeg.exe")).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&zip_path);
            let _ = fs::remove_dir_all(&tmp);
        }
        "whisper" => {
            let urls = [
                "https://github.com/ggml-org/whisper.cpp/releases/latest/download/whisper-bin-x64.zip",
                "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-x64.zip",
                "https://github.com/ggml-org/whisper.cpp/releases/download/v1.7.6/whisper-bin-x64.zip",
            ];
            install_whisper_zip(&app, "whisper", &urls, whisper_dir(&app)).await?;
        }
        "whisper-cuda" => {
            let urls = [
                "https://github.com/ggml-org/whisper.cpp/releases/download/b5454/whisper-bin-win-cuda-12.4.0-x64.zip",
                "https://github.com/ggml-org/whisper.cpp/releases/download/v1.7.6/whisper-cublas-12.4.0-bin-x64.zip",
            ];
            install_whisper_zip(&app, "whisper-cuda", &urls, whisper_dir_for(&app, "cuda")).await?;
        }
        "whisper-vulkan" => {
            install_whisper_vulkan(&app).await?;
        }
        other => return Err(format!("未知工具: {other}")),
    }
    Ok(())
}

async fn install_whisper_zip(
    app: &AppHandle,
    id: &str,
    urls: &[&str],
    target: PathBuf,
) -> Result<(), String> {
    clear_asset_cancel(app, "tool", id);
    let zip_path = bin_dir(app).join(format!("{id}.zip"));
    let mut last_err = String::new();
    let mut ok = false;
    for base in urls {
        for url in crate::net::github_candidates(base) {
            match download_to(app, &url, &zip_path, "tool", id).await {
                Ok(()) => {
                    ok = true;
                    break;
                }
                Err(e) if is_asset_canceled(app, "tool", id) => return Err(e),
                Err(e) => last_err = e,
            }
        }
        if ok {
            break;
        }
    }
    if !ok {
        return Err(last_err);
    }
    let result = extract_whisper_zip(&zip_path, &target);
    let _ = fs::remove_file(&zip_path);
    result
}

fn extract_whisper_zip(zip_path: &Path, target: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(target);
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    extract_zip(zip_path, target)?;
    if !target.join("whisper-cli.exe").exists() {
        // 某些版本解压后多一层目录，把内的文件拍平
        if let Some(found) = find_file_recursive(target, "whisper-cli.exe") {
            if let Some(parent) = found.parent() {
                for entry in fs::read_dir(parent).map_err(|e| e.to_string())?.flatten() {
                    let _ = fs::copy(entry.path(), target.join(entry.file_name()));
                }
            }
        }
    }
    if !target.join("whisper-cli.exe").exists() {
        return Err("whisper.cpp 安装不完整（缺少 whisper-cli.exe）".into());
    }
    Ok(())
}

/// Vulkan 版没有官方预编译包，由项目内置（resources/whisper-vulkan.zip）
async fn install_whisper_vulkan(app: &AppHandle) -> Result<(), String> {
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("resources").join("whisper-vulkan.zip"));
        candidates.push(dir.join("whisper-vulkan.zip"));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("whisper-vulkan.zip"),
    );
    let zip = candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "当前安装包未内置 Vulkan 加速组件".to_string())?;
    let target = whisper_dir_for(app, "vulkan");
    tokio::task::spawn_blocking(move || extract_whisper_zip(&zip, &target))
        .await
        .map_err(|e| e.to_string())?
}

pub async fn ensure_model(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let def = MODELS
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| format!("未知模型: {id}"))?;
    let dir = models_dir(app);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let dest = dir.join(def.file);
    if dest.exists() {
        return Ok(dest);
    }
    clear_asset_cancel(app, "model", id);
    let urls = crate::net::model_candidates(&format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        def.file
    ));
    let mut last_err = String::new();
    for url in urls {
        match download_to(app, &url, &dest, "model", id).await {
            Ok(()) => {
                if fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) > 1_000_000 {
                    return Ok(dest);
                }
                last_err = "模型文件不完整".to_string();
                let _ = fs::remove_file(&dest);
            }
            Err(e) if is_asset_canceled(app, "model", id) => return Err(e),
            Err(e) => last_err = e,
        }
    }
    Err(format!("模型下载失败: {last_err}"))
}
