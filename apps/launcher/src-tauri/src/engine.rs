//! Starting the engines, relaying their telemetry ("D8> {json}" lines) and logs to the UI.

use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;

use crate::paths::AppPaths;
use crate::prepare::Manifest;
use crate::settings::{EngineKind, Settings};
use crate::{eden, logs, native};

#[derive(Debug, Clone, Deserialize)]
pub struct RoomTarget {
    pub address: String,
    pub port: u16,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExitEvent {
    pub code: Option<i32>,
    pub message: Option<String>,
}

pub struct Launched {
    pub kill: oneshot::Sender<()>,
}

fn version_name(v: u32) -> String {
    match v {
        x if x == 23 << 16 => "4.0.1".into(),
        x if x == 20 << 16 => "3.0.4".into(),
        x => format!("v{x}"),
    }
}

/// eden-cli's `-m nick[:password]@address:port`. Its parser needs at least one character after `:`, so a room without a
/// password must leave the `:` out (with `nick:@...` the option is dropped and the game starts outside the room).
fn multiplayer_arg(nickname: &str, room: &RoomTarget) -> String {
    if room.password.is_empty() {
        format!("{nickname}@{}:{}", room.address, room.port)
    } else {
        format!("{nickname}:{}@{}:{}", room.password, room.address, room.port)
    }
}

/// Validates the nickname the same way eden-cli does (it asserts on bad input).
pub fn valid_nickname(nick: &str) -> bool {
    (4..=20).contains(&nick.chars().count())
        && nick.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
}

fn sync_lossless(paths: &AppPaths, s: &Settings) -> Option<String> {
    if s.smoothness.frame_gen != "lsfg" {
        return None;
    }
    let Some(src) = crate::system::find_lossless_dll() else {
        return Some("Frame generation richiesta ma Lossless.dll non trovato: avvio senza".into());
    };
    let dst = paths.compat_user.join("lossless").join("Lossless.dll");
    let same = std::fs::metadata(&src).ok().map(|m| m.len()) == std::fs::metadata(&dst).ok().map(|m| m.len());
    if !same {
        let _ = std::fs::create_dir_all(dst.parent().unwrap());
        if let Err(e) = std::fs::copy(&src, &dst) {
            return Some(format!("Lossless.dll non copiato: {e}"));
        }
    }
    None
}

/// Drains one of the engine's output pipes until it closes. Bytes, not UTF-8 lines: the engines may print text that is not
/// UTF-8 (the game's own debug output), and a reader that stopped there would leave the pipe full and the engine blocked on
/// its next write.
async fn pump<R: AsyncRead + Unpin + Send + 'static>(app: AppHandle, reader: R, log_file: PathBuf) {
    use tokio::io::AsyncWriteExt;
    let mut reader = BufReader::new(reader);
    let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(&log_file).await.ok();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&buf);
        let line = text.trim_end_matches(['\r', '\n']);
        if let Some(json) = line.strip_prefix("D8> ")
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(json)
        {
            let _ = app.emit("engine://event", value);
            continue;
        }
        if let Some(f) = file.as_mut() {
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.write_all(b"\n").await;
        }
        let _ = app.emit("engine://log", line.to_string());
    }
}

/// Asks the engine to quit the way closing its window does, so it shuts the game down and flushes its files: WM_CLOSE to its
/// windows on Windows, SIGTERM elsewhere.
fn request_close(pid: u32) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
        unsafe extern "system" fn close_if_owned(hwnd: HWND, pid: LPARAM) -> windows::core::BOOL {
            let mut owner = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
            if owner as isize == pid.0 {
                let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
            true.into()
        }
        let _ = unsafe { EnumWindows(Some(close_if_owned), LPARAM(pid as isize)) };
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    }
}

