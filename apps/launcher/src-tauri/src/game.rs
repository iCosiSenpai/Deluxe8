//! Finding and validating the user's own files: game packages, keys and firmware.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use d8_nx::package::{self, Container, PackageKind};
use serde::Serialize;

const BASE_ID: u64 = 0x0100_1520_0002_2000;
const UPDATE_ID: u64 = 0x0100_1520_0002_2800;
const SUPPORTED_UPDATE: u32 = 23 << 16; // 4.0.1

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageView {
    pub path: String,
    pub file_name: String,
    pub container: String,
    pub title_id: String,
    pub kind: String,
    pub version: u32,
    pub display_version: Option<String>,
    pub solid: bool,
    pub size: u64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameScan {
    pub folder: String,
    pub base: Option<PackageView>,
    pub update: Option<PackageView>,
    pub dlc: Vec<PackageView>,
    pub others: Vec<PackageView>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeysView {
    pub path: String,
    pub ok: bool,
    pub count: usize,
    pub has_header_key: bool,
    pub max_master_key_revision: Option<u32>,
    pub missing_required: Vec<String>,
    pub title_keys_path: Option<String>,
    pub title_keys_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirmwareView {
    pub path: String,
    pub ok: bool,
    pub nca_count: usize,
    pub version_hint: Option<String>,
    pub is_zip: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Candidates {
    pub game_folders: Vec<String>,
    pub keys: Vec<String>,
    pub firmware: Vec<String>,
}

fn is_mk8d(id: u64) -> bool {
    id & !0xFFF == BASE_ID & !0xFFF
}

fn view(info: &package::PackageInfo, path: &Path) -> PackageView {
    let solid = info.ncz.iter().any(|n| n.solid);
    let mut note = None;
    if let Some(bits) = info.bitness_note() {
        note = Some(bits.to_string());
    }
    if info.kind == PackageKind::Update && info.version != SUPPORTED_UPDATE && note.is_none() {
        note = Some(crate::tr!(
            "Version {version}: Deluxe8 is tuned for 4.0.1 (the Native engine accepts only 3.0.4 and 4.0.1)",
            version = info.display_version.clone().unwrap_or_else(|| format!("v{}", info.version))
        ));
    }
    if solid && note.is_none() {
        note = Some(crate::tr!("Solid compression: converted to NSP once").into());
    }
    PackageView {
        path: path.to_string_lossy().to_string(),
        file_name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        container: match info.container {
            Container::Nsp => "nsp",
            Container::Nsz => "nsz",
        }
        .into(),
        title_id: info.title_id.map(|t| format!("{t:016X}")).unwrap_or_default(),
        kind: info.kind.as_str().to_lowercase(),
        version: info.version,
        display_version: info.display_version.clone(),
        solid,
        size: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        note,
    }
}

fn package_files(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if depth < 1 {
                    dirs.push((p, depth + 1));
                }
            } else if matches!(
                p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
                Some("nsp" | "nsz")
            ) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn scan_folder(folder: &Path) -> Result<GameScan, String> {
    if !folder.is_dir() {
        return Err(crate::tr!("The folder does not exist").into());
    }
    let mut scan = GameScan {
        folder: folder.to_string_lossy().to_string(),
        base: None,
        update: None,
        dlc: Vec::new(),
        others: Vec::new(),
        problems: Vec::new(),
    };
    for path in package_files(folder) {
        let info = match package::inspect(&path) {
            Ok(info) => info,
            Err(e) => {
                scan.problems.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let v = view(&info, &path);
        match (info.kind, info.title_id) {
            (PackageKind::Base, Some(BASE_ID)) => scan.base = Some(v),
            (PackageKind::Update, Some(UPDATE_ID)) => {
                // Keep the newest update if several are present.
                if scan.update.as_ref().is_none_or(|u| u.version < v.version) {
                    scan.update = Some(v);
                }
            }
            (PackageKind::Dlc, Some(id)) if is_mk8d(id) => scan.dlc.push(v),
            _ => scan.others.push(v),
        }
    }
    if scan.base.is_none() {
        scan.problems.push(crate::tr!("The base game (0100152000022000) is missing.").into());
    }
    if scan.update.is_none() {
        scan.problems.push(crate::tr!("The update (0100152000022800) is missing: 4.0.1 is needed.").into());
    }
    Ok(scan)
}

pub fn inspect_keys(path: &Path) -> Result<KeysView, String> {
    let file = File::open(path).map_err(|e| format!("{e}"))?;
    let set = d8_nx::keys::KeySet::parse(BufReader::new(file)).map_err(|e| format!("{e}"))?;
    let report = d8_nx::keys::validate(&set);
    let title_keys_path = path.with_file_name("title.keys");
    let title_count = File::open(&title_keys_path)
        .ok()
        .and_then(|f| d8_nx::keys::TitleKeys::parse(BufReader::new(f)).ok())
        .map(|t| t.count());
    Ok(KeysView {
        path: path.to_string_lossy().to_string(),
        ok: report.has_header_key && report.missing_required.is_empty(),
        count: report.count,
        has_header_key: report.has_header_key,
        max_master_key_revision: Some(report.max_master_key_revision),
        missing_required: report.missing_required,
        title_keys_path: title_count.map(|_| title_keys_path.to_string_lossy().to_string()),
        title_keys_count: title_count,
    })
}

pub fn inspect_firmware(path: &Path) -> Result<FirmwareView, String> {
    let report = d8_nx::firmware::inspect(path).map_err(|e| format!("{e}"))?;
    Ok(FirmwareView {
        path: path.to_string_lossy().to_string(),
        ok: report.looks_valid,
        nca_count: report.nca_count,
        version_hint: report.version_hint,
        is_zip: path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")),
    })
}

/// Folders of yuzu-family emulators that may already hold the user's keys or library paths: (data folder, config folder).
/// Windows keeps both in `%APPDATA%/<name>`; Linux splits them between `$XDG_DATA_HOME` and `$XDG_CONFIG_HOME`.
fn emulator_roots() -> Vec<(PathBuf, PathBuf)> {
    const NAMES: [&str; 6] = ["eden", "yuzu", "citron", "suyu", "sudachi", "torzu"];
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(base) = std::env::var_os("APPDATA").map(PathBuf::from) {
        for name in NAMES {
            roots.push((base.join(name), base.join(name).join("config")));
        }
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback))
        };
        let (data, config) = (xdg("XDG_DATA_HOME", ".local/share"), xdg("XDG_CONFIG_HOME", ".config"));
        for name in NAMES {
            roots.push((data.join(name), config.join(name)));
        }
    }
    roots
}

fn ryujinx_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join("Ryujinx"));
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".config/Ryujinx"));
    }
    roots
}

