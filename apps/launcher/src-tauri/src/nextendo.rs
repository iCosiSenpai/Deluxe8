//! The Online engine: Ryujinx Nextendo, the Nextendo team's build of Ryujinx (MIT) that plays online on the Nextendo
//! Network (a community-run replacement of the console's online service: matchmaking, friends, rooms, tournaments).
//!
//! Deluxe8 downloads the official build ([`crate::components::ensure_online_engine`]), keeps its data in `<data>/online`
//! (passed with `-r`, never the engine folder), puts the player's keys, firmware, update and DLC there and starts the
//! game. The Nextendo account is linked inside Ryujinx's own window (Settings > Nextendo): Deluxe8 only
//! checks that the account file exists and never reads it (it holds a login token, protected by Windows on Windows).

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::paths::{AppPaths, TITLE_ID};
use crate::prepare::Manifest;
use crate::settings::Settings;

/// Ryujinx's data folder for Deluxe8 (its `-r` root).
pub fn root(paths: &AppPaths) -> PathBuf {
    paths.data.join("online")
}

pub fn exe(engine_dir: &Path) -> PathBuf {
    engine_dir.join(if cfg!(windows) { "Ryujinx.exe" } else { "Ryujinx" })
}

/// True when a Nextendo account is linked in Ryujinx Nextendo (the account file exists; its content is not read).
pub fn signed_in(paths: &AppPaths) -> bool {
    let root = root(paths);
    [root.join("nextendo_account.txt"), root.join("system").join("nextendo").join("nextendo_account.txt")]
        .iter()
        .any(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0))
}

/// Hard link (same volume, no extra space) or copy.
fn link_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() {
        return Ok(());
    }
    std::fs::hard_link(src, dst).or_else(|_| std::fs::copy(src, dst).map(|_| ()))
}

/// Ryujinx's names for the game's language and region.
fn language_and_region(s: &Settings) -> (&'static str, &'static str) {
    let region = match s.system.region.as_str() {
        "usa" => "USA",
        "jpn" => "Japan",
        _ => "Europe",
    };
    let language = match s.game_language() {
        "ja" => "Japanese",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        _ if region == "USA" => "AmericanEnglish",
        _ => "BritishEnglish",
    };
    (language, region)
}

/// The DLC contents of one package for Ryujinx's `dlc.json`: every NCA but the metadata one, under the package's title.
fn dlc_entry(package: &Path) -> Result<serde_json::Value, String> {
    let info = d8_nx::package::inspect(package).map_err(|e| e.to_string())?;
    let title_id = info.title_id.ok_or_else(|| crate::tr!("No title id in {file}", file = package.display()))?;
    let file = std::fs::File::open(package).map_err(|e| e.to_string())?;
    let pfs0 = d8_nx::pfs0::Pfs0::read(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let ncas: Vec<serde_json::Value> = pfs0
        .entries()
        .iter()
        .map(|e| e.name.to_ascii_lowercase())
        .filter(|n| n.ends_with(".nca") && !n.ends_with(".cnmt.nca"))
        .map(|n| json!({ "path": format!("/{n}"), "title_id": title_id, "is_enabled": true }))
        .collect();
    Ok(json!({ "path": package.to_string_lossy(), "dlc_nca_list": ncas }))
}

/// Keys, firmware, update and DLC where Ryujinx looks for them. Cheap after the first time (links and small files).
pub fn prepare(paths: &AppPaths, manifest: &Manifest) -> Result<(), String> {
    let root = root(paths);
    let io = |e: std::io::Error| e.to_string();
    std::fs::create_dir_all(&root).map_err(io)?;
    // Deluxe8 does the job of Ryujinx Nextendo's first-start wizard (keys, firmware, game): mark it done, and this
    // version's "what's new" as seen, so the game starts straight away instead of waiting behind two dialogs. The
    // wizard stays one click away in Ryujinx (Help menu); the notes are on Nextendo's release page.
    for (flag, value) in [("nextendo_setup_done", "1"), ("nextendo_patchnote_version", crate::components::ONLINE_VERSION)] {
        let path = root.join(flag);
        if !path.exists() {
            std::fs::write(path, value).map_err(io)?;
        }
    }
    // keys: real copies (Ryujinx owns its system folder)
    let system = root.join("system");
    std::fs::create_dir_all(&system).map_err(io)?;
    for name in ["prod.keys", "title.keys"] {
        let src = paths.compat_user.join("keys").join(name);
        if src.is_file() {
            std::fs::copy(&src, system.join(name)).map_err(io)?;
        }
    }
    // firmware: the NCAs the preparation installed for the Emulator engine, in Ryujinx's own layout: one folder per
    // NCA, "<id>.nca/00" (its installer drops a ".cnmt" part the same way), the file itself a hard link
    let fw_src = paths.compat_user.join("nand/system/Contents/registered");
    let fw_dst = root.join("bis/system/Contents/registered");
    std::fs::create_dir_all(&fw_dst).map_err(io)?;
    let ncas: Vec<(String, PathBuf)> = std::fs::read_dir(&fw_src)
        .map_err(|e| crate::tr!("Firmware not installed yet ({e}): run the preparation again", e = e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("nca")))
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().to_string();
            Some((format!("{}.nca", name.split('.').next()?), p))
        })
        .collect();
    if ncas.is_empty() {
        return Err(crate::tr!("Firmware not installed yet: run the preparation again").into());
    }
    for old in std::fs::read_dir(&fw_dst).map_err(io)?.flatten() {
        let name = old.file_name().to_string_lossy().to_string();
        if !ncas.iter().any(|(dir, _)| *dir == name) {
            let p = old.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
    for (dir, nca) in &ncas {
        let folder = fw_dst.join(dir);
        std::fs::create_dir_all(&folder).map_err(io)?;
        link_or_copy(nca, &folder.join("00")).map_err(io)?;
    }
    // the update and the DLC, by path (Ryujinx reads them in place)
    let games = root.join("games").join(TITLE_ID.to_ascii_lowercase());
    std::fs::create_dir_all(&games).map_err(io)?;
    let updates = json!({ "selected": manifest.update, "paths": [manifest.update] });
    std::fs::write(games.join("updates.json"), serde_json::to_vec_pretty(&updates).unwrap_or_default()).map_err(io)?;
    let dlc: Vec<serde_json::Value> =
        manifest.dlc.iter().map(|d| dlc_entry(Path::new(d))).collect::<Result<_, _>>()?;
    std::fs::write(games.join("dlc.json"), serde_json::to_vec_pretty(&dlc).unwrap_or_default()).map_err(io)?;
    Ok(())
}

/// Ryujinx Nextendo with Deluxe8's data folder; with `game`, straight into the game.
pub fn command(engine_dir: &Path, paths: &AppPaths, s: &Settings, game: Option<&str>) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe(engine_dir));
    cmd.current_dir(engine_dir).arg("-r").arg(root(paths));
    if let Some(game) = game {
        let (language, region) = language_and_region(s);
        if s.display.fullscreen {
            cmd.arg("--fullscreen");
        }
        cmd.args(["--docked-mode", "--system-language", language, "--system-region", region]).arg(game);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ryujinx_language_names() {
        let mut s = Settings::default();
        s.system.language = "it".into();
        assert_eq!(language_and_region(&s), ("Italian", "Europe"));
        s.system.language = "en".into();
        s.system.region = "usa".into();
        assert_eq!(language_and_region(&s), ("AmericanEnglish", "USA"));
    }
}
