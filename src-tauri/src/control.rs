//! 本地控制 API：给 echo-pipeline 提供受保护的下载/转写/进度/浏览器唤醒入口。
//!
//! 安全边界：
//! - 只绑定 127.0.0.1；
//! - 所有请求必须携带 `X-Control-Token`（与 echo-pipeline 共享同一份本地 token 文件）；
//! - Host 必须是回环地址，防止 DNS Rebinding；
//! - 不修改任何原有业务逻辑，只调用 tasks / browser 已有函数。

use std::io::Read;
use std::path::PathBuf;
use std::thread;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::tasks::{self, AppState, DownloadOptions, TranscribeOptions};

const DEFAULT_PORT: u16 = 4517;
const MAX_BODY_BYTES: u64 = 1024 * 1024;

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let port = std::env::var("YTW_CONTROL_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let addr = format!("127.0.0.1:{}", port);
        let server = match Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[control] 控制 API 绑定 {} 失败（不影响桌面 GUI）：{}", addr, e);
                return;
            }
        };
        println!("[control] 本地控制 API 已启动: http://{}", addr);

        for request in server.incoming_requests() {
            handle(request, &app);
        }
    });
}

fn handle(mut request: Request, app: &AppHandle) {
    if !host_allowed(&request) {
        respond_error(request, 403, "forbidden host");
        return;
    }
    if !token_valid(&request) {
        respond_error(request, 401, "invalid control token");
        return;
    }

    let method = request.method().clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or(&url).to_string();

    if method == Method::Get && path == "/health" {
        respond_ok(request, health_payload(app));
        return;
    }

    if method == Method::Get && path == "/browser/status" {
        let ready = crate::browser::is_ready(app);
        respond_ok(
            request,
            json!({
                "cookiesReady": ready,
                "cookieCount": if ready { crate::browser::cookie_count(app) } else { 0 }
            }),
        );
        return;
    }

    if method == Method::Post && path == "/browser/ensure" {
        match crate::browser::ensure_hidden(app) {
            Ok(created) => respond_ok(
                request,
                json!({
                    "created": created,
                    "cookiesReady": crate::browser::is_ready(app),
                    "cookieCount": crate::browser::cookie_count(app)
                }),
            ),
            Err(e) => respond_error(request, 500, &e),
        }
        return;
    }

    if method == Method::Post && path == "/download" {
        let body: DownloadReq = match read_json(&mut request) {
            Ok(v) => v,
            Err(e) => return respond_error(request, 400, &e),
        };
        if !is_youtube_url(&body.url) {
            return respond_error(request, 400, "仅允许 youtube.com / youtu.be 链接");
        }
        let options = DownloadOptions {
            extract_mp3: body.extract_mp3,
            transcribe: body.transcribe,
            quality: body.quality,
            model: body.model,
        };
        match tasks::start_download(app.clone(), body.url, options) {
            Ok(task) => respond_ok(request, json!({ "task_id": task.id })),
            Err(e) => respond_error(request, 500, &e),
        }
        return;
    }

    if method == Method::Post && path == "/transcribe" {
        let body: TranscribeReq = match read_json(&mut request) {
            Ok(v) => v,
            Err(e) => return respond_error(request, 400, &e),
        };
        if body.paths.is_empty() {
            return respond_error(request, 400, "paths 不能为空");
        }
        let options = TranscribeOptions {
            model: body.model,
            language: body.language,
        };
        match tasks::start_transcribe(app.clone(), body.paths, options) {
            Ok(created) if !created.is_empty() => {
                let ids: Vec<String> = created.iter().map(|t| t.id.clone()).collect();
                respond_ok(
                    request,
                    json!({ "task_id": ids[0], "task_ids": ids }),
                );
            }
            Ok(_) => respond_error(request, 400, "没有可转写的文件"),
            Err(e) => respond_error(request, 500, &e),
        }
        return;
    }

    if method == Method::Get && path == "/tasks" {
        let state = app.state::<AppState>();
        let mut list: Vec<Value> = {
            let map = state.tasks.lock().unwrap();
            map.values().cloned().map(|t| task_json(&t)).collect()
        };
        list.sort_by_key(|v| {
            v.get("createdAt")
                .and_then(|c| c.as_u64())
                .unwrap_or_default()
        });
        respond_ok(request, json!(list));
        return;
    }

    if method == Method::Get && path.starts_with("/tasks/") {
        let id = path.trim_start_matches("/tasks/");
        let state = app.state::<AppState>();
        let found = {
            let map = state.tasks.lock().unwrap();
            map.get(id).cloned()
        };
        match found {
            Some(task) => respond_ok(request, task_json(&task)),
            None => respond_error(request, 404, "task not found"),
        }
        return;
    }

    if method == Method::Post && path.starts_with("/tasks/") && path.ends_with("/cancel") {
        let id = path
            .trim_start_matches("/tasks/")
            .trim_end_matches("/cancel")
            .trim_end_matches('/')
            .to_string();
        tasks::cancel(app, &id);
        respond_ok(request, json!({ "canceled": true }));
        return;
    }

    respond_error(request, 404, "not found");
}

