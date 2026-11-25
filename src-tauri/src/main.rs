#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn ensure_webview2_proxy_bypass() {
    const REQUIRED_HOSTS: &[&str] = &["<local>", "*.localhost", "tauri.localhost", "127.0.0.1", "localhost"];
    let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    if current_args.is_empty() {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            format!("--proxy-bypass-list={}", REQUIRED_HOSTS.join(";")),
        );
        return;
    }
    if let Some(pos) = current_args.find("--proxy-bypass-list=") {
        let prefix_end = pos + "--proxy-bypass-list=".len();
        let rest = &current_args[prefix_end..];
        let val_end = rest.find(char::is_whitespace).map(|i| prefix_end + i).unwrap_or(current_args.len());
        let existing = &current_args[prefix_end..val_end];
        let mut merged: Vec<String> = existing
            .split(';')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        for host in REQUIRED_HOSTS {
            if !merged.iter().any(|h| h.eq_ignore_ascii_case(host)) {
                merged.push((*host).to_string());
            }
        }
        let updated = format!(
            "{}--proxy-bypass-list={}{}",
            &current_args[..pos],
            merged.join(";"),
            &current_args[val_end..]
        );
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", updated);
    } else {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            format!("{} --proxy-bypass-list={}", current_args, REQUIRED_HOSTS.join(";")),
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
