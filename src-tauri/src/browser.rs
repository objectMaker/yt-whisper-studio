use crate::cookies::{self, Cookie};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, Window};

const LABEL: &str = "yt-browser";
const HOME: &str = "https://www.youtube.com/";

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieSummary {
    pub count: usize,
    pub logged_in: bool,
    pub names: Vec<String>,
}

fn find(window: &Window) -> Option<tauri::Webview> {
    window.webviews().into_iter().find(|w| w.label() == LABEL)
}

fn require(window: &Window) -> Result<tauri::Webview, String> {
    find(window).ok_or_else(|| "内置浏览器尚未创建".to_string())
}

/// 前端传的是页面视口坐标，而窗口级子 WebView 的原点是窗口外框左上角，
/// 需要补偿标题栏/边框的偏移，否则内嵌页面会盖住顶部工具栏。
fn window_offset(window: &Window) -> (f64, f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    match (window.outer_position(), window.inner_position()) {
        (Ok(outer), Ok(inner)) => (
            (inner.x - outer.x) as f64 / scale,
            (inner.y - outer.y) as f64 / scale,
        ),
        _ => (0.0, 0.0),
    }
}

fn place(window: &Window, wv: &tauri::Webview, bounds: Bounds) {
    let (ox, oy) = window_offset(window);
    let _ = wv.set_position(LogicalPosition::new(bounds.x + ox, bounds.y + oy));
    let _ = wv.set_size(LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)));
}

#[tauri::command]
pub async fn browser_show(window: Window, app: AppHandle, bounds: Bounds) -> Result<(), String> {
    let state = app.state::<crate::tasks::AppState>();
    let _guard = state.browser_lock.lock().unwrap();
    if let Some(wv) = find(&window) {
        place(&window, &wv, bounds);
        let _ = wv.show();
        return Ok(());
    }
    let url: tauri::Url = HOME.parse().map_err(|_| "无效地址".to_string())?;
    let app2 = app.clone();
    let (ox, oy) = window_offset(&window);
    window
        .add_child(
            WebviewBuilder::new(LABEL, WebviewUrl::External(url)).on_navigation(move |u| {
                let _ = app2.emit("yt-navigated", u.to_string());
                true
            }),
            LogicalPosition::new(bounds.x + ox, bounds.y + oy),
            LogicalSize::new(bounds.width.max(1.0), bounds.height.max(1.0)),
        )
        .map_err(|e| format!("创建内置浏览器失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn browser_hide(window: Window) -> Result<(), String> {
    if let Some(wv) = find(&window) {
        let _ = wv.hide();
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_set_bounds(window: Window, bounds: Bounds) -> Result<(), String> {
    if let Some(wv) = find(&window) {
        place(&window, &wv, bounds);
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_navigate(window: Window, url: String) -> Result<(), String> {
    let wv = require(&window)?;
    let parsed: tauri::Url = url.parse().map_err(|_| "无效地址".to_string())?;
    wv.navigate(parsed).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_back(window: Window) -> Result<(), String> {
    require(&window)?.eval("history.back()").map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_forward(window: Window) -> Result<(), String> {
    require(&window)?.eval("history.forward()").map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_reload(window: Window) -> Result<(), String> {
    require(&window)?
        .eval("location.reload()")
        .map_err(|e| e.to_string())
}

/// 切换离开浏览器页时暂停正在播放的媒体，避免声音继续在后播放。
#[tauri::command]
pub async fn browser_pause(window: Window) -> Result<(), String> {
    if let Some(wv) = find(&window) {
        wv.eval(
            "document.querySelectorAll('video,audio').forEach(function(m){try{m.pause()}catch(e){}})",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn browser_url(window: Window) -> Result<Option<String>, String> {
    Ok(find(&window)
        .and_then(|wv| wv.url().ok())
        .map(|u| u.to_string()))
}

#[tauri::command]
pub async fn extract_youtube_cookies(window: Window) -> Result<CookieSummary, String> {
    let wv = find(&window)
        .ok_or_else(|| "请先打开「YouTube 浏览器」并登录账号".to_string())?;
    let list: Vec<Cookie> = cookies::read_youtube_cookies(&wv)?;
    if cfg!(debug_assertions) {
        let p = std::env::temp_dir().join("ytw-cookies-debug.txt");
        let _ = std::fs::write(&p, cookies::to_netscape(&list));
    }
    let logged_in = list
        .iter()
        .any(|c| matches!(c.name.as_str(), "SID" | "SAPISID" | "__Secure-3PSID"));
    Ok(CookieSummary {
        count: list.len(),
        logged_in,
        names: list.iter().map(|c| c.name.clone()).take(40).collect(),
    })
}

// ---------------------------------------------------------------------------
// 控制 API 专用辅助（不影响桌面 GUI 原有交互）
// ---------------------------------------------------------------------------

/// 内置浏览器 webview 是否已创建（冷启动时为 false）。
pub fn is_ready(app: &tauri::AppHandle) -> bool {
    app.get_webview(LABEL).is_some()
}

/// 当前可读到的 YouTube Cookie 数量，用于健康检查展示。
pub fn cookie_count(app: &tauri::AppHandle) -> usize {
    app.get_webview(LABEL)
        .and_then(|wv| cookies::read_youtube_cookies(&wv).ok())
        .map(|list| list.len())
        .unwrap_or(0)
}

/// 冷启动时在主线程静默创建一个隐藏 webview，让 Cookie 会话开始加载。
/// 已经存在时直接返回 false，绝不改变用户正在使用的浏览器状态。
pub fn ensure_hidden(app: &tauri::AppHandle) -> Result<bool, String> {
    let app_for_main = app.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(ensure_hidden_inner(&app_for_main));
    })
    .map_err(|e| format!("提交主线程任务失败: {}", e))?;
    rx.recv_timeout(std::time::Duration::from_secs(20))
        .map_err(|_| "等待主线程创建浏览器超时".to_string())?
}

fn ensure_hidden_inner(app: &tauri::AppHandle) -> Result<bool, String> {
    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口尚未就绪".to_string())?;
    if find(&window).is_some() {
        return Ok(false);
    }
    let state = app.state::<crate::tasks::AppState>();
    let _guard = state.browser_lock.lock().unwrap();
    if find(&window).is_some() {
        return Ok(false);
    }
    let url: tauri::Url = HOME.parse().map_err(|_| "无效地址".to_string())?;
    let webview = window
        .add_child(
            WebviewBuilder::new(LABEL, WebviewUrl::External(url)),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(2.0, 2.0),
        )
        .map_err(|e| format!("创建内置浏览器失败: {e}"))?;
    let _ = webview.hide();
    Ok(true)
}
