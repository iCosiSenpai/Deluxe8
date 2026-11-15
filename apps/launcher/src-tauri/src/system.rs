//! Hardware and environment detection for the setup wizard and the Graphics page.

use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub vram_mb: Option<u64>,
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os: String,
    pub cpu: String,
    pub cores: usize,
    pub ram_gb: u64,
    pub gpus: Vec<GpuInfo>,
    pub displays: Vec<DisplayInfo>,
    pub lossless_dll: Option<String>,
    pub native_engine: bool,
    pub compat_engine: bool,
    pub data_dir: String,
}

fn vendor_name(id: u32) -> &'static str {
    match id {
        0x10de => "nvidia",
        0x1002 | 0x1022 => "amd",
        0x8086 => "intel",
        _ => "other",
    }
}

/// Quietly runs a command and returns trimmed stdout.
pub fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn nvidia_driver() -> Option<String> {
    command_output("nvidia-smi", &["--query-gpu=driver_version", "--format=csv,noheader"])
        .and_then(|s| s.lines().next().map(str::to_string))
}

#[cfg(windows)]
pub fn gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
    let mut out = Vec::new();
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return out;
    };
    let mut index = 0;
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
            continue;
        }
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..len]).trim().to_string();
        if out.iter().any(|g: &GpuInfo| g.name == name) {
            continue;
        }
        let vendor = vendor_name(desc.VendorId).to_string();
        let driver = if vendor == "nvidia" { nvidia_driver() } else { None };
        out.push(GpuInfo {
            name,
            vendor,
            vram_mb: Some(desc.DedicatedVideoMemory as u64 / (1024 * 1024)),
            driver,
        });
    }
    // Discrete GPUs first.
    out.sort_by_key(|g| match g.vendor.as_str() {
        "nvidia" | "amd" => 0,
        _ => 1,
    });
    out
}

#[cfg(not(windows))]
pub fn gpus() -> Vec<GpuInfo> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else { return out };
    let mut seen = std::collections::HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dev = entry.path().join("device");
        let read_hex = |file: &str| {
            std::fs::read_to_string(dev.join(file))
                .ok()
                .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        };
        let Some(vendor_id) = read_hex("vendor") else { continue };
        let slot = std::fs::read_to_string(dev.join("uevent")).ok().and_then(|u| {
            u.lines().find_map(|l| l.strip_prefix("PCI_SLOT_NAME=").map(str::to_string))
        });
        if let Some(slot) = &slot
            && !seen.insert(slot.clone())
        {
            continue;
        }
        let pretty = slot
            .as_deref()
            .and_then(|s| command_output("lspci", &["-mm", "-s", s]))
            .and_then(|line| {
                // "01:00.0 "VGA compatible controller" "NVIDIA Corporation" "GB203 [GeForce RTX 5070 Ti]" ..."
                let parts: Vec<&str> = line.split('"').filter(|p| !p.trim().is_empty()).collect();
                parts.get(3).map(|s| s.to_string())
            })
            .unwrap_or_else(|| format!("GPU {vendor_id:04x}"));
        let vram_mb = std::fs::read_to_string(dev.join("mem_info_vram_total"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|b| b / (1024 * 1024));
        let vendor = vendor_name(vendor_id).to_string();
        let driver = if vendor == "nvidia" { nvidia_driver() } else { None };
        out.push(GpuInfo { name: pretty, vendor, vram_mb, driver });
    }
    out
}

#[cfg(windows)]
pub fn displays() -> Vec<DisplayInfo> {
    use windows::Win32::Graphics::Gdi::{
        DEVMODEW, DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, EnumDisplayDevicesW,
        EnumDisplaySettingsW,
    };
    use windows::core::PCWSTR;
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0) }.as_bool() {
            break;
        }
        i += 1;
        if (dd.StateFlags & DISPLAY_DEVICE_ACTIVE).0 == 0 {
            continue;
        }
        let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        if unsafe { EnumDisplaySettingsW(PCWSTR(dd.DeviceName.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm) }.as_bool() {
            out.push(DisplayInfo {
                width: dm.dmPelsWidth,
                height: dm.dmPelsHeight,
                refresh_hz: dm.dmDisplayFrequency,
            });
        }
    }
    out
}

#[cfg(not(windows))]
pub fn displays() -> Vec<DisplayInfo> {
    // xrandr gives the refresh rate; Wayland sessions usually still ship it via XWayland.
    let mut out = Vec::new();
    if let Some(text) = command_output("xrandr", &["--current"]) {
        for line in text.lines() {
            let line = line.trim();
            if !line.contains('*') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(mode) = parts.next() else { continue };
            let Some((w, h)) = mode.split_once('x') else { continue };
            let hz = parts
                .find(|p| p.contains('*'))
                .and_then(|p| p.trim_end_matches(['*', '+']).parse::<f32>().ok())
                .unwrap_or(0.0);
            if let (Ok(width), Ok(height)) = (w.parse(), h.trim_end_matches('i').parse()) {
                out.push(DisplayInfo { width, height, refresh_hz: hz.round() as u32 });
            }
        }
    }
    out
}

/// Finds the user's own Lossless Scaling DLL (Steam library), or DELUXE8_LOSSLESS_DLL.
pub fn find_lossless_dll() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("DELUXE8_LOSSLESS_DLL") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut steam_roots: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        if let Some(path) = command_output("reg", &["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
            .and_then(|out| out.lines().find_map(|l| l.split("REG_SZ").nth(1).map(|v| v.trim().to_string())))
        {
            steam_roots.push(PathBuf::from(path));
        }
        steam_roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        steam_roots.push(home.join(".local/share/Steam"));
        steam_roots.push(home.join(".steam/steam"));
        steam_roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    let mut libraries: Vec<PathBuf> = Vec::new();
    for root in &steam_roots {
        libraries.push(root.clone());
        if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            for line in vdf.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("\"path\"") {
                    let value = rest.trim().trim_matches('"').replace("\\\\", "\\");
                    libraries.push(PathBuf::from(value));
                }
            }
        }
    }
    libraries
        .into_iter()
        .map(|lib| lib.join("steamapps").join("common").join("Lossless Scaling").join("Lossless.dll"))
        .find(|p| p.is_file())
}

pub fn collect(paths: &crate::paths::AppPaths) -> SystemInfo {
    let (cpu, cores, ram_bytes) = match sysinfo::System::new() {
        Ok(mut sys) => {
            sys.refresh_memory();
            sys.refresh_cpu_all();
            let cpu = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
            (cpu, sys.cpus().len(), sys.total_memory())
        }
        Err(_) => (String::new(), std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0), 0),
    };
    SystemInfo {
        os: sysinfo::System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.to_string()),
        cpu,
        cores,
        ram_gb: (ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)).round() as u64,
        gpus: gpus(),
        displays: displays(),
        lossless_dll: find_lossless_dll().map(|p| p.to_string_lossy().to_string()),
        native_engine: paths.native_engine_dir().is_some(),
        compat_engine: paths.compat_engine().is_some(),
        data_dir: paths.data.to_string_lossy().to_string(),
    }
}
