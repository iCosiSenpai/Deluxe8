//! One-time preparation: keys and firmware into the engine's data folder, NSZ -> NSP
//! conversion (verified), content layout and engine configuration.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::paths::AppPaths;
use crate::settings::{EngineKind, Settings};
use crate::{eden, logs};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareStep {
    pub id: String,
    pub label: String,
    pub state: String,
    pub detail: Option<String>,
    pub progress: Option<f64>,
}

/// What the preparation produced; read by the launcher before starting an engine.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub base: String,
    pub update: String,
    pub dlc: Vec<String>,
    pub update_version: u32,
}

impl Manifest {
    pub fn path(paths: &AppPaths) -> PathBuf {
        paths.content.join("manifest.json")
    }
    pub fn load(paths: &AppPaths) -> Option<Manifest> {
        serde_json::from_str(&std::fs::read_to_string(Self::path(paths)).ok()?).ok()
    }
    fn save(&self, paths: &AppPaths) -> std::io::Result<()> {
        std::fs::write(Self::path(paths), serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)
    }
}

struct Reporter {
    app: AppHandle,
}

impl Reporter {
    fn step(&self, id: &str, label: &str, state: &str, detail: Option<String>, progress: Option<f64>) {
        let _ = self.app.emit(
            "prepare://step",
            PrepareStep { id: id.into(), label: label.into(), state: state.into(), detail, progress },
        );
    }
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    // Same volume: a hard link costs nothing. Otherwise copy.
    if std::fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ())
}

fn same_size(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len(),
        _ => false,
    }
}

/// A real copy, never a link: the engine owns its keys folder, and a write there must not reach the player's own file.
fn copy_small(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if dst.exists() {
        std::fs::remove_file(dst)?; // it may be a hard link made by an older Deluxe8
    }
    std::fs::copy(src, dst).map(|_| ())
}

fn install_keys(paths: &AppPaths, s: &Settings) -> Result<String, String> {
    let keys_dir = paths.compat_user.join("keys");
    copy_small(Path::new(&s.paths.prod_keys), &keys_dir.join("prod.keys")).map_err(|e| format!("prod.keys: {e}"))?;
    let mut files = vec!["prod.keys"];
    if !s.paths.title_keys.is_empty() && Path::new(&s.paths.title_keys).is_file() {
        copy_small(Path::new(&s.paths.title_keys), &keys_dir.join("title.keys"))
            .map_err(|e| format!("title.keys: {e}"))?;
        files.push("title.keys");
    }
    Ok(crate::tr!("Copied {files}", files = files.join(", ")))
}

/// Removes packages an earlier preparation left in a content folder and this one no longer uses (another update, a DLC
/// taken off the list, a cancelled conversion): the Compat engine loads every package it finds there. Only Deluxe8's own
/// file names are touched.
fn prune_content(dir: &Path, keep: &[&Path]) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let ours = ["base-", "update-", "dlc-"].iter().any(|p| name.starts_with(p))
            && [".nsp", ".nsp.source", ".nsp.part"].iter().any(|s| name.ends_with(s));
        let kept = keep.iter().any(|k| {
            path == *k || path == k.with_extension("nsp.source") || path == k.with_extension("nsp.part")
        });
        if ours && !kept && path.is_file() {
            match std::fs::remove_file(&path) {
                Ok(()) => logs::write(&format!("removed stale content {}", path.display())),
                Err(e) => logs::write(&format!("stale content {} not removed: {e}", path.display())),
            }
        }
    }
}

fn firmware_marker(source: &str, count: usize) -> String {
    format!("{source}\n{count}\n")
}

