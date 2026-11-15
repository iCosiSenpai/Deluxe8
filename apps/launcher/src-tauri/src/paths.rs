//! Where Deluxe8 keeps its data and where it finds its engines.
//!
//! Data (settings, converted content, engine user dirs, downloads, logs):
//!   Windows `%LOCALAPPDATA%\Deluxe8\data` (tens of GB: not in the roaming profile; a folder of its own because the per-user
//!   installer puts the program in `%LOCALAPPDATA%\Deluxe8`), Linux `$XDG_DATA_HOME/deluxe8` (`~/.local/share/deluxe8`), or
//!   `DELUXE8_DATA_DIR`.
//! Engines: in the installed launcher's resources (`engine/compat`, `engine/native`; next to the executable on Windows,
//! `/usr/lib/Deluxe8` for the Linux packages), the Native engine downloaded by the launcher (`<data>/engines/native/v<version>`),
//! or the repository's `out/` folders when running from a checkout.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data: PathBuf,
    pub logs: PathBuf,
    pub content: PathBuf,
    pub content_base: PathBuf,
    pub content_updates: PathBuf,
    pub compat_user: PathBuf,
    /// The Native engine's own data: XDG roots `data/`, `cache/`, `config/` (each holds a `kartrecomp` folder).
    pub native_user: PathBuf,
    /// Engines the launcher downloaded (`native/v<version>`).
    pub engines: PathBuf,
    /// Downloaded archives and SDK files, kept so a rebuild needs no second download.
    pub downloads: PathBuf,
    pub settings_file: PathBuf,
    pub compat_engine_dir: Option<PathBuf>,
    /// Where a Native engine may be, best first; see [`AppPaths::native_engine_dir`].
    native_candidates: Vec<PathBuf>,
    /// Sources of the DLSS plugin built on this PC (`resources/dlss-plugin`).
    pub dlss_plugin_src: Option<PathBuf>,
    pub font: Option<PathBuf>,
}

pub const TITLE_ID: &str = "0100152000022000";

fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("DELUXE8_DATA_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Deluxe8").join("data")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
            });
        base.join("deluxe8")
    }
}

/// Walks up from the launcher executable looking for the repository root (engine/upstream.json).
fn repo_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..8 {
        if dir.join("engine").join("upstream.json").is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

pub fn platform_dir() -> &'static str {
    if cfg!(windows) { "win-x64" } else { "linux-x64" }
}

pub fn exe_name(stem: &str) -> String {
    if cfg!(windows) { format!("{stem}.exe") } else { stem.to_string() }
}

fn first_dir_with(candidates: &[PathBuf], file: &str) -> Option<PathBuf> {
    candidates.iter().find(|d| d.join(file).is_file()).cloned()
}

impl AppPaths {
    pub fn resolve(resource_dir: Option<PathBuf>) -> AppPaths {
        let data = data_root();
        let content = data.join("content");
        let engines = data.join("engines");
        let repo = repo_root();
        let exe = exe_dir();

        // an installed launcher keeps the engines in its resources (the executable's folder on Windows, /usr/lib/<app> in the
        // Linux packages); both are looked at
        let mut installed: Vec<PathBuf> = Vec::new();
        for dir in [resource_dir.clone(), exe.clone()].into_iter().flatten() {
            if !installed.contains(&dir) {
                installed.push(dir);
            }
        }
        let mut compat_candidates = Vec::new();
        let mut native_candidates = Vec::new();
        let native_downloaded = engines.join("native").join(format!("v{}", crate::components::NATIVE_VERSION));
        if let Some(dir) = std::env::var_os("DELUXE8_NATIVE_DIR").filter(|d| !d.is_empty()) {
            // an explicit Native engine folder; only the downloaded one is looked at besides it
            native_candidates.push(PathBuf::from(dir));
            native_candidates.push(native_downloaded);
        } else {
            native_candidates.extend(installed.iter().map(|d| d.join("engine").join("native")));
            native_candidates.push(native_downloaded);
            if let Some(repo) = &repo {
                native_candidates.push(repo.join("out").join("native").join(platform_dir()));
            }
        }
        compat_candidates.extend(installed.iter().map(|d| d.join("engine").join("compat")));
        if let Some(repo) = &repo {
            compat_candidates.push(repo.join("out").join("engine").join(platform_dir()));
        }
        // the Linux AppImage keeps the engine next to the launcher (usr/bin), where linuxdeploy bundles its libraries
        compat_candidates.extend(exe.clone());

        let mut font_candidates = Vec::new();
        let mut dlss_candidates = Vec::new();
        if let Some(res) = &resource_dir {
            font_candidates.push(res.join("fonts").join("Archivo-Variable.ttf"));
            dlss_candidates.push(res.join("dlss-plugin"));
        }
        if let Some(repo) = &repo {
            font_candidates.push(repo.join("apps/launcher/src/assets/fonts/Archivo-Variable.ttf"));
            dlss_candidates.push(repo.join("apps/launcher/src-tauri/resources/dlss-plugin"));
        }

        AppPaths {
            logs: data.join("logs"),
            content_base: content.join("base"),
            content_updates: content.join("updates"),
            content,
            compat_user: data.join("compat"),
            native_user: data.join("native"),
            downloads: data.join("downloads"),
            engines,
            settings_file: data.join("settings.json"),
            compat_engine_dir: first_dir_with(&compat_candidates, &exe_name("deluxe8-engine")),
            native_candidates,
            dlss_plugin_src: dlss_candidates.into_iter().find(|d| d.join("kr_upscaler_dlss.cpp").is_file()),
            font: font_candidates.into_iter().find(|p| p.is_file()),
            data,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for dir in [&self.data, &self.logs, &self.content_base, &self.content_updates, &self.compat_user] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// The Native engine folder (with `engine/mk8d-engine`), looked up on every call: one can be downloaded while the
    /// launcher runs.
    pub fn native_engine_dir(&self) -> Option<PathBuf> {
        self.native_candidates
            .iter()
            .find(|d| d.join("engine").join(exe_name("mk8d-engine")).is_file())
            .cloned()
    }

    /// Ryujinx Nextendo (the Online engine) when it has been downloaded.
    pub fn online_engine_dir(&self) -> Option<PathBuf> {
        let dir = self.online_install_dir();
        crate::nextendo::exe(&dir).is_file().then_some(dir)
    }

    /// Where Ryujinx Nextendo goes.
    pub fn online_install_dir(&self) -> PathBuf {
        self.engines.join("online").join(format!("v{}", crate::components::ONLINE_VERSION))
    }

    /// Where a downloaded Native engine goes.
    pub fn native_install_dir(&self) -> PathBuf {
        self.engines.join("native").join(format!("v{}", crate::components::NATIVE_VERSION))
    }

    pub fn compat_engine(&self) -> Option<PathBuf> {
        self.compat_engine_dir.as_ref().map(|d| d.join(exe_name("deluxe8-engine")))
    }

    pub fn compat_room(&self) -> Option<PathBuf> {
        self.compat_engine_dir
            .as_ref()
            .map(|d| d.join(exe_name("deluxe8-room")))
            .filter(|p| p.is_file())
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.compat_user.join("load").join(TITLE_ID)
    }

    pub fn mods_disabled_dir(&self) -> PathBuf {
        self.compat_user.join("load-disabled").join(TITLE_ID)
    }
}
