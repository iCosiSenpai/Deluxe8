//! Deluxe8 launcher backend (Tauri 2).

// `pub` only so examples/ can drive the backend without the UI (see examples/components_check.rs).
pub mod components;
mod eden;
mod engine;
mod game;
pub mod i18n;
mod i18n_table;
pub mod logs;
mod mods;
pub mod native;
pub mod nextendo;
mod online;
pub mod paths;
pub mod prepare;
pub mod settings;
pub mod system;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, State};

use paths::AppPaths;
use settings::{EngineKind, Settings};

struct AppState {
    paths: AppPaths,
    settings: Mutex<Settings>,
    engine: Mutex<Option<engine::Launched>>,
    /// Set while `launch` starts an engine, so a second click cannot start another one.
    launching: AtomicBool,
    room: tokio::sync::Mutex<Option<online::HostedRoom>>,
    preparing: AtomicBool,
    prepare_cancel: Arc<AtomicBool>,
}

type CmdResult<T> = Result<T, String>;

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    settings.save(&state.paths.settings_file).map_err(|e| e.to_string())?;
    i18n::set_italian(i18n::wants_italian(&settings.ui.language, &i18n::system_locale()));
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
async fn system_info(state: State<'_, AppState>) -> CmdResult<system::SystemInfo> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || system::collect(&paths)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn find_candidates() -> CmdResult<game::Candidates> {
    tauri::async_runtime::spawn_blocking(game::find_candidates).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_game_folder(path: String) -> CmdResult<game::GameScan> {
    tauri::async_runtime::spawn_blocking(move || game::scan_folder(&PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn inspect_keys(path: String) -> CmdResult<game::KeysView> {
    tauri::async_runtime::spawn_blocking(move || game::inspect_keys(&PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn inspect_firmware(path: String) -> CmdResult<game::FirmwareView> {
    tauri::async_runtime::spawn_blocking(move || game::inspect_firmware(&PathBuf::from(path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn prepare(app: AppHandle, state: State<'_, AppState>, engine: EngineKind) -> CmdResult<()> {
    if state.preparing.swap(true, Ordering::SeqCst) {
        return Err(crate::tr!("A preparation is already running").into());
    }
    let mut settings = state.settings.lock().unwrap().clone();
    settings.engine = engine;
    let paths = state.paths.clone();
    let cancel = state.prepare_cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let result = prepare::run(app.clone(), paths, settings, cancel);
        let payload = match &result {
            Ok(()) => serde_json::json!({ "ok": true }),
            Err(e) => serde_json::json!({ "ok": false, "message": e }),
        };
        if let Some(state) = app.try_state::<AppState>() {
            state.preparing.store(false, Ordering::SeqCst);
        }
        let _ = app.emit("prepare://done", payload);
    });
    Ok(())
}

#[tauri::command]
fn cancel_prepare(state: State<'_, AppState>) {
    state.prepare_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
async fn components_status(state: State<'_, AppState>) -> CmdResult<components::ComponentsStatus> {
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || components::status(&paths)).await.map_err(|e| e.to_string())
}

/// Downloads (and for DLSS builds) one component: "native" or "dlss". Progress arrives as `prepare://step` events, the end
/// as `component://done`.
#[tauri::command]
fn install_component(app: AppHandle, state: State<'_, AppState>, name: String) -> CmdResult<()> {
    if state.preparing.swap(true, Ordering::SeqCst) {
        return Err(crate::tr!("Another preparation is running").into());
    }
    let paths = state.paths.clone();
    let cancel = state.prepare_cancel.clone();
    cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let result = prepare::install_component(app.clone(), paths, &name, cancel);
        let payload = match &result {
            Ok(()) => serde_json::json!({ "ok": true, "name": name }),
            Err(e) => serde_json::json!({ "ok": false, "name": name, "message": e }),
        };
        if let Some(state) = app.try_state::<AppState>() {
            state.preparing.store(false, Ordering::SeqCst);
        }
        let _ = app.emit("component://done", payload);
    });
    Ok(())
}

#[tauri::command]
async fn launch(
    app: AppHandle,
    state: State<'_, AppState>,
    engine: EngineKind,
    room: Option<engine::RoomTarget>,
) -> CmdResult<()> {
    if state.preparing.load(Ordering::SeqCst) {
        return Err(crate::tr!("Wait for the preparation to finish").into());
    }
    if state.launching.swap(true, Ordering::SeqCst) {
        return Err(crate::tr!("The game is already starting").into());
    }
    let result = async {
        if state.engine.lock().unwrap().as_ref().is_some_and(|l| !l.kill.is_closed()) {
            return Err(crate::tr!("The game is already running").to_string());
        }
        let mut settings = state.settings.lock().unwrap().clone();
        settings.engine = engine;
        let launched = engine::launch(app, state.paths.clone(), settings, room).await?;
        *state.engine.lock().unwrap() = Some(launched);
        Ok(())
    }
    .await;
    state.launching.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
fn stop_engine(state: State<'_, AppState>) {
    if let Some(launched) = state.engine.lock().unwrap().take() {
        let _ = launched.kill.send(());
    }
}

#[tauri::command]
async fn list_rooms(state: State<'_, AppState>) -> CmdResult<Vec<online::RoomInfo>> {
    let url = state.settings.lock().unwrap().online.lobby_url.clone();
    online::list_rooms(&url).await
}

#[tauri::command]
async fn host_room(state: State<'_, AppState>) -> CmdResult<online::HostResult> {
    let mut room = state.room.lock().await;
    if let Some(existing) = room.take() {
        online::stop_room(existing).await;
    }
    let settings = state.settings.lock().unwrap().clone();
    let version = prepare::Manifest::load(&state.paths)
        .map(|m| if m.update_version == 23 << 16 { "4.0.1".to_string() } else { format!("v{}", m.update_version) })
        .unwrap_or_else(|| "4.0.1".into());
    let (hosted, result) = online::host_room(&state.paths, &settings, &version).await?;
    *room = Some(hosted);
    logs::write(&format!("room hosted on {}:{} ({})", result.address, result.port, result.upnp));
    Ok(result)
}

#[tauri::command]
async fn stop_room(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(room) = state.room.lock().await.take() {
        online::stop_room(room).await;
    }
    Ok(())
}

#[tauri::command]
async fn room_status(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    let room = state.room.lock().await;
    Ok(match room.as_ref() {
        Some(r) => serde_json::json!({ "hosting": true, "address": r.address, "port": r.port }),
        None => serde_json::json!({ "hosting": false, "address": null, "port": null }),
    })
}

#[tauri::command]
fn list_mods(state: State<'_, AppState>) -> Vec<mods::ModInfo> {
    mods::list(&state.paths)
}

#[tauri::command]
fn set_mod_enabled(state: State<'_, AppState>, name: String, enabled: bool) -> CmdResult<()> {
    mods::set_enabled(&state.paths, &name, enabled)
}

#[tauri::command]
fn mods_folder(state: State<'_, AppState>) -> String {
    let dir = state.paths.mods_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

/// Opens Ryujinx Nextendo without a game so the player can link a Nextendo account in its own window (Deluxe8 never
/// handles the password). `online://closed` follows when the window is closed.
#[tauri::command]
fn open_online_client(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let dir = state
        .paths
        .online_engine_dir()
        .ok_or(tr!("Ryujinx Nextendo is not on this PC yet: set it up from the Online page"))?;
    if let Some(manifest) = prepare::Manifest::load(&state.paths) {
        // keys and firmware in place, so the account window can also reach the game list
        nextendo::prepare(&state.paths, &manifest)?;
    } else {
        std::fs::create_dir_all(nextendo::root(&state.paths)).map_err(|e| e.to_string())?;
    }
    let settings = state.settings.lock().unwrap().clone();
    let mut child = nextendo::command(&dir, &state.paths, &settings, None)
        .spawn()
        .map_err(|e| tr!("Ryujinx Nextendo did not start: {e}", e = e))?;
    logs::write("Ryujinx Nextendo opened for the Nextendo Network login");
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = app.emit("online://closed", ());
    });
    Ok(())
}

/// Opens one of Deluxe8's own folders in the file manager. The webview has no permission to open arbitrary paths: it can only
/// name one of these.
#[tauri::command]
fn open_folder(app: AppHandle, state: State<'_, AppState>, which: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = match which.as_str() {
        "mods" => state.paths.mods_dir(),
        "logs" => state.paths.logs.clone(),
        "data" => state.paths.data.clone(),
        other => return Err(format!("cartella sconosciuta: {other}")),
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_log(state: State<'_, AppState>, which: String) -> String {
    let path = match which.as_str() {
        "engine" => state.paths.logs.join("engine.log"),
        _ => logs::path().unwrap_or_else(|| state.paths.logs.join("launcher.log")),
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    // Last ~200 lines are plenty for the UI.
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(200)..].join("\n")
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let resource_dir = app.path().resource_dir().ok();
            let paths = AppPaths::resolve(resource_dir);
            paths.ensure()?;
            logs::init(paths.logs.join("launcher.log"));
            logs::write(&format!(
                "data={} compat={:?} native={:?} dlss-src={:?} font={:?}",
                paths.data.display(),
                paths.compat_engine_dir,
                paths.native_engine_dir(),
                paths.dlss_plugin_src,
                paths.font
            ));
            let settings = Settings::load(&paths.settings_file);
            i18n::set_italian(i18n::wants_italian(&settings.ui.language, &i18n::system_locale()));
            app.manage(AppState {
                paths,
                settings: Mutex::new(settings),
                engine: Mutex::new(None),
                launching: AtomicBool::new(false),
                room: tokio::sync::Mutex::new(None),
                preparing: AtomicBool::new(false),
                prepare_cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            system_info,
            find_candidates,
            scan_game_folder,
            inspect_keys,
            inspect_firmware,
            prepare,
            cancel_prepare,
            components_status,
            install_component,
            launch,
            stop_engine,
            list_rooms,
            host_room,
            stop_room,
            room_status,
            list_mods,
            set_mod_enabled,
            mods_folder,
            open_folder,
            open_online_client,
            read_log,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Deluxe8")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // A hosted room lives as long as the launcher: stop its server and close the router port it opened.
                // (A running game is left alone: the player may close the launcher on purpose.)
                if let Some(state) = app.try_state::<AppState>()
                    && let Ok(mut room) = state.room.try_lock()
                    && let Some(hosted) = room.take()
                {
                    tauri::async_runtime::block_on(online::stop_room(hosted));
                    logs::write("room stopped on exit");
                }
            }
        });
}