/// Mirrors Eden's own firmware installer: clean `nand/system/Contents/registered`, then copy
/// every `.nca` with its original file name.
fn install_firmware(paths: &AppPaths, s: &Settings, rep: &Reporter, cancel: &AtomicBool) -> Result<String, String> {
    let src = Path::new(&s.paths.firmware);
    let registered = paths.compat_user.join("nand/system/Contents/registered");
    let marker = registered.join(".deluxe8-firmware");
    let is_zip = src.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));

    let entries: Vec<(String, Option<PathBuf>)> = if is_zip {
        let file = std::fs::File::open(src).map_err(|e| e.to_string())?;
        let zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        zip.file_names()
            .filter_map(|n| n.ok())
            .filter(|n| n.to_lowercase().ends_with(".nca"))
            .map(|n| (n.to_string(), None))
            .collect()
    } else {
        std::fs::read_dir(src)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(".nca") && e.path().is_file())
            .map(|e| (e.file_name().to_string_lossy().to_string(), Some(e.path())))
            .collect()
    };
    if entries.is_empty() {
        return Err(crate::tr!("No .nca files found in the firmware").into());
    }
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(&firmware_marker(&s.paths.firmware, entries.len())) {
        return Ok(crate::tr!("Already installed").into());
    }
    if registered.exists() {
        std::fs::remove_dir_all(&registered).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&registered).map_err(|e| e.to_string())?;

    let total = entries.len();
    if is_zip {
        let file = std::fs::File::open(src).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for (i, (name, _)) in entries.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            let mut entry = zip.by_name(name).map_err(|e| e.to_string())?;
            let file_name = Path::new(name).file_name().unwrap_or_default();
            let mut out = std::fs::File::create(registered.join(file_name)).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            rep.step("firmware", L_FIRMWARE(), "running", None, Some((i + 1) as f64 / total as f64));
        }
    } else {
        for (i, (name, path)) in entries.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            std::fs::copy(path.as_ref().unwrap(), registered.join(name)).map_err(|e| e.to_string())?;
            if i % 8 == 0 || i + 1 == total {
                rep.step("firmware", L_FIRMWARE(), "running", None, Some((i + 1) as f64 / total as f64));
            }
        }
    }
    std::fs::write(&marker, firmware_marker(&s.paths.firmware, total)).map_err(|e| e.to_string())?;
    Ok(crate::tr!("{total} NCA files installed", total = total))
}

/// Converts an NSZ to a verified NSP in `dest_dir`, or links/copies an NSP. Returns the result path.
fn place_package(
    src: &Path,
    dest_dir: &Path,
    dest_name: &str,
    id: &str,
    label: &str,
    rep: &Reporter,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(dest_name);
    let is_nsz = src.extension().is_some_and(|e| e.eq_ignore_ascii_case("nsz"));
    if !is_nsz {
        if !(dest.exists() && same_size(src, &dest)) {
            copy_file(src, &dest).map_err(|e| e.to_string())?;
        }
        return Ok(dest);
    }
    // A previous verified conversion of the same source is kept next to a stamp file.
    let stamp = dest.with_extension("nsp.source");
    let src_meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    let stamp_text = format!("{}\n{}\n", src.display(), src_meta.len());
    if dest.exists() && std::fs::read_to_string(&stamp).ok().as_deref() == Some(stamp_text.as_str()) {
        return Ok(dest);
    }
    let mut last = std::time::Instant::now();
    let mut progress = |done: u64, total: u64, _name: &str| {
        if last.elapsed().as_millis() > 200 || done == total {
            last = std::time::Instant::now();
            let frac = if total > 0 { done as f64 / total as f64 } else { 0.0 };
            rep.step(id, label, "running", Some(crate::tr!("Decompressing and checking SHA-256").into()), Some(frac));
        }
    };
    let summary = d8_nx::ncz::nsz_to_nsp(src, &dest, &mut progress, cancel).map_err(|e| e.to_string())?;
    std::fs::write(&stamp, stamp_text).map_err(|e| e.to_string())?;
    logs::write(&format!(
        "converted {} -> {} ({} bytes, {:.1}s)",
        src.display(),
        dest.display(),
        summary.total_output_size,
        summary.elapsed.as_secs_f64()
    ));
    Ok(dest)
}

