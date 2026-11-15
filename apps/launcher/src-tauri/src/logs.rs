//! Tiny append-only launcher log (`<data>/logs/launcher.log`).

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

pub fn init(path: PathBuf) {
    // Keep the previous session for comparison.
    if path.exists() {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    *LOG.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(path);
    write("Deluxe8 launcher avviato");
}

pub fn write(line: &str) {
    let Some(lock) = LOG.get() else { return };
    let Ok(guard) = lock.lock() else { return };
    let Some(path) = guard.as_ref() else { return };
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] {line}");
    }
}

pub fn path() -> Option<PathBuf> {
    LOG.get()?.lock().ok()?.clone()
}
