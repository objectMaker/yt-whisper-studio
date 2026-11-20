#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn ensure_webview2_proxy_bypass() {
    let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    let bypass_arg = "--proxy-bypass-list=<-loopback>;<local>;*.localhost;tauri.localhost;127.0.0.1;localhost";
    if current_args.is_empty() {
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", bypass_arg);
    } else if !current_args.contains("--proxy-bypass-list") {
        std::env::set_var(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            format!("{} {}", current_args, bypass_arg),
        );
    }
}

fn main() {
    #[cfg(windows)]
    ensure_webview2_proxy_bypass();

    yt_whisper_studio_lib::run()
}
