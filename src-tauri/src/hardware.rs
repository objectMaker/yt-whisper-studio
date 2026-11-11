use serde::Serialize;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub vram: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub cpu: String,
    pub cores: u32,
    pub threads: u32,
    pub ram: u64,
    pub gpus: Vec<GpuInfo>,
    /// 推荐后端：cuda / vulkan / cpu
    pub recommended: String,
    pub cuda_available: bool,
    pub vulkan_available: bool,
}

#[derive(serde::Deserialize)]
struct RawGpu {
    name: Option<String>,
    #[serde(default)]
    vram: u64,
}

#[derive(serde::Deserialize)]
struct RawInfo {
    cpu: Option<String>,
    #[serde(default)]
    cores: u32,
    #[serde(default)]
    threads: u32,
    #[serde(default)]
    ram: u64,
    #[serde(default)]
    gpus: Vec<RawGpu>,
}

const PS_SCRIPT: &str = r#"
[Console]::OutputEncoding=[Text.Encoding]::UTF8
$ErrorActionPreference='SilentlyContinue'
$cpu=Get-CimInstance Win32_Processor | Select-Object -First 1
$cs=Get-CimInstance Win32_ComputerSystem
$gpus=@()
Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' | ForEach-Object {
  $p=Get-ItemProperty $_.PSPath
  if($p.DriverDesc){
    $v=0
    if($p.'HardwareInformation.qwMemorySize'){ $v=[uint64]$p.'HardwareInformation.qwMemorySize' }
    $gpus += [pscustomobject]@{name=$p.DriverDesc; vram=$v}
  }
}
[pscustomobject]@{cpu=$cpu.Name; cores=$cpu.NumberOfCores; threads=$cpu.NumberOfLogicalProcessors; ram=[uint64]$cs.TotalPhysicalMemory; gpus=$gpus} | ConvertTo-Json -Depth 4 -Compress
"#;

fn vendor_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("nvidia") || n.contains("geforce") || n.contains("rtx") || n.contains("gtx") {
        "nvidia"
    } else if n.contains("amd") || n.contains("radeon") || n.contains("ati ") {
        "amd"
    } else if n.contains("intel") {
        "intel"
    } else if n.contains("microsoft") || n.contains("basic") {
        "virtual"
    } else {
        "other"
    }
}

fn query() -> SystemInfo {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", PS_SCRIPT])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let fallback = || SystemInfo {
        cpu: String::new(),
        cores: 0,
        threads: std::thread::available_parallelism()
            .map(|v| v.get() as u32)
            .unwrap_or(4),
        ram: 0,
        gpus: Vec::new(),
        recommended: "cpu".into(),
        cuda_available: false,
        vulkan_available: cfg!(windows)
            && std::path::Path::new(r"C:\Windows\System32\vulkan-1.dll").exists(),
    };
    let Ok(out) = cmd.output() else {
        return fallback();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(start) = text.find('{') else {
        return fallback();
    };
    let json = &text[start..];
    let Ok(raw) = serde_json::from_str::<RawInfo>(json) else {
        return fallback();
    };
    let gpus: Vec<GpuInfo> = raw
        .gpus
        .into_iter()
        .filter_map(|g| {
            let name = g.name.unwrap_or_default();
            if name.trim().is_empty() {
                return None;
            }
            let vendor = vendor_of(&name);
            if vendor == "virtual" {
                return None;
            }
            Some(GpuInfo {
                name,
                vendor: vendor.to_string(),
                vram: g.vram,
            })
        })
        .collect();
    let cuda_available = gpus.iter().any(|g| g.vendor == "nvidia" && g.vram > 1_500_000_000);
    let vulkan_available = cfg!(windows)
        && std::path::Path::new(r"C:\Windows\System32\vulkan-1.dll").exists()
        && gpus
            .iter()
            .any(|g| g.vendor == "amd" || g.vendor == "intel" || g.vendor == "nvidia");
    let recommended = if cuda_available {
        "cuda"
    } else if vulkan_available && gpus.iter().any(|g| g.vendor == "amd") {
        "vulkan"
    } else {
        "cpu"
    };
    SystemInfo {
        cpu: raw.cpu.unwrap_or_default(),
        cores: raw.cores,
        threads: if raw.threads > 0 {
            raw.threads
        } else {
            std::thread::available_parallelism()
                .map(|v| v.get() as u32)
                .unwrap_or(4)
        },
        ram: raw.ram,
        gpus,
        recommended: recommended.into(),
        cuda_available,
        vulkan_available,
    }
}

pub fn system() -> SystemInfo {
    static CACHE: OnceLock<SystemInfo> = OnceLock::new();
    CACHE.get_or_init(query).clone()
}
