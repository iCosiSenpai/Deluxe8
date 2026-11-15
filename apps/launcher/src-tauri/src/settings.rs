//! Launcher settings (mirrors `src/lib/types.ts`). Every field has a default so older or
//! partial files load without errors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    pub setup_done: bool,
    pub engine: EngineKind,
    pub preset: String,
    pub paths: Paths,
    pub display: Display,
    pub image: Image,
    pub smoothness: Smoothness,
    pub advanced: Advanced,
    pub audio: Audio,
    pub controls: Controls,
    pub online: Online,
    pub system: SystemPrefs,
    pub native: NativePrefs,
    pub ui: UiPrefs,
}

/// The launcher's own preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiPrefs {
    /// Interface language: "auto" (Italian in Italy or on an Italian desktop, English otherwise), "en" or "it".
    pub language: String,
}
impl Default for UiPrefs {
    fn default() -> Self {
        Self { language: "auto".into() }
    }
}

/// Components of the Native engine the preparation downloads.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NativePrefs {
    /// Download NVIDIA's DLSS SDK from GitHub and build the DLSS plugin for this PC (NVIDIA RTX only).
    pub dlss: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Native,
    /// The Emulator engine (Eden-based).
    #[default]
    Compat,
    /// Ryujinx Nextendo on the Nextendo Network.
    Online,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Paths {
    pub game_folder: String,
    pub base: String,
    pub update: String,
    pub dlc: Vec<String>,
    pub prod_keys: String,
    pub title_keys: String,
    pub firmware: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Display {
    pub fullscreen: bool,
    pub vsync: String,
    pub aspect: String,
    pub output_height: u32,
}
impl Default for Display {
    fn default() -> Self {
        Self { fullscreen: true, vsync: "fifo".into(), aspect: "16:9".into(), output_height: 1440 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Image {
    pub compat_scale: u32,
    pub render_scale: u32,
    pub upscaler: String,
    pub sharpness: u32,
    pub anti_aliasing: String,
    pub anisotropy: String,
    pub hdr: bool,
    pub hdr_paper_white: u32,
    pub hdr_peak: u32,
}
impl Default for Image {
    fn default() -> Self {
        Self {
            compat_scale: 5, // 1.5x: the "Bilanciato" preset the defaults are named after
            render_scale: 100,
            upscaler: "fsr1".into(),
            sharpness: 60,
            anti_aliasing: "smaa".into(),
            anisotropy: "16x".into(),
            hdr: false,
            hdr_paper_white: 203,
            hdr_peak: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Smoothness {
    pub frame_gen: String,
    pub multiplier: u32,
    pub flow_scale: u32,
    pub hud: String,
    pub speed_limit: bool,
}
impl Default for Smoothness {
    fn default() -> Self {
        Self { frame_gen: "off".into(), multiplier: 2, flow_scale: 0, hud: "compact".into(), speed_limit: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Advanced {
    pub gpu_accuracy: String,
    pub async_shaders: bool,
    pub disk_shader_cache: bool,
    pub astc_decode: String,
    pub vram_mode: String,
    pub cpu_accuracy: String,
}
impl Default for Advanced {
    fn default() -> Self {
        Self {
            gpu_accuracy: "high".into(),
            async_shaders: false,
            disk_shader_cache: true,
            astc_decode: "gpu".into(),
            vram_mode: "aggressive".into(),
            cpu_accuracy: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Audio {
    pub volume: u32,
    pub mute_in_background: bool,
    pub output: String,
}
impl Default for Audio {
    fn default() -> Self {
        Self { volume: 80, mute_in_background: true, output: "auto".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Controls {
    pub rumble: bool,
    pub motion: bool,
    pub players: u32,
    /// 3-4 player split screen at 60 fps instead of the game's 30 (update 4.0.1 only).
    pub split60: bool,
}
impl Default for Controls {
    fn default() -> Self {
        Self { rumble: true, motion: true, players: 1, split60: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Online {
    pub nickname: String,
    pub lobby_url: String,
    pub host_port: u16,
    pub max_players: u32,
    pub password: String,
    pub room_name: String,
    pub upnp: bool,
}
impl Default for Online {
    fn default() -> Self {
        Self {
            nickname: String::new(),
            lobby_url: "https://api.ynet-fun.xyz".into(),
            host_port: 24872,
            max_players: 8,
            password: String::new(),
            room_name: "Deluxe8 room".into(),
            upnp: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemPrefs {
    pub language: String,
    pub region: String,
}
impl Default for SystemPrefs {
    fn default() -> Self {
        Self { language: "auto".into(), region: "eur".into() }
    }
}

impl Settings {
    /// The game's language: the setting, or the interface language when it is "auto".
    pub fn game_language(&self) -> &str {
        match self.system.language.as_str() {
            "auto" | "" => crate::i18n::code(),
            other => other,
        }
    }

    pub fn load(path: &std::path::Path) -> Settings {
        let mut s: Settings = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                // keep the unreadable file for the player (or a bug report) instead of overwriting it on the next save
                let bad = path.with_extension("json.bad");
                let _ = std::fs::rename(path, &bad);
                crate::logs::write(&format!("settings unreadable ({e}): moved to {}, defaults used", bad.display()));
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        if s.version == 0 {
            s.version = 1;
        }
        if s.preset.is_empty() {
            s.preset = "bilanciato".into();
        }
        s
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}