// Preparation step labels, in the launcher's language.
#[allow(non_snake_case)]
fn L_KEYS() -> &'static str {
    crate::tr!("Copying the keys")
}
#[allow(non_snake_case)]
fn L_FIRMWARE() -> &'static str {
    crate::tr!("Installing the firmware")
}
#[allow(non_snake_case)]
fn L_BASE() -> &'static str {
    crate::tr!("Base game to NSP")
}
#[allow(non_snake_case)]
fn L_UPDATE() -> &'static str {
    crate::tr!("Update to NSP")
}
#[allow(non_snake_case)]
fn L_DLC() -> &'static str {
    crate::tr!("Extra content")
}
#[allow(non_snake_case)]
fn L_CONFIG() -> &'static str {
    crate::tr!("Configuring the engine")
}

fn version_label(v: u32) -> String {
    match v {
        0 => "1.0.0".into(),
        x if x == 20 << 16 => "3.0.4".into(),
        x if x == 23 << 16 => "4.0.1".into(),
        x => format!("v{x}"),
    }
}

pub fn run(app: AppHandle, paths: AppPaths, s: Settings, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let rep = Reporter { app };
    paths.ensure().map_err(|e| e.to_string())?;
    let steps: [(&str, &str); 6] = [
        ("keys", L_KEYS()),
        ("firmware", L_FIRMWARE()),
        ("base", L_BASE()),
        ("update", L_UPDATE()),
        ("dlc", L_DLC()),
        ("config", L_CONFIG()),
    ];
    for (id, label) in steps {
        rep.step(id, label, "pending", None, None);
    }
    let fail = |id: &str, label: &str, e: String| {
        rep.step(id, label, "error", Some(e.clone()), None);
        logs::write(&format!("prepare {id} failed: {e}"));
        e
    };

    rep.step("keys", L_KEYS(), "running", None, None);
    let d = install_keys(&paths, &s).map_err(|e| fail("keys", L_KEYS(), e))?;
    rep.step("keys", L_KEYS(), "done", Some(d), Some(1.0));

    rep.step("firmware", L_FIRMWARE(), "running", None, Some(0.0));
    let d = install_firmware(&paths, &s, &rep, &cancel).map_err(|e| fail("firmware", L_FIRMWARE(), e))?;
    rep.step("firmware", L_FIRMWARE(), "done", Some(d), Some(1.0));

    let base_info = d8_nx::package::inspect(&s.paths.base).map_err(|e| fail("base", L_BASE(), e.to_string()))?;
    rep.step("base", L_BASE(), "running", None, Some(0.0));
    let base = place_package(
        Path::new(&s.paths.base),
        &paths.content_base,
        &format!("base-{}.nsp", version_label(base_info.version)),
        "base",
        L_BASE(),
        &rep,
        &cancel,
    )
    .map_err(|e| fail("base", L_BASE(), e))?;
    rep.step("base", L_BASE(), "done", Some(base.display().to_string()), Some(1.0));

    let update_info =
        d8_nx::package::inspect(&s.paths.update).map_err(|e| fail("update", L_UPDATE(), e.to_string()))?;
    rep.step("update", L_UPDATE(), "running", None, Some(0.0));
    let update = place_package(
        Path::new(&s.paths.update),
        &paths.content_updates,
        &format!("update-{}.nsp", version_label(update_info.version)),
        "update",
        L_UPDATE(),
        &rep,
        &cancel,
    )
    .map_err(|e| fail("update", L_UPDATE(), e))?;
    rep.step("update", L_UPDATE(), "done", Some(update.display().to_string()), Some(1.0));

    let mut dlc = Vec::new();
    if s.paths.dlc.is_empty() {
        rep.step("dlc", L_DLC(), "skipped", Some(crate::tr!("No DLC").into()), None);
    } else {
        rep.step("dlc", L_DLC(), "running", None, None);
        for (i, path) in s.paths.dlc.iter().enumerate() {
            let src = Path::new(path);
            let name = format!("dlc-{i}.{}", src.extension().and_then(|e| e.to_str()).unwrap_or("nsp"));
            let placed = place_package(src, &paths.content_updates, &name.replace(".nsz", ".nsp"), "dlc", L_DLC(), &rep, &cancel)
                .map_err(|e| fail("dlc", L_DLC(), e))?;
            dlc.push(placed.display().to_string());
        }
        rep.step("dlc", L_DLC(), "done", Some(crate::tr!("{n} packages", n = dlc.len())), Some(1.0));
    }

    let dlc_paths: Vec<PathBuf> = dlc.iter().map(PathBuf::from).collect();
    let mut keep_updates: Vec<&Path> = vec![update.as_path()];
    keep_updates.extend(dlc_paths.iter().map(PathBuf::as_path));
    prune_content(&paths.content_updates, &keep_updates);
    prune_content(&paths.content_base, &[base.as_path()]);

    rep.step("config", L_CONFIG(), "running", None, None);
    eden::write_config(&paths.compat_user, &s).map_err(|e| fail("config", L_CONFIG(), e.to_string()))?;
    Manifest {
        base: base.display().to_string(),
        update: update.display().to_string(),
        dlc,
        update_version: update_info.version,
    }
    .save(&paths)
    .map_err(|e| fail("config", L_CONFIG(), e.to_string()))?;
    rep.step("config", L_CONFIG(), "done", None, Some(1.0));

    if s.engine == EngineKind::Online {
        prepare_online(&rep, &paths, &cancel)?;
    }
    if s.engine == EngineKind::Native {
        prepare_native(&rep, &paths, &s, &cancel)?;
    }
    Ok(())
}

