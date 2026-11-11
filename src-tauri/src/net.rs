use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 读取 Windows 系统代理（IE/Edge 设置），让下载与 yt-dlp 与浏览器走同一条线路。
pub fn system_proxy() -> Option<String> {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("reg");
        cmd.args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let out = cmd.output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut enabled = false;
        let mut server = String::new();
        for line in text.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 3 && cols[0] == "ProxyEnable" {
                enabled = cols[cols.len() - 1].eq_ignore_ascii_case("0x1");
            } else if cols.len() >= 3 && cols[0] == "ProxyServer" {
                server = cols[2..].join("");
            }
        }
        if !enabled || server.is_empty() {
            return None;
        }
        let raw = if server.contains('=') {
            server
                .split(';')
                .filter_map(|p| p.split_once('='))
                .find(|(k, _)| k.eq_ignore_ascii_case("https") || k.eq_ignore_ascii_case("http"))
                .map(|(_, v)| v.to_string())?
        } else {
            server
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        if raw.contains("://") {
            Some(raw.to_string())
        } else {
            Some(format!("http://{raw}"))
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn is_github(url: &str) -> bool {
    url.contains("github.com") || url.contains("githubusercontent.com")
}

/// 为 GitHub 资源生成候选地址：国内直连经常失败或极慢，优先走加速镜像。
pub fn github_candidates(url: &str) -> Vec<String> {
    if !is_github(url) {
        return vec![url.to_string()];
    }
    let mirrored = vec![
        format!("https://gh-proxy.com/{url}"),
        format!("https://ghfast.top/{url}"),
    ];
    let mut out = Vec::new();
    if system_proxy().is_some() {
        // 已有代理时直连更快
        out.push(url.to_string());
        out.extend(mirrored);
    } else {
        out.extend(mirrored);
        out.push(url.to_string());
    }
    out
}

/// Hugging Face 模型候选地址：优先国内镜像，再回退官方。
pub fn model_candidates(url: &str) -> Vec<String> {
    let mirror = url.replace("https://huggingface.co", "https://hf-mirror.com");
    let mut out = Vec::new();
    if mirror != url {
        out.push(mirror);
    }
    out.push(url.to_string());
    out
}