fn health_payload(app: &AppHandle) -> Value {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    let task_count = state.tasks.lock().unwrap().len();
    let ready = crate::browser::is_ready(app);

    json!({
        "ok": true,
        "toolchain": crate::tools::status(app),
        "settings": {
            "outputDir": settings.output_dir,
            "language": settings.language,
            "model": settings.model
        },
        "taskCount": task_count,
        "cookiesReady": ready,
        "cookieCount": if ready { crate::browser::cookie_count(app) } else { 0 }
    })
}

fn task_json(task: &tasks::Task) -> Value {
    let mut val = serde_json::to_value(task).unwrap_or(Value::Null);
    if let Some(log) = val.get_mut("log").and_then(|v| v.as_array_mut()) {
        let keep_from = log.len().saturating_sub(100);
        log.drain(0..keep_from);
    }
    val
}

#[derive(Deserialize)]
struct DownloadReq {
    url: String,
    #[serde(default)]
    extract_mp3: Option<bool>,
    #[serde(default)]
    transcribe: Option<bool>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct TranscribeReq {
    paths: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    language: Option<String>,
}

fn is_youtube_url(url: &str) -> bool {
    let Ok(parsed) = tauri::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return false;
    }
    match parsed.host_str().map(|h| h.to_lowercase()) {
        Some(host) => {
            host == "youtube.com"
                || host.ends_with(".youtube.com")
                || host == "youtu.be"
                || host.ends_with(".youtu.be")
        }
        None => false,
    }
}

fn host_allowed(request: &Request) -> bool {
    header_value(request, "Host")
        .map(|h| {
            let host = h.to_lowercase();
            host == "127.0.0.1"
                || host.starts_with("127.0.0.1:")
                || host == "localhost"
                || host.starts_with("localhost:")
        })
        .unwrap_or(false)
}

fn token_valid(request: &Request) -> bool {
    let expected = load_or_create_token();
    match header_value(request, "X-Control-Token") {
        Some(actual) => constant_time_eq(actual.as_bytes(), expected.as_bytes()),
        None => false,
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn read_json<T: DeserializeOwned>(request: &mut Request) -> Result<T, String> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut body)
        .map_err(|e| format!("读取请求体失败: {}", e))?;
    serde_json::from_str(&body).map_err(|e| format!("请求体不是合法 JSON: {}", e))
}

fn respond_ok(request: Request, data: Value) {
    respond(request, 200, json!({ "ok": true, "data": data }));
}

fn respond_error(request: Request, status: u16, message: &str) {
    respond(request, status, json!({ "ok": false, "error": message }));
}

fn respond(request: Request, status: u16, body: Value) {
    let text = serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string());
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..])
        .expect("static header");
    let response = Response::from_string(text)
        .with_status_code(StatusCode(status))
        .with_header(header);
    let _ = request.respond(response);
}

/// 与 echo-pipeline 共享的本地 token：谁先启动谁生成，另一方读取同一文件。
fn load_or_create_token() -> String {
    let path = token_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        let token = text.trim();
        if !token.is_empty() {
            return token.to_string();
        }
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &token);
    token
}

fn token_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("echo-pipeline")
        .join("ytw_token.key")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_url_whitelist() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
        assert!(!is_youtube_url("https://evil.com/youtube.com/"));
        assert!(!is_youtube_url("file:///youtube.com/x"));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
