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
    let mut detail = "prod.keys".to_string();
    if !s.paths.title_keys.is_empty() && Path::new(&s.paths.title_keys).is_file() {
        copy_small(Path::new(&s.paths.title_keys), &keys_dir.join("title.keys"))
            .map_err(|e| format!("title.keys: {e}"))?;
        detail.push_str(" e title.keys");
    }
    Ok(format!("Copiati {detail}"))
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
        return Err("Nessun file .nca trovato nel firmware".into());
    }
    if std::fs::read_to_string(&marker).ok().as_deref() == Some(&firmware_marker(&s.paths.firmware, entries.len())) {
        return Ok("Già installato".into());
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
            rep.step("firmware", "Installazione del firmware", "running", None, Some((i + 1) as f64 / total as f64));
        }
    } else {
        for (i, (name, path)) in entries.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Annullato".into());
            }
            std::fs::copy(path.as_ref().unwrap(), registered.join(name)).map_err(|e| e.to_string())?;
            if i % 8 == 0 || i + 1 == total {
                rep.step("firmware", "Installazione del firmware", "running", None, Some((i + 1) as f64 / total as f64));
            }
        }
    }
    std::fs::write(&marker, firmware_marker(&s.paths.firmware, total)).map_err(|e| e.to_string())?;
    Ok(format!("{total} file NCA installati"))
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
            rep.step(id, label, "running", Some("Decompressione e verifica SHA-256".into()), Some(frac));
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
        ("keys", "Copia delle chiavi"),
        ("firmware", "Installazione del firmware"),
        ("base", "Gioco base in NSP"),
        ("update", "Aggiornamento in NSP"),
        ("dlc", "Contenuti aggiuntivi"),
        ("config", "Configurazione del motore"),
    ];
    for (id, label) in steps {
        rep.step(id, label, "pending", None, None);
    }
    let fail = |id: &str, label: &str, e: String| {
        rep.step(id, label, "error", Some(e.clone()), None);
        logs::write(&format!("prepare {id} failed: {e}"));
        e
    };

    rep.step("keys", "Copia delle chiavi", "running", None, None);
    let d = install_keys(&paths, &s).map_err(|e| fail("keys", "Copia delle chiavi", e))?;
    rep.step("keys", "Copia delle chiavi", "done", Some(d), Some(1.0));

    rep.step("firmware", "Installazione del firmware", "running", None, Some(0.0));
    let d = install_firmware(&paths, &s, &rep, &cancel).map_err(|e| fail("firmware", "Installazione del firmware", e))?;
    rep.step("firmware", "Installazione del firmware", "done", Some(d), Some(1.0));

    let base_info = d8_nx::package::inspect(&s.paths.base).map_err(|e| fail("base", "Gioco base in NSP", e.to_string()))?;
    rep.step("base", "Gioco base in NSP", "running", None, Some(0.0));
    let base = place_package(
        Path::new(&s.paths.base),
        &paths.content_base,
        &format!("base-{}.nsp", version_label(base_info.version)),
        "base",
        "Gioco base in NSP",
        &rep,
        &cancel,
    )
    .map_err(|e| fail("base", "Gioco base in NSP", e))?;
    rep.step("base", "Gioco base in NSP", "done", Some(base.display().to_string()), Some(1.0));

    let update_info =
        d8_nx::package::inspect(&s.paths.update).map_err(|e| fail("update", "Aggiornamento in NSP", e.to_string()))?;
    rep.step("update", "Aggiornamento in NSP", "running", None, Some(0.0));
    let update = place_package(
        Path::new(&s.paths.update),
        &paths.content_updates,
        &format!("update-{}.nsp", version_label(update_info.version)),
        "update",
        "Aggiornamento in NSP",
        &rep,
        &cancel,
    )
    .map_err(|e| fail("update", "Aggiornamento in NSP", e))?;
    rep.step("update", "Aggiornamento in NSP", "done", Some(update.display().to_string()), Some(1.0));

    let mut dlc = Vec::new();
    if s.paths.dlc.is_empty() {
        rep.step("dlc", "Contenuti aggiuntivi", "skipped", Some("Nessun DLC".into()), None);
    } else {
        rep.step("dlc", "Contenuti aggiuntivi", "running", None, None);
        for (i, path) in s.paths.dlc.iter().enumerate() {
            let src = Path::new(path);
            let name = format!("dlc-{i}.{}", src.extension().and_then(|e| e.to_str()).unwrap_or("nsp"));
            let placed = place_package(src, &paths.content_updates, &name.replace(".nsz", ".nsp"), "dlc", "Contenuti aggiuntivi", &rep, &cancel)
                .map_err(|e| fail("dlc", "Contenuti aggiuntivi", e))?;
            dlc.push(placed.display().to_string());
        }
        rep.step("dlc", "Contenuti aggiuntivi", "done", Some(format!("{} pacchetti", dlc.len())), Some(1.0));
    }

    let dlc_paths: Vec<PathBuf> = dlc.iter().map(PathBuf::from).collect();
    let mut keep_updates: Vec<&Path> = vec![update.as_path()];
    keep_updates.extend(dlc_paths.iter().map(PathBuf::as_path));
    prune_content(&paths.content_updates, &keep_updates);
    prune_content(&paths.content_base, &[base.as_path()]);

    rep.step("config", "Configurazione del motore", "running", None, None);
    eden::write_config(&paths.compat_user, &s).map_err(|e| fail("config", "Configurazione del motore", e.to_string()))?;
    Manifest {
        base: base.display().to_string(),
        update: update.display().to_string(),
        dlc,
        update_version: update_info.version,
    }
    .save(&paths)
    .map_err(|e| fail("config", "Configurazione del motore", e.to_string()))?;
    rep.step("config", "Configurazione del motore", "done", None, Some(1.0));

    if s.engine == EngineKind::Native {
        prepare_native(&rep, &paths, &s, &cancel)?;
    }
    Ok(())
}

