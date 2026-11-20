#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn ensure_webview2_proxy_bypass() {
    let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    let bypass_arg = "--proxy-bypass-list=<local>;*.localhost;tauri.localhost;127.0.0.1;localhost";
    if current_args.is_empty() {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", bypass_arg);
    } else if !current_args.contains("--proxy-bypass-list") {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            format!("{} {}", current_args, bypass_arg),
        );
    }
}

#[cfg(windows)]
fn register_windows_environment(app_name: &str) {
    use std::os::windows::process::CommandExt;
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_str = exe_path.to_string_lossy().to_string();
            let dir_str = exe_dir.to_string_lossy().to_string();

            // 1. 注册 Windows App Paths（供外部程序/命令行秒级直接定位 exe）
            let key = format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\App Paths\{}.exe", app_name);
            let _ = std::process::Command::new("reg")
                .args(["add", &key, "/ve", "/t", "REG_SZ", "/d", &exe_str, "/f"])
                .creation_flags(0x08000000)
                .spawn();
            let _ = std::process::Command::new("reg")
                .args(["add", &key, "/v", "Path", "/t", "REG_SZ", "/d", &dir_str, "/f"])
                .creation_flags(0x08000000)
                .spawn();

            // 2. 检查并追加当前安装目录到当前用户 PATH（若尚未包含）
            std::thread::spawn(move || {
                let check_output = std::process::Command::new("reg")
                    .args(["query", r"HKCU\Environment", "/v", "Path"])
                    .creation_flags(0x08000000)
                    .output();
                if let Ok(out) = check_output {
                    let text = String::from_utf8_lossy(&out.stdout);
                    if !text.to_lowercase().contains(&dir_str.to_lowercase()) {
                        let append_script = format!(
                            "$cur = [System.Environment]::GetEnvironmentVariable('Path', 'User'); if ($cur -notlike '*{0}*') {{ [System.Environment]::SetEnvironmentVariable('Path', $cur + ';{0}', 'User') }}",
                            dir_str.replace("'", "''")
                        );
                        let _ = std::process::Command::new("powershell")
                            .args(["-NoProfile", "-NonInteractive", "-Command", &append_script])
                            .creation_flags(0x08000000)
                            .output();
                    }
                }
            });
        }
    }
}

fn main() {
    #[cfg(windows)]
    {
        ensure_webview2_proxy_bypass();
        register_windows_environment("yt-whisper-studio");
    }

    yt_whisper_studio_lib::run()
}
