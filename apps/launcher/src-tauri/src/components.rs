//! Components the launcher downloads when the player asks for them. Every file is pinned by URL, size and SHA-256.
//!
//! - **Native engine**: the Windows release of Racing 8 DX Recomp (GPL-3.0) from its GitHub releases page.
//! - **DLSS** (NVIDIA RTX): NVIDIA's DLSS SDK files from github.com/NVIDIA/DLSS and the Khronos Vulkan headers, from which the
//!   Native engine's DLSS plugin is compiled on this PC with the engine's own llvm-mingw toolchain (no Visual Studio needed).
//!   NVIDIA's files are downloaded straight from NVIDIA's repository onto this PC and never leave it: Deluxe8 does not
//!   redistribute them.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::logs;
use crate::paths::{AppPaths, exe_name};

/// Racing 8 DX Recomp release the Native engine comes from (and whose plugin ABI the DLSS plugin follows).
pub const NATIVE_VERSION: &str = "0.2.4";

pub struct Remote {
    pub url: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

#[cfg(windows)]
pub const NATIVE_PACKAGE: Remote = Remote {
    url: "https://github.com/galarianpikachu2021-hue/Racing-8-DX-Recomp/releases/download/v0.2.4/KartRecomp-windows-x64.zip",
    size: 213_289_258,
    sha256: "8a54f6404037ef1fb913acd47ca7a069d6cc9d90822a65a0d63ed52970db411a",
};

/// Ryujinx Nextendo, the Online engine (Nextendo team, MIT), from their Forgejo releases.
pub const ONLINE_VERSION: &str = "1.10.6";

#[cfg(windows)]
pub const ONLINE_PACKAGE: Remote = Remote {
    url: "https://git.nextendo.network/NextendoNetwork/Ryujinx-Nextendo/releases/download/v1.10.6/Nextendo-1.10.6-win-x64.zip",
    size: 42_190_874,
    sha256: "17d65eaf3f23f940d21d457a8f1a6528bff56231c6d7e7d152bbd646c1732a19",
};
#[cfg(not(windows))]
pub const ONLINE_PACKAGE: Remote = Remote {
    url: "https://git.nextendo.network/NextendoNetwork/Ryujinx-Nextendo/releases/download/v1.10.6/Nextendo-1.10.6-linux-x64.tar.gz",
    size: 39_645_254,
    sha256: "d61e336c1ac385918b2c32e29e882f437077f81dfb4122e8c75d6b55d19f9226",
};

pub const DLSS_SDK_VERSION: &str = "310.9.16";
const DLSS_REPO: &str = "NVIDIA/DLSS";
const DLSS_COMMIT: &str = "9981c6350dc58c0ce21f024fd65e31e851c31689"; // "DLSS 310.9.16 SDK"
const DLSS_FILES: &[(&str, u64, &str)] = &[
    ("include/nvsdk_ngx.h", 40289, "dc38e7467cf415379c9d12ae1b6e4a494c453ed92720fb53e92aecb523e7b848"),
    ("include/nvsdk_ngx_defs.h", 44313, "ea23f33497cd274860d1c25a97644fce807dcb0037c594547203343103fad03e"),
    ("include/nvsdk_ngx_defs_dlssg.h", 23208, "c29765ab9e0125f9d724ae9811cc2abe03738d17486ec8eee80d78a1504549e1"),
    ("include/nvsdk_ngx_defs_vk.h", 3196, "0a24d0861ace7d6b9362a67b7f08bea1b33ea123c5ce730b19133de8a891d031"),
    ("include/nvsdk_ngx_helpers.h", 5317, "5bcbadfe7478b802cf6d3aca4dc5ddd7d0889b99726e69c63f9e9bd555f44471"),
    ("include/nvsdk_ngx_helpers_cuda.h", 3593, "f2851a76107bdf4fbcb7b261f3c35573c312d8ecb4bc18fb02d9e2cec7705042"),
    ("include/nvsdk_ngx_helpers_d3d.h", 31976, "ec75224f36ed6580baaf250fa83620405ad81a1d98c2c436654a0c9cf6a6b8ba"),
    ("include/nvsdk_ngx_helpers_dlssg_vk.h", 17077, "b371d90b9f4372b9a415e68a0afbdc2f31b7b552e219a9637d848749bc965316"),
    ("include/nvsdk_ngx_helpers_vk.h", 19720, "c192bff72138f12f770db48e12c1d8f712dfc81ea0e8e055ebb27d8dd1b31623"),
    ("include/nvsdk_ngx_params.h", 7744, "943bc8cc5cdae03b6303016fbad3183636f2335ae27a2d18776798c3b4efabbc"),
    ("include/nvsdk_ngx_params_dlssg.h", 6137, "c24fa5aa68ffa2808c828b75cfa32843f1606f80a145f84055ac450d7e6c74ad"),
    ("include/nvsdk_ngx_vk.h", 38382, "2d364ce7132881eb669e9498fd570d74cba563b1fbebc82c235f8e1ad2dd8b6d"),
    ("lib/Windows_x86_64/x64/nvsdk_ngx_d.lib", 4_090_658, "bc43df9ec032004fde681d3e3afd01cde239e03a0125b822e88674c84287f99c"),
    ("lib/Windows_x86_64/rel/nvngx_dlss.dll", 58_956_912, "d00b5b0ab29c3ca101b70c3a5ecb3a01b7ef33766a3383a28a18623083e6123e"),
    ("lib/Windows_x86_64/rel/nvngx_dlssg.dll", 7_506_544, "8d59d49b36410dd6b62fef2503a5871d347415bd85d943fa0848563a95dd50c7"),
    ("LICENSE.txt", 26620, "d4216e39ebef5f9b50a6712ebb37beeb5379862a67733a9999c651f21592aaf0"),
];
pub const DLSS_LICENSE_URL: &str = "https://github.com/NVIDIA/DLSS/blob/main/LICENSE.txt";

const VK_REPO: &str = "KhronosGroup/Vulkan-Headers";
const VK_COMMIT: &str = "6802bb4733b63ed5efd3adb308a6c885ef180ea1"; // v1.4.363
const VK_FILES: &[(&str, u64, &str)] = &[
    ("include/vulkan/vk_platform.h", 2832, "a2cd9085c66776845d2524c3b0e73ccf4827ba046aa6f676c4b8ddbed47d6552"),
    ("include/vulkan/vulkan.h", 1620, "096f50152d0b298d8df84a561a37d8190c8a60af34dff0e2cc8328b24491a640"),
    ("include/vulkan/vulkan_core.h", 1_362_950, "2f64fd6c7c3f342b0e44f039f1c359b7f0e6cfcb71a7dca1cc5156282faa3b51"),
    ("include/vk_video/vulkan_video_codec_av1std.h", 16694, "c75c1d324b97d247aef0008024bc7f3adb98c741ab7f66c882ec38fdacc7ee33"),
    ("include/vk_video/vulkan_video_codec_av1std_decode.h", 4257, "7e3a1ce177c12546d410f3179ce1b81f2da7a8eba4f525b29cd563ab4099c0e5"),
    ("include/vk_video/vulkan_video_codec_av1std_encode.h", 5986, "8d166b4543260a38347860443b1a59c7a5e86cdb0d0facaf4c704f667de030e3"),
    ("include/vk_video/vulkan_video_codec_h264std.h", 13961, "fded484cef9f90fbbd089e0647268f36e5f4292179fbc5428a2c9e8d7709bbe9"),
    ("include/vk_video/vulkan_video_codec_h264std_decode.h", 2730, "37b970c3d80536ad1ac074cfe19b58ee03c3075378e02179e1dc5e4351266821"),
    ("include/vk_video/vulkan_video_codec_h264std_encode.h", 6885, "227e092b53c4e7ca1a948ed021704511c1ed16040cd6188ff6703e5ae66db64d"),
    ("include/vk_video/vulkan_video_codec_h265std.h", 23446, "0b81f8986ada015ef2e127449eff9d9634899bb59a0a9277a4054e9ec4416c12"),
    ("include/vk_video/vulkan_video_codec_h265std_decode.h", 2436, "879a0dd370a1b1ad184638906c52bdfe7d80d639fbc5a5baa8b02d7c5b60b147"),
    ("include/vk_video/vulkan_video_codec_h265std_encode.h", 7469, "abb3e72af22e4e0a3dbe5dff7be1b275949388809fb3987830341e8f495ea1c7"),
    ("include/vk_video/vulkan_video_codec_vp9std.h", 5391, "0a47125865376a3fe7014b69ff6db9d04e30ebf8c4d15664f1d894649ad5c09d"),
    ("include/vk_video/vulkan_video_codec_vp9std_decode.h", 2378, "1ceb1a8d0e3370e508cf688a6e57dc314cd82186b60f3cef420ea4b1b483865d"),
    ("include/vk_video/vulkan_video_codecs_common.h", 667, "d2e7caa396c521d03d491a269572b1d31b925b5127d22fda14199951ebae89f8"),
    ("LICENSE.md", 495, "95ad366d23fadf701d355bc45fb8b82ae2d700239471d35d41286ac3b08ff903"),
];

/// The plugin files the Native engine loads (`kr_<kind>_<id>.dll`) plus NVIDIA's runtime snippets beside them.
pub const DLSS_INSTALLED: &[&str] = &["kr_upscaler_dlss.dll", "kr_framegen_dlss.dll", "nvngx_dlss.dll", "nvngx_dlssg.dll"];
const DLSS_STAMP: &str = "deluxe8-dlss.json";

fn raw_url(repo: &str, commit: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{commit}/{path}")
}

fn sha256_file(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_verified(path: &Path, size: u64, sha256: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.len() == size) && sha256_file(path).as_deref() == Some(sha256)
}

/// How long a download may go without receiving a byte.
const STALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Resolves once `cancel` is set.
async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Downloads `url` to `dest` and checks its size and SHA-256 (a verified file already there is kept). `progress(done, total)`.
pub fn download(
    url: &str,
    dest: &Path,
    size: u64,
    sha256: &str,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), String> {
    if is_verified(dest, size, sha256) {
        progress(size, size);
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = PathBuf::from(format!("{}.part", dest.display()));
    let result = tauri::async_runtime::block_on(async {
        let client = reqwest::Client::builder()
            .user_agent(concat!("Deluxe8/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        let mut response = client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| crate::tr!("Download of {url} failed: {e}", url = url, e = e))?;
        let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        loop {
            // a stalled connection ends the download instead of hanging the preparation, and "Annulla" works while waiting
            let next = tokio::select! {
                next = tokio::time::timeout(STALL_TIMEOUT, response.chunk()) => next,
                _ = cancelled(cancel) => return Err(crate::tr!("Cancelled").to_string()),
            };
            let next = next
                .map_err(|_| crate::tr!("The download stalled for {s} seconds: check the connection and try again", s = STALL_TIMEOUT.as_secs()))?
                .map_err(|e| crate::tr!("Download interrupted: {e}", e = e))?;
            let Some(chunk) = next else { break };
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            hasher.update(&chunk);
            done += chunk.len() as u64;
            progress(done.min(size), size);
        }
        file.flush().map_err(|e| e.to_string())?;
        if done != size {
            return Err(crate::tr!("{url}: received {done} bytes instead of {size}", url = url, done = done, size = size));
        }
        let got = hex(&hasher.finalize());
        if got != sha256 {
            return Err(crate::tr!("{url}: the downloaded file does not match (SHA-256 {got})", url = url, got = got));
        }
        Ok(())
    });
    match result {
        Ok(()) => std::fs::rename(&part, dest).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

fn run(program: &Path, args: &[String], cwd: &Path) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("{}: {e}", program.display()))?;
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        return Err(tail[tail.len().saturating_sub(6)..].join("\n"));
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ Native engine

/// Makes sure a Native engine is on this PC: the one already found (installed next to the launcher, downloaded earlier, or a
/// development build), otherwise the pinned GitHub release, downloaded and unpacked into `<data>/engines/native/v<version>`.
pub fn ensure_native_engine(
    paths: &AppPaths,
    report: &mut dyn FnMut(String, Option<f64>),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if let Some(dir) = paths.native_engine_dir() {
        return Ok(dir);
    }
    #[cfg(not(windows))]
    {
        let _ = (report, cancel);
        Err(crate::tr!(
            "On Linux the Native engine is not downloaded by the app yet: extract the AppImage of Racing 8 DX Recomp v{version} (Racing_8_DX_Recomp-x86_64.AppImage --appimage-extract) and copy the extracted folder to {dir}",
            version = NATIVE_VERSION,
            dir = paths.native_install_dir().display()
        ))
    }
    #[cfg(windows)]
    {
        let zip_path = paths.downloads.join(format!("KartRecomp-windows-x64-v{NATIVE_VERSION}.zip"));
        let mut last = std::time::Instant::now();
        download(
            NATIVE_PACKAGE.url,
            &zip_path,
            NATIVE_PACKAGE.size,
            NATIVE_PACKAGE.sha256,
            &mut |done, total| {
                if last.elapsed().as_millis() >= 250 || done == total {
                    last = std::time::Instant::now();
                    report(
                        crate::tr!("Downloading from GitHub: {done} of {total} MB", done = done / 1_000_000, total = total / 1_000_000),
                        Some(0.9 * done as f64 / total as f64),
                    );
                }
            },
            cancel,
        )?;
        report(crate::tr!("Unpacking").into(), Some(0.9));
        let dest = paths.native_install_dir();
        let partial = dest.with_file_name(format!(".v{NATIVE_VERSION}.partial"));
        if partial.exists() {
            std::fs::remove_dir_all(&partial).map_err(|e| e.to_string())?;
        }
        extract_zip(&zip_path, &partial, cancel)?;
        if !partial.join("engine").join(exe_name("mk8d-engine")).is_file() {
            let _ = std::fs::remove_dir_all(&partial);
            return Err(crate::tr!("The Native engine archive has no engine/mk8d-engine.exe").into());
        }
        if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&partial, &dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&zip_path); // 200 MB that are now unpacked
        logs::write(&format!("native engine v{NATIVE_VERSION} installed in {}", dest.display()));
        report(crate::tr!("Installed").into(), Some(1.0));
        Ok(dest)
    }
}

/// Unpacks a zip, dropping a single top-level folder (the release zip holds `win-x64/...`). Entry names are checked: nothing
/// is written outside `dest`.
fn extract_zip(zip_path: &Path, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let names: Vec<String> = (0..zip.len())
        .map(|i| {
            zip.by_index_raw(i)
                .ok()
                .and_then(|f| f.name().ok().map(|n| n.replace('\\', "/")))
                .unwrap_or_default()
        })
        .collect();
    let first = |n: &str| n.split('/').next().unwrap_or("").to_string();
    let top = names.first().map(|n| first(n)).unwrap_or_default();
    let strip = !top.is_empty() && names.iter().all(|n| first(n) == top && n.len() > top.len());
    for (i, name) in names.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::tr!("Cancelled").into());
        }
        let rel = if strip { &name[top.len() + 1..] } else { name.as_str() };
        if rel.is_empty() {
            continue;
        }
        let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        if parts.iter().any(|p| *p == ".." || p.contains(':')) || rel.starts_with('/') {
            return Err(crate::tr!("Invalid path in the archive: {name}", name = name));
        }
        let out_path = parts.iter().fold(dest.to_path_buf(), |acc, p| acc.join(p));
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() || name.ends_with('/') {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(dir) = out_path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ Online engine

/// Makes sure Ryujinx Nextendo is on this PC: the pinned release, downloaded and unpacked into
/// `<data>/engines/online/v<version>` (the archive's single `publish` folder is dropped).
pub fn ensure_online_engine(
    paths: &AppPaths,
    report: &mut dyn FnMut(String, Option<f64>),
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    if let Some(dir) = paths.online_engine_dir() {
        return Ok(dir);
    }
    let archive = paths.downloads.join(NAME_ONLINE_ARCHIVE);
    let mut last = std::time::Instant::now();
    download(
        ONLINE_PACKAGE.url,
        &archive,
        ONLINE_PACKAGE.size,
        ONLINE_PACKAGE.sha256,
        &mut |done, total| {
            if last.elapsed().as_millis() >= 250 || done == total {
                last = std::time::Instant::now();
                report(
                    crate::tr!("Downloading from Nextendo: {done} of {total} MB", done = done / 1_000_000, total = total / 1_000_000),
                    Some(0.9 * done as f64 / total as f64),
                );
            }
        },
        cancel,
    )?;
    report(crate::tr!("Unpacking").into(), Some(0.9));
    let dest = paths.online_install_dir();
    let partial = dest.with_file_name(format!(".v{ONLINE_VERSION}.partial"));
    if partial.exists() {
        std::fs::remove_dir_all(&partial).map_err(|e| e.to_string())?;
    }
    if cfg!(windows) {
        extract_zip(&archive, &partial, cancel)?;
    } else {
        // the Linux build is a tar.gz: every Linux system has tar
        std::fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
        run(Path::new("tar"), &["-xzf".into(), archive.display().to_string(), "-C".into(), partial.display().to_string()], &partial)?;
        let inner = partial.join("publish");
        if inner.is_dir() {
            let flat = partial.with_file_name(format!(".v{ONLINE_VERSION}.flat"));
            let _ = std::fs::remove_dir_all(&flat);
            std::fs::rename(&inner, &flat).map_err(|e| e.to_string())?;
            std::fs::remove_dir_all(&partial).map_err(|e| e.to_string())?;
            std::fs::rename(&flat, &partial).map_err(|e| e.to_string())?;
        }
    }
    if !crate::nextendo::exe(&partial).is_file() {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(crate::tr!("The Ryujinx Nextendo archive has no Ryujinx executable").into());
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&partial, &dest).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&archive);
    logs::write(&format!("Ryujinx Nextendo v{ONLINE_VERSION} installed in {}", dest.display()));
    report(crate::tr!("Installed").into(), Some(1.0));
    Ok(dest)
}

const NAME_ONLINE_ARCHIVE: &str = if cfg!(windows) {
    "Nextendo-1.10.6-win-x64.zip"
} else {
    "Nextendo-1.10.6-linux-x64.tar.gz"
};

// ------------------------------------------------------------------------------------------ DLSS

/// Where the Native engine looks for plugins built on this PC (`$XDG_DATA_HOME/kartrecomp/plugins`).
pub fn native_plugins_dir(paths: &AppPaths) -> PathBuf {
    paths.native_user.join("data").join("kartrecomp").join("plugins")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentsStatus {
    pub native_version: String,
    pub native_engine: Option<String>,
    pub native_download_mb: u64,
    pub native_downloadable: bool,
    pub dlss_installed: bool,
    pub dlss_version: String,
    pub dlss_download_mb: u64,
    pub dlss_license_url: String,
    pub dlss_buildable: bool,
    pub online_version: String,
    /// Folder of Ryujinx Nextendo when it is on this PC.
    pub online_engine: Option<String>,
    pub online_download_mb: u64,
    /// A Nextendo Network account is linked in Ryujinx Nextendo.
    pub nextendo_signed_in: bool,
}

pub fn status(paths: &AppPaths) -> ComponentsStatus {
    let plugins = native_plugins_dir(paths);
    let native = paths.native_engine_dir();
    ComponentsStatus {
        native_version: NATIVE_VERSION.into(),
        native_engine: native.as_ref().map(|p| p.display().to_string()),
        native_download_mb: if cfg!(windows) { 213 } else { 0 },
        native_downloadable: cfg!(windows),
        dlss_installed: DLSS_INSTALLED.iter().all(|f| plugins.join(f).is_file()),
        dlss_version: DLSS_SDK_VERSION.into(),
        dlss_download_mb: (DLSS_FILES.iter().chain(VK_FILES).map(|f| f.1).sum::<u64>()).div_ceil(1_000_000),
        dlss_license_url: DLSS_LICENSE_URL.into(),
        dlss_buildable: cfg!(windows) && paths.dlss_plugin_src.is_some(),
        online_version: ONLINE_VERSION.into(),
        online_engine: paths.online_engine_dir().map(|p| p.display().to_string()),
        online_download_mb: ONLINE_PACKAGE.size.div_ceil(1_000_000),
        nextendo_signed_in: crate::nextendo::signed_in(paths),
    }
}

/// Downloads NVIDIA's DLSS SDK files and the Vulkan headers, compiles the DLSS plugin with the Native engine's toolchain and
/// installs it with NVIDIA's runtime into the engine's plugin folder in Deluxe8's data. Windows only.
pub fn install_dlss(
    paths: &AppPaths,
    native_root: &Path,
    report: &mut dyn FnMut(String, Option<f64>),
    cancel: &AtomicBool,
) -> Result<String, String> {
    if !cfg!(windows) {
        return Err(crate::tr!("On Linux the DLSS plugin is set up with plugins/dlss/install-linux.sh from Racing 8 DX Recomp").into());
    }
    let src = paths.dlss_plugin_src.clone().ok_or(crate::tr!("The DLSS plugin sources are missing from the Deluxe8 installation"))?;
    let clang = native_root.join("toolchain").join("bin").join(exe_name("clang"));
    if !clang.is_file() {
        return Err(crate::tr!("The Native engine's toolchain was not found ({path})", path = clang.display()));
    }

    // 1. NVIDIA's SDK files and the Khronos headers, straight from their GitHub repositories
    let sdk = paths.downloads.join(format!("dlss-sdk-{DLSS_SDK_VERSION}"));
    let vk = paths.downloads.join("vulkan-headers-1.4.363");
    let plan: Vec<(String, PathBuf, u64, &str)> = DLSS_FILES
        .iter()
        .map(|(rel, size, sha)| (raw_url(DLSS_REPO, DLSS_COMMIT, rel), sdk.join(rel), *size, *sha))
        .chain(VK_FILES.iter().map(|(rel, size, sha)| (raw_url(VK_REPO, VK_COMMIT, rel), vk.join(rel), *size, *sha)))
        .collect();
    let total: u64 = plan.iter().map(|p| p.2).sum();
    let mut before = 0u64;
    let mut last = std::time::Instant::now();
    for (url, dest, size, sha) in &plan {
        download(
            url,
            dest,
            *size,
            sha,
            &mut |done, _| {
                if last.elapsed().as_millis() >= 250 {
                    last = std::time::Instant::now();
                    let now = before + done;
                    report(
                        crate::tr!("Downloading from GitHub (NVIDIA, Khronos): {done} of {total} MB", done = now / 1_000_000, total = total.div_ceil(1_000_000)),
                        Some(0.8 * now as f64 / total as f64),
                    );
                }
            },
            cancel,
        )?;
        before += size;
    }

    // 2. compile with the Native engine's own toolchain (llvm-mingw, no C++ standard library needed)
    report(crate::tr!("Building the plugin for this PC").into(), Some(0.82));
    let work = paths.data.join("cache").join("dlss-build");
    if work.exists() {
        std::fs::remove_dir_all(&work).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let p = |path: &Path| path.display().to_string();
    let cxx = |source: &str, obj: &str| -> Vec<String> {
        let mut a: Vec<String> = [
            "--target=x86_64-w64-mingw32", "-std=c++17", "-O2", "-fno-exceptions", "-fno-rtti", "-fno-threadsafe-statics",
            "-nostdinc++", "-w", "-c",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        a.push(p(&src.join(source)));
        a.extend([format!("-I{}", p(&sdk.join("include"))), format!("-I{}", p(&vk.join("include"))), format!("-I{}", p(&src))]);
        a.extend(["-o".to_string(), p(&work.join(obj))]);
        a
    };
    run(&clang, &cxx("kr_upscaler_dlss.cpp", "plugin.o"), &work).map_err(|e| crate::tr!("Building the DLSS plugin: {e}", e = e))?;
    run(&clang, &cxx("msvc_shim.cpp", "shim.o"), &work).map_err(|e| crate::tr!("Building the DLSS plugin: {e}", e = e))?;
    report(crate::tr!("Linking with NVIDIA's NGX library").into(), Some(0.9));
    let mut link: Vec<String> = ["--target=x86_64-w64-mingw32", "-shared", "-fuse-ld=lld", "-rtlib=compiler-rt", "-unwindlib=none", "-nostdlib++"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    link.extend(["-o".to_string(), p(&work.join("kr_upscaler_dlss.dll")), p(&work.join("plugin.o")), p(&work.join("shim.o"))]);
    link.push(p(&sdk.join("lib/Windows_x86_64/x64/nvsdk_ngx_d.lib")));
    link.extend(["-lvcruntime140", "-ladvapi32", "-luser32", "-lshell32", "-lole32"].iter().map(|s| s.to_string()));
    // the NGX library asks for MSVC's OLDNAMES and msvcprt: the few symbols it needs come from msvc_shim.cpp instead
    link.extend(["-Xlinker", "-Xlink=-nodefaultlib:OLDNAMES", "-Xlinker", "-Xlink=-nodefaultlib:msvcprt"].iter().map(|s| s.to_string()));
    run(&clang, &link, &work).map_err(|e| crate::tr!("Linking the DLSS plugin: {e}", e = e))?;
    let forwarder: Vec<String> = vec![
        "--target=x86_64-w64-mingw32".into(),
        "-O2".into(),
        "-shared".into(),
        "-fuse-ld=lld".into(),
        "-rtlib=compiler-rt".into(),
        "-unwindlib=none".into(),
        "-w".into(),
        "-o".into(),
        p(&work.join("kr_framegen_dlss.dll")),
        p(&src.join("forwarder.c")),
    ];
    run(&clang, &forwarder, &work).map_err(|e| crate::tr!("Building the DLSS forwarder: {e}", e = e))?;

    // 3. install beside NVIDIA's runtime snippets, in the engine's plugin folder inside Deluxe8's data
    report(crate::tr!("Installing").into(), Some(0.96));
    let plugins = native_plugins_dir(paths);
    std::fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    for (from, name) in [
        (work.join("kr_upscaler_dlss.dll"), "kr_upscaler_dlss.dll"),
        (work.join("kr_framegen_dlss.dll"), "kr_framegen_dlss.dll"),
        (sdk.join("lib/Windows_x86_64/rel/nvngx_dlss.dll"), "nvngx_dlss.dll"),
        (sdk.join("lib/Windows_x86_64/rel/nvngx_dlssg.dll"), "nvngx_dlssg.dll"),
    ] {
        std::fs::copy(&from, plugins.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    let stamp = serde_json::json!({
        "dlssSdk": DLSS_SDK_VERSION,
        "nativeEngine": NATIVE_VERSION,
        "note": "Built on this PC from NVIDIA's DLSS SDK (github.com/NVIDIA/DLSS). For this PC only: do not redistribute.",
    });
    let _ = std::fs::write(plugins.join(DLSS_STAMP), serde_json::to_vec_pretty(&stamp).unwrap_or_default());
    let _ = std::fs::remove_dir_all(&work);
    logs::write(&format!("DLSS plugin built and installed in {}", plugins.display()));
    report(crate::tr!("Ready").into(), Some(1.0));
    Ok(crate::tr!("DLSS {version} ready for this PC", version = DLSS_SDK_VERSION))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deluxe8-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extract_strips_the_release_folder() {
        let dir = scratch("zip-ok");
        let zip = dir.join("a.zip");
        // PowerShell's Compress-Archive may write backslashes: both separators count
        make_zip(&zip, &[("win-x64/engine/mk8d-engine.exe", b"exe"), (r"win-x64\data\x.txt", b"x")]);
        extract_zip(&zip, &dir.join("out"), &AtomicBool::new(false)).unwrap();
        assert_eq!(std::fs::read(dir.join("out/engine/mk8d-engine.exe")).unwrap(), b"exe");
        assert_eq!(std::fs::read(dir.join("out/data/x.txt")).unwrap(), b"x");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn extract_refuses_paths_outside_the_destination() {
        let dir = scratch("zip-evil");
        let zip = dir.join("b.zip");
        make_zip(&zip, &[("ok.txt", b"1"), ("../evil.txt", b"2")]);
        assert!(extract_zip(&zip, &dir.join("out"), &AtomicBool::new(false)).is_err());
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_keeps_a_verified_file() {
        let dir = scratch("dl");
        let file = dir.join("f.bin");
        std::fs::write(&file, b"hello").unwrap();
        let sha = hex(&Sha256::digest(b"hello"));
        // already verified: no network needed
        download("https://invalid.example/f.bin", &file, 5, &sha, &mut |_, _| {}, &AtomicBool::new(false)).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pins_look_sane() {
        for (rel, size, sha) in DLSS_FILES.iter().chain(VK_FILES) {
            assert!(!rel.starts_with('/') && !rel.contains(".."), "{rel}");
            assert!(*size > 0, "{rel}");
            assert_eq!(sha.len(), 64, "{rel}");
        }
    }
}