/// Online engine: download Ryujinx Nextendo if needed and put the game, keys and firmware where it reads them.
fn prepare_online(rep: &Reporter, paths: &AppPaths, cancel: &AtomicBool) -> Result<(), String> {
    let label = crate::tr!("Ryujinx Nextendo");
    rep.step("online-engine", label, "running", None, Some(0.0));
    let root = crate::components::ensure_online_engine(
        paths,
        &mut |detail, progress| rep.step("online-engine", label, "running", Some(detail), progress),
        cancel,
    )
    .inspect_err(|e| rep.step("online-engine", label, "error", Some(e.clone()), None))?;
    rep.step("online-engine", label, "done", Some(root.display().to_string()), Some(1.0));
    let manifest = Manifest::load(paths).ok_or(crate::tr!("The game files are not ready"))?;
    let setup = crate::tr!("Game, keys and firmware for Ryujinx Nextendo");
    rep.step("online", setup, "running", None, None);
    crate::nextendo::prepare(paths, &manifest).inspect_err(|e| rep.step("online", setup, "error", Some(e.clone()), None))?;
    rep.step("online", setup, "done", None, Some(1.0));
    Ok(())
}

/// Native engine: download it if this PC has none, build the game for it, and (if asked, on Windows) download and build DLSS.
fn prepare_native(rep: &Reporter, paths: &AppPaths, s: &Settings, cancel: &AtomicBool) -> Result<(), String> {
    let engine_step: (&str, &str) = ("native-engine", crate::tr!("Native engine"));
    let dlss_step: (&str, &str) = ("dlss", crate::tr!("NVIDIA DLSS for this PC"));
    rep.step(engine_step.0, engine_step.1, "pending", None, None);
    rep.step("native", crate::tr!("Recompiling the game (Native engine)"), "pending", None, None);
    if s.native.dlss {
        rep.step(dlss_step.0, dlss_step.1, "pending", None, None);
    }

    rep.step(engine_step.0, engine_step.1, "running", None, Some(0.0));
    let had_engine = paths.native_engine_dir();
    let root = crate::components::ensure_native_engine(
        paths,
        &mut |detail, progress| rep.step(engine_step.0, engine_step.1, "running", Some(detail), progress),
        cancel,
    )
    .inspect_err(|e| rep.step(engine_step.0, engine_step.1, "error", Some(e.clone()), None))?;
    let detail = match had_engine {
        Some(_) => crate::tr!("Already here ({path})", path = root.display()),
        None => crate::tr!("Racing 8 DX Recomp v{version} downloaded from GitHub", version = crate::components::NATIVE_VERSION),
    };
    rep.step(engine_step.0, engine_step.1, "done", Some(detail), Some(1.0));

    crate::native::prepare(
        &|state, detail, progress| rep.step("native", crate::tr!("Recompiling the game (Native engine)"), state, detail, progress),
        paths,
        s,
        cancel,
    )?;

    if s.native.dlss {
        rep.step(dlss_step.0, dlss_step.1, "running", None, Some(0.0));
        let result = crate::components::install_dlss(
            paths,
            &root,
            &mut |detail, progress| rep.step(dlss_step.0, dlss_step.1, "running", Some(detail), progress),
            cancel,
        );
        match result {
            Ok(detail) => rep.step(dlss_step.0, dlss_step.1, "done", Some(detail), Some(1.0)),
            Err(e) => {
                // the game still plays (FSR 3.1 / FSR 1): report it, don't fail the whole preparation
                logs::write(&format!("DLSS setup failed: {e}"));
                rep.step(dlss_step.0, dlss_step.1, "error", Some(crate::tr!("{e}. The game still works with FSR.", e = e)), None);
            }
        }
    }
    Ok(())
}