fn push_unique(list: &mut Vec<String>, p: &Path) {
    let s = p.to_string_lossy().to_string();
    if !list.contains(&s) {
        list.push(s);
    }
}

/// Folder candidates from other emulators' library settings, plus keys/firmware they hold.
pub fn find_candidates() -> Candidates {
    let mut c = Candidates::default();
    let mut library_dirs: Vec<PathBuf> = Vec::new();

    for (root, config) in emulator_roots() {
        let keys = root.join("keys").join("prod.keys");
        if keys.is_file() {
            push_unique(&mut c.keys, &keys);
        }
        for cfg in ["qt-config.ini", "sdl2-config.ini"] {
            let Ok(text) = std::fs::read_to_string(config.join(cfg)) else { continue };
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('=')
                    && key.contains("gamedirs")
                    && key.ends_with("\\path")
                    && !value.is_empty()
                {
                    library_dirs.push(PathBuf::from(value.replace('/', std::path::MAIN_SEPARATOR_STR)));
                }
            }
        }
    }
    for root in ryujinx_roots() {
        let keys = root.join("system").join("prod.keys");
        if keys.is_file() {
            push_unique(&mut c.keys, &keys);
        }
        if let Ok(text) = std::fs::read_to_string(root.join("Config.json"))
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(dirs) = json.get("game_dirs").and_then(|d| d.as_array())
        {
            library_dirs.extend(dirs.iter().filter_map(|d| d.as_str()).map(PathBuf::from));
        }
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        let keys = PathBuf::from(home).join(".switch/prod.keys");
        if keys.is_file() {
            push_unique(&mut c.keys, &keys);
        }
    }

    // A library folder (or one of its subfolders) holding files named after the title id.
    for lib in library_dirs {
        for file in package_files(&lib) {
            let name = file.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if (name.contains("01001520000220") || name.contains("mario kart 8 deluxe"))
                && let Some(parent) = file.parent()
            {
                push_unique(&mut c.game_folders, parent);
            }
        }
    }

    // Firmware stored next to a keys folder ("Chiavi-Firmware/Firmware.22.5.0", "*.zip").
    let key_paths: Vec<PathBuf> = c.keys.iter().map(PathBuf::from).collect();
    for keys in key_paths {
        for up in [keys.parent(), keys.parent().and_then(Path::parent)].into_iter().flatten() {
            let Ok(entries) = std::fs::read_dir(up) else { continue };
            for entry in entries.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains("firmware") && (p.is_dir() || name.ends_with(".zip")) {
                    push_unique(&mut c.firmware, &p);
                }
            }
        }
    }
    c
}
