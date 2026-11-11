use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub output_dir: String,
    pub quality: String,
    pub extract_mp3: bool,
    pub transcribe: bool,
    pub language: String,
    pub model: String,
    pub open_folder: bool,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    #[serde(default = "default_true")]
    pub use_embedded_cookies: bool,
    #[serde(default)]
    pub cookies_file: String,
    #[serde(default = "default_true")]
    pub auto_download_model: bool,
    /// 推理设备：auto / cpu / gpu
    #[serde(default = "default_compute")]
    pub compute: String,
}

fn default_compute() -> String {
    "auto".into()
}

fn default_concurrency() -> u32 {
    2
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        let output_dir = dirs::video_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("YT-Whisper")
            .to_string_lossy()
            .to_string();
        Self {
            output_dir,
            quality: "best".into(),
            extract_mp3: true,
            transcribe: true,
            language: "en".into(),
            model: "base.en".into(),
            open_folder: false,
            concurrency: default_concurrency(),
            use_embedded_cookies: true,
            cookies_file: String::new(),
            auto_download_model: true,
            compute: default_compute(),
        }
    }
}

pub fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &tauri::AppHandle) -> Settings {
    let defaults = Settings::default();
    let Ok(path) = settings_path(app) else {
        return defaults;
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        let _ = save(app, &defaults);
        return defaults;
    };
    // 兼容带 BOM 的文件
    let cleaned = raw.trim_start_matches('\u{feff}');
    serde_json::from_str::<Settings>(cleaned).unwrap_or(defaults)
}

pub fn save(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| format!("保存设置失败: {e}"))
}