/// Installs one component on request (Sistema > Componenti), with the same step events as the preparation.
pub fn install_component(app: AppHandle, paths: AppPaths, name: &str, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let rep = Reporter { app };
    match name {
        "native" => {
            rep.step("native-engine", crate::tr!("Native engine"), "running", None, Some(0.0));
            let root = crate::components::ensure_native_engine(
                &paths,
                &mut |detail, progress| rep.step("native-engine", crate::tr!("Native engine"), "running", Some(detail), progress),
                &cancel,
            )
            .inspect_err(|e| rep.step("native-engine", crate::tr!("Native engine"), "error", Some(e.clone()), None))?;
            rep.step("native-engine", crate::tr!("Native engine"), "done", Some(root.display().to_string()), Some(1.0));
            Ok(())
        }
        "online" => {
            let label = crate::tr!("Ryujinx Nextendo");
            rep.step("online-engine", label, "running", None, Some(0.0));
            let root = crate::components::ensure_online_engine(
                &paths,
                &mut |detail, progress| rep.step("online-engine", label, "running", Some(detail), progress),
                &cancel,
            )
            .inspect_err(|e| rep.step("online-engine", label, "error", Some(e.clone()), None))?;
            rep.step("online-engine", label, "done", Some(root.display().to_string()), Some(1.0));
            Ok(())
        }
        "dlss" => {
            let root = paths.native_engine_dir().ok_or(crate::tr!("The Native engine comes first"))?;
            rep.step("dlss", crate::tr!("NVIDIA DLSS for this PC"), "running", None, Some(0.0));
            let detail = crate::components::install_dlss(
                &paths,
                &root,
                &mut |detail, progress| rep.step("dlss", crate::tr!("NVIDIA DLSS for this PC"), "running", Some(detail), progress),
                &cancel,
            )
            .inspect_err(|e| rep.step("dlss", crate::tr!("NVIDIA DLSS for this PC"), "error", Some(e.clone()), None))?;
            rep.step("dlss", crate::tr!("NVIDIA DLSS for this PC"), "done", Some(detail), Some(1.0));
            Ok(())
        }
        other => Err(crate::tr!("Unknown component: {name}", name = other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_current_packages_only() {
        let dir = std::env::temp_dir().join(format!("deluxe8-test-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "update-4.0.1.nsp",
            "update-4.0.1.nsp.source",
            "update-3.0.4.nsp",
            "dlc-0.nsp",
            "dlc-1.nsp",
            "dlc-1.nsp.part",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let update = dir.join("update-4.0.1.nsp");
        let dlc = dir.join("dlc-0.nsp");
        prune_content(&dir, &[update.as_path(), dlc.as_path()]);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, ["dlc-0.nsp", "notes.txt", "update-4.0.1.nsp", "update-4.0.1.nsp.source"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
