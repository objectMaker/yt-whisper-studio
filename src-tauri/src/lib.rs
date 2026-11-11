mod browser;
mod cookies;
mod hardware;
mod net;
mod settings;
mod tasks;
mod tools;

use std::path::Path;
use tauri::{Emitter, Manager};
use tasks::{
    AppState, DownloadOptions, MediaFile, Task, TranscribeOptions,
};

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> settings::Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    settings: settings::Settings,
) -> Result<settings::Settings, String> {
    settings::save(&app, &settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

#[tauri::command]
async fn toolchain_status(app: tauri::AppHandle) -> tools::ToolchainStatus {
    tools::status(&app)
}

#[tauri::command]
async fn system_info() -> hardware::SystemInfo {
    hardware::system()
}

#[tauri::command]
async fn install_tool(app: tauri::AppHandle, id: String) -> Result<tools::ToolchainStatus, String> {
    tools::install_tool(app.clone(), id).await?;
    Ok(tools::status(&app))
}

#[tauri::command]
fn cancel_download(app: tauri::AppHandle, state: tauri::State<AppState>, kind: String, id: String) {
    if let Ok(mut set) = state.canceled_assets.lock() {
        set.insert(format!("{kind}:{id}"));
    }
    let _ = app.emit("asset-canceled", format!("{kind}:{id}"));
}

#[tauri::command]
async fn download_model(app: tauri::AppHandle, id: String) -> Result<tools::ToolchainStatus, String> {
    tools::ensure_model(&app, &id).await?;
    Ok(tools::status(&app))
}

#[tauri::command]
async fn delete_model(app: tauri::AppHandle, id: String) -> Result<tools::ToolchainStatus, String> {
    if let Some(path) = tools::model_path(&app, &id) {
        let _ = std::fs::remove_file(path);
    }
    Ok(tools::status(&app))
}

#[tauri::command]
fn list_tasks(state: tauri::State<AppState>) -> Vec<Task> {
    let map = state.tasks.lock().unwrap();
    let mut list: Vec<Task> = map.values().cloned().collect();
    list.sort_by_key(|t| t.created_at);
    list
}

#[tauri::command]
fn cancel_task(app: tauri::AppHandle, id: String) {
    tasks::cancel(&app, &id);
}

#[tauri::command]
fn remove_task(app: tauri::AppHandle, state: tauri::State<AppState>, id: String) {
    state.tasks.lock().unwrap().remove(&id);
    let _ = app.emit("task-removed", id);
}

#[tauri::command]
fn start_download(
    app: tauri::AppHandle,
    url: String,
    options: DownloadOptions,
) -> Result<Task, String> {
    tasks::start_download(app, url, options)
}

#[tauri::command]
fn start_transcribe(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: TranscribeOptions,
) -> Result<Vec<Task>, String> {
    tasks::start_transcribe(app, paths, options)
}

#[tauri::command]
async fn scan_folder(folder: String) -> Result<Vec<MediaFile>, String> {
    tauri::async_runtime::spawn_blocking(move || tasks::scan_folder(Path::new(&folder)))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let settings = settings::load(app.handle());
            app.manage(AppState::new(settings));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            toolchain_status,
            system_info,
            install_tool,
            cancel_download,
            download_model,
            delete_model,
            list_tasks,
            cancel_task,
            remove_task,
            start_download,
            start_transcribe,
            scan_folder,
            open_path,
            browser::browser_show,
            browser::browser_hide,
            browser::browser_set_bounds,
            browser::browser_navigate,
            browser::browser_back,
            browser::browser_forward,
            browser::browser_reload,
            browser::browser_pause,
            browser::browser_url,
            browser::extract_youtube_cookies,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
