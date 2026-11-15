//! Native engine (static recompilation, Racing 8 DX Recomp toolchain).
//!
//! Engine folder (bundled, downloaded by [`crate::components`] or built by `tools/native.ps1`): `kartrecomp-setup(.exe)`,
//! `engine/mk8d-engine(.exe)`, `engine/plugins/`, `toolchain/`, `data/`. It is never written to.
//! Everything the engine and its setup write goes into Deluxe8's data (`<data>/native`): the XDG roots `data/`, `cache/` and
//! `config/`, each with a `kartrecomp` folder (`KARTRECOMP_APP_NAME=kartrecomp`). The recompiled game is built there by
//! `kartrecomp-setup` from the NSPs Deluxe8 converted.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter};
use tokio::process::Command;

use crate::logs;
use crate::paths::{AppPaths, exe_name};
use crate::prepare::Manifest;
use crate::settings::Settings;

/// The engine's folders inside `<data>/native`.
pub struct NativeLayout {
    pub root: PathBuf,
    pub user: PathBuf,
}

impl NativeLayout {
    pub fn new(paths: &AppPaths, root: PathBuf) -> NativeLayout {
        NativeLayout { root, user: paths.native_user.clone() }
    }
    fn xdg_data(&self) -> PathBuf {
        self.user.join("data")
    }
    fn xdg_cache(&self) -> PathBuf {
        self.user.join("cache")
    }
    fn xdg_config(&self) -> PathBuf {
        self.user.join("config")
    }
    /// `DataLayout::dataRoot` of the setup tool: content, keys, firmware, the built modules, logs, saves.
    pub fn app_data(&self) -> PathBuf {
        self.xdg_data().join("kartrecomp")
    }
    pub fn config_dir(&self) -> PathBuf {
        self.xdg_config().join("kartrecomp")
    }
    fn content(&self) -> PathBuf {
        self.app_data().join("content")
    }
    /// The setup tool's fixed names (it calls any update "3.0.4" in its file name).
    pub fn base(&self) -> PathBuf {
        self.content().join("base.nsp")
    }
    pub fn update(&self) -> PathBuf {
        self.content().join("update-3.0.4.nsp")
    }
    fn dlc_dir(&self) -> PathBuf {
        self.content().join("dlc")
    }
    pub fn module_dir(&self) -> PathBuf {
        self.app_data().join("module")
    }
    pub fn log_file(&self) -> PathBuf {
        self.app_data().join("log").join("suyu_log.txt")
    }
    fn engine_exe(&self) -> PathBuf {
        self.root.join("engine").join(exe_name("mk8d-engine"))
    }
    fn engine_version(&self) -> String {
        std::fs::read_to_string(self.root.join("engine").join("ENGINE_VERSION")).unwrap_or_default().trim().to_string()
    }
    fn dlc_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(self.dlc_dir())
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "nsp")).collect())
            .unwrap_or_default();
        out.sort();
        out
    }
    fn apply_env(&self, cmd: &mut std::process::Command) {
        cmd.env("KARTRECOMP_APP_NAME", "kartrecomp")
            .env("XDG_DATA_HOME", self.xdg_data())
            .env("XDG_CACHE_HOME", self.xdg_cache())
            .env("XDG_CONFIG_HOME", self.xdg_config());
    }
}

fn layout(paths: &AppPaths) -> Result<NativeLayout, String> {
    let root = paths.native_engine_dir().ok_or(crate::tr!("The Native engine is not on this PC: run the preparation again"))?;
    Ok(NativeLayout::new(paths, root))
}

/// Hard link when Deluxe8's content and the native data share a volume (no extra space), copy otherwise.
fn place(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let same = |a: &Path, b: &Path| match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.len() == y.len() && x.modified().ok() == y.modified().ok(),
        _ => false,
    };
    if same(src, dst) {
        return Ok(());
    }
    if dst.exists() {
        std::fs::remove_file(dst)?;
    }
    if std::fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map(|_| ())
}

fn update_label(v: u32) -> &'static str {
    match v {
        x if x == 23 << 16 => "4.0.1",
        x if x == 20 << 16 => "3.0.4",
        _ => "",
    }
}

