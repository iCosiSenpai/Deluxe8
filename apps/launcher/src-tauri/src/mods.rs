//! Mods for the Compat engine: `<compat>/load/<title id>/<mod>/{romfs,exefs,cheats}`.
//! Disabled mods are moved to `<compat>/load-disabled/<title id>/` so the engine never sees them.

use std::path::Path;

use serde::Serialize;

use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub kinds: Vec<String>,
    pub size: u64,
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            match entry.metadata() {
                Ok(m) if m.is_dir() => stack.push(entry.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

/// Patch folders the launcher manages from its own settings (not shown as mods).
pub const BUILTIN_SPLIT60: &str = "Deluxe8 60fps split-screen";

fn scan(dir: &Path, enabled: bool, out: &mut Vec<ModInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || entry.file_name() == BUILTIN_SPLIT60 {
            continue;
        }
        let kinds = ["romfs", "exefs", "cheats"]
            .iter()
            .filter(|k| path.join(k).is_dir())
            .map(|k| k.to_string())
            .collect();
        out.push(ModInfo {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path.to_string_lossy().to_string(),
            enabled,
            kinds,
            size: dir_size(&path),
        });
    }
}

pub fn list(paths: &AppPaths) -> Vec<ModInfo> {
    let _ = std::fs::create_dir_all(paths.mods_dir());
    let mut out = Vec::new();
    scan(&paths.mods_dir(), true, &mut out);
    scan(&paths.mods_disabled_dir(), false, &mut out);
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

pub fn set_enabled(paths: &AppPaths, name: &str, enabled: bool) -> Result<(), String> {
    if name.contains(['/', '\\']) || name == ".." || name.is_empty() {
        return Err("Nome non valido".into());
    }
    let (from, to) = if enabled {
        (paths.mods_disabled_dir(), paths.mods_dir())
    } else {
        (paths.mods_dir(), paths.mods_disabled_dir())
    };
    std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
    std::fs::rename(from.join(name), to.join(name)).map_err(|e| e.to_string())
}

/// Main of update 4.0.1 (build ID 158C0C7399AABD7BF83795C2EF3484C7): `b.lo` at main+0xafece8, which keeps 3-4 player split
/// screen at 30 fps by drawing each view every other frame, becomes `b` (every view drawn every frame). Offset found by
/// Racing 8 DX Recomp (docs/FRAMERATE.md, "split60"); the IPS offset counts the NSO's 0x100-byte header.
const SPLIT60_BUILD_ID: &str = "158C0C7399AABD7BF83795C2EF3484C7";
const SPLIT60_IPS: &[u8] = &[
    b'P', b'A', b'T', b'C', b'H', //
    0xAF, 0xED, 0xE8, 0x00, 0x04, 0x0D, 0x00, 0x00, 0x14, // 0x100 + 0xafece8: 0x1400000d (b +0x34)
    b'E', b'O', b'F',
];

/// Installs or removes the split-screen patch for the Compat engine (an IPS in the mods folder, 4.0.1 only).
pub fn sync_split60(paths: &AppPaths, enabled: bool, update_version: u32) -> std::io::Result<()> {
    let dir = paths.mods_dir().join(BUILTIN_SPLIT60);
    if enabled && update_version == 23 << 16 {
        let exefs = dir.join("exefs");
        std::fs::create_dir_all(&exefs)?;
        std::fs::write(exefs.join(format!("{SPLIT60_BUILD_ID}.ips")), SPLIT60_IPS)
    } else if dir.exists() {
        std::fs::remove_dir_all(dir)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split60_ips_writes_one_branch() {
        // classic IPS: "PATCH", 3-byte big-endian offset, 2-byte size, data, "EOF"
        assert_eq!(&SPLIT60_IPS[..5], b"PATCH");
        let offset = u32::from_be_bytes([0, SPLIT60_IPS[5], SPLIT60_IPS[6], SPLIT60_IPS[7]]);
        assert_eq!(offset, 0x100 + 0xafece8); // NSO header + main+0xafece8
        assert_eq!(u16::from_be_bytes([SPLIT60_IPS[8], SPLIT60_IPS[9]]), 4);
        assert_eq!(u32::from_le_bytes(SPLIT60_IPS[10..14].try_into().unwrap()), 0x1400000d); // b +0x34
        assert_eq!(&SPLIT60_IPS[14..], b"EOF");
    }
}