pub async fn launch(
    app: AppHandle,
    paths: AppPaths,
    s: Settings,
    room: Option<RoomTarget>,
) -> Result<Launched, String> {
    let manifest = Manifest::load(&paths).ok_or("Il gioco non è ancora preparato: rifai la preparazione")?;
    let mut native_log: Option<PathBuf> = None;
    let mut cmd = match s.engine {
        EngineKind::Compat => {
            let exe = paths.compat_engine().ok_or("Motore Compat non trovato")?;
            eden::write_config(&paths.compat_user, &s).map_err(|e| format!("Configurazione non scritta: {e}"))?;
            crate::mods::sync_split60(&paths, s.controls.split60, manifest.update_version)
                .map_err(|e| format!("Patch schermo diviso non applicata: {e}"))?;
            if let Some(warning) = sync_lossless(&paths, &s) {
                logs::write(&warning);
                let _ = app.emit("engine://log", warning);
            }
            let mut cmd = Command::new(&exe);
            cmd.current_dir(exe.parent().unwrap())
                .env("DELUXE8_USER_DIR", &paths.compat_user)
                .env("DELUXE8_CONTENT_DIRS", &paths.content_updates)
                .env("DELUXE8_TELEMETRY", "1")
                .env("DELUXE8_GAME_VERSION", version_name(manifest.update_version))
                .env("DELUXE8_PLAYERS", s.controls.players.clamp(1, 8).to_string());
            if let Some(font) = &paths.font {
                cmd.env("DELUXE8_OVERLAY_FONT", font);
            }
            if s.display.fullscreen {
                cmd.arg("-f");
            }
            if let Some(room) = &room {
                if !valid_nickname(&s.online.nickname) {
                    return Err("Nome da pilota non valido (4-20 caratteri: lettere, numeri, spazio, . _ -)".into());
                }
                cmd.arg("-m").arg(multiplayer_arg(&s.online.nickname, room));
            }
            cmd.arg(&manifest.base);
            cmd
        }
        EngineKind::Native => {
            if room.is_some() {
                return Err("Le stanze online sono disponibili solo con il motore Compat".into());
            }
            let (cmd, log) = native::command(&paths, &s, &manifest)?;
            native_log = Some(log);
            cmd
        }
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let mut shown = format!("{:?}", cmd.as_std());
    if let Some(room) = room.as_ref().filter(|r| !r.password.is_empty()) {
        // the room password stays out of the log
        let hidden = RoomTarget { password: "***".into(), ..room.clone() };
        shown = shown.replace(&multiplayer_arg(&s.online.nickname, room), &multiplayer_arg(&s.online.nickname, &hidden));
    }
    logs::write(&format!("launch {:?}: {shown}", s.engine));
    let mut child = cmd.spawn().map_err(|e| format!("Avvio del motore fallito: {e}"))?;
    let log_file = paths.logs.join("engine.log");
    let _ = std::fs::write(&log_file, "");
    if let Some(out) = child.stdout.take() {
        tauri::async_runtime::spawn(pump(app.clone(), out, log_file.clone()));
    }
    if let Some(err) = child.stderr.take() {
        tauri::async_runtime::spawn(pump(app.clone(), err, log_file.clone()));
    }
    // The Native engine writes its log (and fps samples) to a file, not to stdout.
    let follow_stop = native_log.clone().map(|log| {
        let (tx, rx) = oneshot::channel::<()>();
        tauri::async_runtime::spawn(native::follow_log(app.clone(), log, rx));
        tx
    });
    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    let started = std::time::Instant::now();
    tauri::async_runtime::spawn(async move {
        let mut stopped = false;
        let status = tokio::select! {
            status = child.wait() => status.ok(),
            _ = kill_rx => {
                // "Ferma": like closing the game window, then the hard way if the engine does not quit in time
                stopped = true;
                if let Some(pid) = child.id() {
                    request_close(pid);
                }
                match tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await {
                    Ok(status) => status.ok(),
                    Err(_) => {
                        logs::write("engine did not close in 10 s: killed");
                        let _ = child.kill().await;
                        child.wait().await.ok()
                    }
                }
            }
        };
        if let Some(stop) = follow_stop {
            let _ = stop.send(());
        }
        if let Some(log) = &native_log {
            // keep the end of the engine's own log where the System page reads the engine's messages
            if let Ok(text) = tokio::fs::read_to_string(log).await {
                let lines: Vec<&str> = text.lines().collect();
                let tail = lines[lines.len().saturating_sub(300)..].join("\n");
                use tokio::io::AsyncWriteExt;
                if let Ok(mut f) = tokio::fs::OpenOptions::new().create(true).append(true).open(&log_file).await {
                    let _ = f.write_all(tail.as_bytes()).await;
                    let _ = f.write_all(b"\n").await;
                }
            }
        }
        let code = status.and_then(|s| s.code());
        let message = match code {
            _ if stopped => None, // the player asked for it: not a crash, whatever the exit code
            Some(0) | None => None,
            Some(6) => Some("Il motore si è fermato: chiavi più vecchie del firmware (codice 6)".to_string()),
            Some(c) if started.elapsed().as_secs() < 8 => Some(format!(
                "Il motore si è chiuso subito (codice {c}). Controlla Sistema > Ultimi messaggi del motore."
            )),
            Some(c) => Some(format!("Il motore si è chiuso con codice {c}")),
        };
        logs::write(&format!("engine exited: {code:?}{}", if stopped { " (stopped)" } else { "" }));
        let _ = app.emit("engine://exit", ExitEvent { code: if stopped { None } else { code }, message });
    });
    Ok(Launched { kill: kill_tx })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_argument_matches_eden_cli() {
        let open = RoomTarget { address: "203.0.113.10".into(), port: 24872, password: String::new() };
        assert_eq!(multiplayer_arg("Lupo 8", &open), "Lupo 8@203.0.113.10:24872");
        let locked = RoomTarget { password: "a:b@c".into(), ..open };
        assert_eq!(multiplayer_arg("Lupo 8", &locked), "Lupo 8:a:b@c@203.0.113.10:24872");
        assert!(valid_nickname("Lupo 8") && !valid_nickname("abc") && !valid_nickname("Lupo!"));
    }
}

#[cfg(all(test, windows))]
mod close_tests {
    use super::request_close;
    use std::process::Command;
    use std::time::Duration;

    fn alive(pid: u32) -> bool {
        let out = Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    }

    /// `cargo test -- --ignored`: starts Character Map hidden (no window shows up) and closes it like "Ferma" does.
    #[test]
    #[ignore = "starts a hidden Windows program"]
    fn request_close_ends_a_windowed_program() {
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", "(Start-Process charmap.exe -WindowStyle Hidden -PassThru).Id"])
            .output()
            .unwrap();
        let pid: u32 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        assert!(alive(pid));
        request_close(pid);
        for _ in 0..50 {
            if !alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).status();
        panic!("WM_CLOSE did not end the program");
    }
}