/// Native engine: download it if this PC has none, build the game for it, and (if asked, on Windows) download and build DLSS.
fn prepare_native(rep: &Reporter, paths: &AppPaths, s: &Settings, cancel: &AtomicBool) -> Result<(), String> {
    const ENGINE: (&str, &str) = ("native-engine", "Motore Nativo");
    const DLSS: (&str, &str) = ("dlss", "NVIDIA DLSS per questo PC");
    rep.step(ENGINE.0, ENGINE.1, "pending", None, None);
    rep.step("native", "Ricompilazione del gioco (motore Nativo)", "pending", None, None);
    if s.native.dlss {
        rep.step(DLSS.0, DLSS.1, "pending", None, None);
    }

    rep.step(ENGINE.0, ENGINE.1, "running", None, Some(0.0));
    let had_engine = paths.native_engine_dir();
    let root = crate::components::ensure_native_engine(
        paths,
        &mut |detail, progress| rep.step(ENGINE.0, ENGINE.1, "running", Some(detail), progress),
        cancel,
    )
    .inspect_err(|e| rep.step(ENGINE.0, ENGINE.1, "error", Some(e.clone()), None))?;
    let detail = match had_engine {
        Some(_) => format!("Già presente ({})", root.display()),
        None => format!("Racing 8 DX Recomp v{} scaricato da GitHub", crate::components::NATIVE_VERSION),
    };
    rep.step(ENGINE.0, ENGINE.1, "done", Some(detail), Some(1.0));

    crate::native::prepare(
        &|state, detail, progress| rep.step("native", "Ricompilazione del gioco (motore Nativo)", state, detail, progress),
        paths,
        s,
        cancel,
    )?;

    if s.native.dlss {
        rep.step(DLSS.0, DLSS.1, "running", None, Some(0.0));
        let result = crate::components::install_dlss(
            paths,
            &root,
            &mut |detail, progress| rep.step(DLSS.0, DLSS.1, "running", Some(detail), progress),
            cancel,
        );
        match result {
            Ok(detail) => rep.step(DLSS.0, DLSS.1, "done", Some(detail), Some(1.0)),
            Err(e) => {
                // the game still plays (FSR 3.1 / FSR 1): report it, don't fail the whole preparation
                logs::write(&format!("DLSS setup failed: {e}"));
                rep.step(DLSS.0, DLSS.1, "error", Some(format!("{e}. Il gioco funziona lo stesso con FSR.")), None);
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
            rep.step("native-engine", "Motore Nativo", "running", None, Some(0.0));
            let root = crate::components::ensure_native_engine(
                &paths,
                &mut |detail, progress| rep.step("native-engine", "Motore Nativo", "running", Some(detail), progress),
                &cancel,
            )
            .inspect_err(|e| rep.step("native-engine", "Motore Nativo", "error", Some(e.clone()), None))?;
            rep.step("native-engine", "Motore Nativo", "done", Some(root.display().to_string()), Some(1.0));
            Ok(())
        }
        "dlss" => {
            let root = paths.native_engine_dir().ok_or("Prima serve il motore Nativo")?;
            rep.step("dlss", "NVIDIA DLSS per questo PC", "running", None, Some(0.0));
            let detail = crate::components::install_dlss(
                &paths,
                &root,
                &mut |detail, progress| rep.step("dlss", "NVIDIA DLSS per questo PC", "running", Some(detail), progress),
                &cancel,
            )
            .inspect_err(|e| rep.step("dlss", "NVIDIA DLSS per questo PC", "error", Some(e.clone()), None))?;
            rep.step("dlss", "NVIDIA DLSS per questo PC", "done", Some(detail), Some(1.0));
            Ok(())
        }
        other => Err(format!("componente sconosciuto: {other}")),
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