/// Build-time code patches of the setup tool (`--patch`) the settings ask for.
fn wanted_patches(s: &Settings, manifest: &Manifest) -> Vec<&'static str> {
    let mut out = Vec::new();
    if s.controls.split60 && matches!(update_label(manifest.update_version), "4.0.1" | "3.0.4") {
        out.push("split60"); // 3-4 player split screen at 60 fps
    }
    out
}

/// Deluxe8's note of the patches the modules were built with (`module/deluxe8.json`).
fn patches_stamp(patches: &[&str]) -> String {
    serde_json::json!({ "patches": patches }).to_string()
}

/// True when the modules in the native data were built by this engine version, for this update and with these patches.
fn already_built(l: &NativeLayout, manifest: &Manifest, patches: &[&str]) -> bool {
    let image = l.module_dir().join(if cfg!(windows) { "recompiled_image.dll" } else { "librecompiled_image.so" });
    let Some(stamp) = std::fs::read_to_string(l.module_dir().join("module.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    else {
        return false;
    };
    let field = |key: &str| stamp.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let ours = std::fs::read_to_string(l.module_dir().join("deluxe8.json")).unwrap_or_else(|_| patches_stamp(&[]));
    let version = l.engine_version();
    image.is_file()
        && !version.is_empty()
        && field("engineVersion") == version
        && field("update") == update_label(manifest.update_version)
        && ours == patches_stamp(patches)
}

/// Puts the converted NSPs where the setup tool keeps them (so it finds them "already in place" instead of moving the
/// player's files), then runs `kartrecomp-setup` (export to C, compile, verify) unless the game is already built.
/// Receives a preparation step's state (`running`, `done`, `error`), detail and progress.
pub type StepReport<'a> = &'a dyn Fn(&str, Option<String>, Option<f64>);

/// `report(state, detail, progress)` gets the step's progress.
pub fn prepare(
    report: StepReport<'_>,
    paths: &AppPaths,
    s: &Settings,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let l = layout(paths)?;
    let manifest = Manifest::load(paths).ok_or(crate::tr!("The game files are not ready"))?;
    let emit = report;
    emit("running", Some(crate::tr!("Game files into the engine's folder").into()), Some(0.0));
    let placed = (|| -> std::io::Result<()> {
        place(Path::new(&manifest.base), &l.base())?;
        place(Path::new(&manifest.update), &l.update())?;
        std::fs::create_dir_all(l.dlc_dir())?;
        let wanted: Vec<String> = manifest
            .dlc
            .iter()
            .filter_map(|d| Path::new(d).file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();
        for old in l.dlc_files() {
            if !old.file_name().is_some_and(|n| wanted.contains(&n.to_string_lossy().to_string())) {
                std::fs::remove_file(old)?;
            }
        }
        for dlc in &manifest.dlc {
            let src = Path::new(dlc);
            place(src, &l.dlc_dir().join(src.file_name().unwrap_or_default()))?;
        }
        Ok(())
    })();
    placed.map_err(|e| crate::tr!("Game files not prepared for the Native engine: {e}", e = e))?;

    let patches = wanted_patches(s, &manifest);
    if already_built(&l, &manifest, &patches) {
        emit("done", Some(crate::tr!("Game already recompiled for this engine version").into()), Some(1.0));
        return Ok(());
    }

    emit("running", Some(crate::tr!("Translating the ARM64 code to C and compiling").into()), Some(0.0));
    let setup = l.root.join(exe_name("kartrecomp-setup"));
    let mut cmd = std::process::Command::new(&setup);
    cmd.current_dir(&l.root)
        .args(["--base".as_ref(), l.base().as_os_str()])
        .args(["--update".as_ref(), l.update().as_os_str()])
        .args(["--keys", &s.paths.prod_keys, "--firmware", &s.paths.firmware])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for dlc in l.dlc_files() {
        cmd.args(["--dlc".as_ref(), dlc.as_os_str()]);
    }
    for patch in &patches {
        cmd.args(["--patch", patch]);
    }
    l.apply_env(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    logs::write(&format!("native setup: {cmd:?}"));
    let mut child = cmd.spawn().map_err(|e| format!("kartrecomp-setup: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stderr), &mut text);
        text
    });
    for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)).map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err(crate::tr!("Cancelled").into());
        }
        // "PROGRESS <permille> <text>"
        if let Some(rest) = line.strip_prefix("PROGRESS ") {
            let (num, text) = rest.split_once(' ').unwrap_or((rest, ""));
            let permille: f64 = num.parse().unwrap_or(0.0);
            emit("running", Some(text.to_string()), Some(permille / 1000.0));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let reason = err_reader.join().unwrap_or_default().trim().to_string();
    if !status.success() {
        emit("error", Some(reason.clone()), None);
        return Err(crate::tr!("Recompilation failed: {reason}", reason = reason));
    }
    let _ = std::fs::write(l.module_dir().join("deluxe8.json"), patches_stamp(&patches));
    emit("done", Some(crate::tr!("Game recompiled and verified").into()), Some(1.0));
    Ok(())
}

/// The scene scales the engine's "3D render scale" row offers (percent, row index): 0 is "off", 1 is 100 % (DLAA / native
/// anti-aliasing). Any other percentage would mean "off", so the nearest offered one is used.
pub const RENDER_SCALES: [(u32, u32); 7] = [(33, 5), (50, 4), (58, 3), (67, 2), (100, 1), (150, 6), (200, 7)];

fn render_scale_row(percent: u32) -> u32 {
    RENDER_SCALES.iter().min_by_key(|(p, _)| p.abs_diff(percent)).map(|(_, row)| *row).unwrap_or(1)
}

/// The in-game menu's file (`kartrecomp_overlay.ini`, flat key=value lines): the launcher's choices are its starting point.
fn write_overlay_ini(l: &NativeLayout, s: &Settings) -> std::io::Result<()> {
    let dir = l.config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("kartrecomp_overlay.ini");
    let mut items: Vec<(String, String)> = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect();
    let mut set = |key: &str, value: String| {
        if let Some(item) = items.iter_mut().find(|(k, _)| k == key) {
            item.1 = value;
        } else {
            items.push((key.to_string(), value));
        }
    };
    let upscaler = match s.image.upscaler.as_str() {
        "dlss" | "dlaa" => "dlss",
        "fsr3" => "fsr3",
        "fsr4" => "fsr4",
        "fsr1" => "fsr1",
        "off" => "off",
        _ => "fsr1",
    };
    set("upscaler", upscaler.into());
    let quality = if upscaler == "off" {
        0
    } else if s.image.upscaler == "dlaa" {
        1
    } else {
        render_scale_row(s.image.render_scale)
    };
    set("quality", quality.to_string());
    // a temporal upscaler needs sub-pixel camera jitter
    set("jitter", if matches!(upscaler, "dlss" | "fsr3" | "fsr4") && quality != 0 { "1" } else { "0" }.into());
    set("sharpness", (s.image.sharpness * 3 / 100).min(3).to_string());
    // frame generation: a plugin's (2x) or the engine's built-in interpolator (2x, 3x or 4x)
    let framegen = match (s.smoothness.frame_gen.as_str(), s.smoothness.multiplier) {
        ("dlss", _) => "dlss",
        ("fsr3", _) => "fsr3",
        ("fsr4", _) => "fsr4",
        ("builtin", 3) => "builtin3",
        ("builtin", 4) => "builtin4",
        ("builtin", _) => "builtin",
        _ => "off",
    };
    set("framegen", framegen.into());
    set(
        "present",
        match s.display.vsync.as_str() {
            "mailbox" => "0",
            "immediate" => "2",
            _ => "1",
        }
        .into(),
    );
    set("hdr", if s.image.hdr { "1" } else { "0" }.into());
    // the HDR rows store the index of the menu's choice, not nits
    let nearest = |choices: &[u32], nits: u32| {
        (0..choices.len()).min_by_key(|&i| choices[i].abs_diff(nits)).unwrap_or(0).to_string()
    };
    set("hdr_paper", nearest(&[100, 150, 203, 250, 300], s.image.hdr_paper_white));
    set("hdr_peak", nearest(&[400, 600, 800, 1000, 1500], s.image.hdr_peak));
    // the game runs at its own pace unless the player turns the limit off (0 = off)
    set("speed_percent", if s.smoothness.speed_limit { "100" } else { "0" }.into());
    set("fps", if s.smoothness.hud == "off" { "0" } else { "1" }.into());
    let text: String = items.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    std::fs::write(path, text)
}

/// The command that starts the engine directly (no Qt launcher in between), plus the log file it writes.
pub fn command(paths: &AppPaths, s: &Settings, _manifest: &Manifest) -> Result<(Command, PathBuf), String> {
    let l = layout(paths)?;
    if !l.module_dir().join(if cfg!(windows) { "recompiled_image.dll" } else { "librecompiled_image.so" }).is_file() {
        return Err(crate::tr!("The game is not recompiled for the Native engine yet: run the preparation again").into());
    }
    write_overlay_ini(&l, s).map_err(|e| e.to_string())?;
    let mut std_cmd = std::process::Command::new(l.engine_exe());
    std_cmd
        .current_dir(l.root.join("engine"))
        .args(["--module-dir".as_ref(), l.module_dir().as_os_str()])
        .args(["--content-base".as_ref(), l.base().as_os_str()])
        .args(["--content-update".as_ref(), l.update().as_os_str()])
        .env("KARTRECOMP_NATIVE_GPU", "native")
        .env("KARTRECOMP_NATIVE_HEIGHT", s.display.output_height.to_string())
        .env("SUYU_CMD_PERF_SAMPLE", "1"); // "PERF game_fps=..." in the log every second: the launcher's fps readout
    for dlc in l.dlc_files() {
        std_cmd.args(["--content-dlc".as_ref(), dlc.as_os_str()]);
    }
    if s.display.fullscreen {
        std_cmd.arg("-f");
    }
    l.apply_env(&mut std_cmd);
    // Plugins built on this PC (DLSS, see components::native_plugins_dir) are found through XDG_DATA_HOME.
    let log = l.log_file();
    let _ = std::fs::remove_file(&log); // so the log follower starts from this run
    Ok((Command::from(std_cmd), log))
}

/// Follows the engine's log while it runs: "PERF game_fps=59.9 system_fps=... frametime_ms=16.7 speed=1.0000" becomes the
/// same `perf` telemetry the Compat engine prints, and the first one also means the game is running.
pub async fn follow_log(app: AppHandle, log: PathBuf, mut stop: tokio::sync::oneshot::Receiver<()>) {
    use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
    let _ = app.emit("engine://event", serde_json::json!({ "t": "hello", "engine": "deluxe8-native" }));
    let mut pos = 0u64;
    let mut running = false;
    let mut pending: Vec<u8> = Vec::new();
    loop {
        tokio::select! {
            _ = &mut stop => return,
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
        }
        let Ok(mut file) = tokio::fs::File::open(&log).await else { continue };
        let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        if len < pos {
            pos = 0; // the engine started a new log
            pending.clear();
        }
        if file.seek(std::io::SeekFrom::Start(pos)).await.is_err() {
            continue;
        }
        let mut reader = tokio::io::BufReader::new(file);
        let mut chunk = Vec::new();
        // bytes, not UTF-8 lines: one stray byte in the log must not stop the readout
        while let Ok(n) = reader.read_until(b'\n', &mut chunk).await {
            if n == 0 {
                break;
            }
            pos += n as u64;
            if !chunk.ends_with(b"\n") {
                pending.append(&mut chunk); // half-written line: wait for the rest
                break;
            }
            pending.append(&mut chunk);
            let full = String::from_utf8_lossy(&pending).into_owned();
            pending.clear();
            let Some(rest) = full.split("PERF ").nth(1) else { continue };
            let value = |key: &str| -> Option<f64> {
                rest.split_whitespace().find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
            };
            let (Some(fps), Some(ms), Some(speed)) = (value("game_fps"), value("frametime_ms"), value("speed")) else {
                continue;
            };
            if !running {
                running = true;
                let _ = app.emit("engine://event", serde_json::json!({ "t": "running", "program_id": crate::paths::TITLE_ID }));
            }
            let _ = app.emit(
                "engine://event",
                serde_json::json!({ "t": "perf", "fps": fps, "frametime_ms": ms, "speed": speed * 100.0 }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_scale_snaps_to_an_offered_row() {
        assert_eq!(render_scale_row(100), 1);
        assert_eq!(render_scale_row(67), 2);
        assert_eq!(render_scale_row(75), 2); // the old "Prestazioni" preset: 67 %, not "off"
        assert_eq!(render_scale_row(33), 5);
        assert_eq!(render_scale_row(10), 5);
        assert_eq!(render_scale_row(180), 7);
        assert_eq!(render_scale_row(160), 6);
    }
}
