//! Online rooms (local-wireless over the internet, Eden/yuzu room protocol).
//! Browsing the public lobby, hosting a room with `deluxe8-room`, opening the port via UPnP.

use std::net::{IpAddr, SocketAddr};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::{Child, Command};

use crate::paths::AppPaths;
use crate::settings::Settings;

pub const GAME_NAME: &str = "Mario Kart 8 Deluxe";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub address: String,
    pub port: u16,
    pub players: u32,
    pub max_players: u32,
    pub has_password: bool,
    pub game: String,
    pub game_version: Option<String>,
}

/// Deluxe8 rooms carry the game version in the advertised game name ("... 4.0.1").
fn version_in(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '[' || c == ']' || c == '(' || c == ')')
        .find(|t| {
            let parts: Vec<&str> = t.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_string)
}

pub async fn list_rooms(lobby_url: &str) -> Result<Vec<RoomInfo>, String> {
    let base = if lobby_url.starts_with("http") { lobby_url.to_string() } else { format!("https://{lobby_url}") };
    let url = format!("{}/lobby", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Deluxe8")
        .build()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| crate::tr!("The lobby cannot be reached: {e}", e = e))?
        .json()
        .await
        .map_err(|e| crate::tr!("Invalid answer from the lobby: {e}", e = e))?;
    let rooms = json.get("rooms").and_then(|r| r.as_array()).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for r in rooms {
        let s = |k: &str| r.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let game = s("preferredGameName");
        let players: Vec<serde_json::Value> = r.get("players").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        let playing_mk8d = players.iter().any(|p| {
            p.get("gameName").and_then(|g| g.as_str()).is_some_and(|g| g.contains("Mario Kart 8"))
        });
        if !game.contains("Mario Kart 8") && !playing_mk8d {
            continue;
        }
        let name = s("name");
        out.push(RoomInfo {
            id: s("id"),
            description: s("description"),
            owner: s("owner"),
            address: s("address"),
            port: r.get("port").and_then(|p| p.as_u64()).unwrap_or(24872) as u16,
            players: players.len() as u32,
            max_players: r.get("maxPlayers").and_then(|p| p.as_u64()).unwrap_or(8) as u32,
            has_password: r.get("hasPassword").and_then(|p| p.as_bool()).unwrap_or(false),
            game_version: version_in(&game).or_else(|| version_in(&name)).or_else(|| version_in(&s("description"))),
            game,
            name,
        });
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.players));
    Ok(out)
}

pub struct HostedRoom {
    pub child: Child,
    pub address: String,
    pub port: u16,
    pub upnp_gateway: Option<igd_next::aio::Gateway<igd_next::aio::tokio::Tokio>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostResult {
    pub address: String,
    pub port: u16,
    pub upnp: String,
}

pub async fn host_room(paths: &AppPaths, s: &Settings, version: &str) -> Result<(HostedRoom, HostResult), String> {
    let exe = paths.compat_room().ok_or(crate::tr!("Room server (deluxe8-room) not found"))?;
    let port = s.online.host_port;
    let mut cmd = Command::new(&exe);
    cmd.args(["--room-name", &s.online.room_name])
        .args(["--room-description", crate::tr!("Deluxe8 room")])
        .args(["--port", &port.to_string()])
        .args(["--max-members", &s.online.max_players.clamp(2, 16).to_string()])
        .args(["--preferred-game", &format!("{GAME_NAME} {version}")])
        .args(["--preferred-game-id", "0100152000022000"])
        .args(["--log-file", &paths.logs.join("room.log").to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if !s.online.password.is_empty() {
        cmd.args(["--password", &s.online.password]);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let child = cmd.spawn().map_err(|e| crate::tr!("The room server did not start: {e}", e = e))?;

    let local_ip = local_ip_address::local_ip().map_err(|e| e.to_string())?;
    let mut address = local_ip.to_string();
    let mut upnp = crate::tr!("UPnP is off: friends outside your home network need the port opened by hand").to_string();
    let mut gateway = None;
    if s.online.upnp {
        match igd_next::aio::tokio::search_gateway(Default::default()).await {
            Ok(gw) => {
                let local = SocketAddr::new(local_ip, port);
                match gw.add_port(igd_next::PortMappingProtocol::UDP, port, local, 0, "Deluxe8 room").await {
                    Ok(()) => {
                        if let Ok(IpAddr::V4(ext)) = gw.get_external_ip().await {
                            address = ext.to_string();
                        }
                        upnp = crate::tr!("UDP port {port} opened on the router (UPnP)", port = port);
                        gateway = Some(gw);
                    }
                    Err(e) => upnp = crate::tr!("The router refused to open the port: {e}", e = e),
                }
            }
            Err(_) => upnp = crate::tr!("No UPnP router found: open the port by hand or use a VPN").into(),
        }
    }
    Ok((
        HostedRoom { child, address: address.clone(), port, upnp_gateway: gateway },
        HostResult { address, port, upnp },
    ))
}

pub async fn stop_room(mut room: HostedRoom) {
    let _ = room.child.kill().await;
    if let Some(gw) = room.upnp_gateway.take() {
        let _ = gw.remove_port(igd_next::PortMappingProtocol::UDP, room.port).await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_versions() {
        assert_eq!(super::version_in("Mario Kart 8 Deluxe 4.0.1").as_deref(), Some("4.0.1"));
        assert_eq!(super::version_in("[3.0.4] notte").as_deref(), Some("3.0.4"));
        assert_eq!(super::version_in("Mario Kart 8 Deluxe"), None);
    }
}
