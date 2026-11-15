//! Writes the Compat engine configuration (`<user>/config/sdl2-config.ini`).
//!
//! The file is edited in place: only the keys Deluxe8 manages are set (with `key\default=false`
//! so Eden honours them); everything else, such as controller bindings created by the engine,
//! is preserved.

use std::path::Path;

use crate::settings::Settings;

struct Ini {
    lines: Vec<String>,
}

impl Ini {
    fn load(path: &Path) -> Ini {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Ini { lines: text.lines().map(str::to_string).collect() }
    }

    fn section_range(&self, section: &str) -> Option<(usize, usize)> {
        let header = format!("[{section}]");
        let start = self.lines.iter().position(|l| l.trim() == header)?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| l.starts_with('['))
            .map(|i| start + 1 + i)
            .unwrap_or(self.lines.len());
        Some((start, end))
    }

    fn set(&mut self, section: &str, key: &str, value: impl ToString) {
        let value = value.to_string();
        let (start, mut end) = match self.section_range(section) {
            Some(range) => range,
            None => {
                if self.lines.last().is_some_and(|l| !l.is_empty()) {
                    self.lines.push(String::new());
                }
                self.lines.push(format!("[{section}]"));
                let start = self.lines.len() - 1;
                (start, self.lines.len())
            }
        };
        let default_key = format!("{key}\\default=");
        let value_key = format!("{key}=");
        let mut found_default = false;
        let mut found_value = false;
        for line in &mut self.lines[start + 1..end] {
            if line.starts_with(&default_key) {
                *line = format!("{key}\\default=false");
                found_default = true;
            } else if line.starts_with(&value_key) {
                *line = format!("{key}={value}");
                found_value = true;
            }
        }
        // Keep new keys before the blank line that separates sections.
        while end > start + 1 && self.lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        let mut insert = Vec::new();
        if !found_default {
            insert.push(format!("{key}\\default=false"));
        }
        if !found_value {
            insert.push(format!("{key}={value}"));
        }
        for (i, line) in insert.into_iter().enumerate() {
            self.lines.insert(end + i, line);
        }
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = self.lines.join("\n");
        text.push('\n');
        std::fs::write(path, text)
    }
}

fn scaling_filter(upscaler: &str) -> u32 {
    match upscaler {
        "nearest" => 0,
        "bilinear" => 1,
        "bicubic" => 2,
        "lanczos" => 4,
        "scaleforce" => 5,
        "area" => 7,
        "mmpx" => 12,
        "sgsr" => 13,
        "sgsr_edge" => 14,
        _ => 6, // FSR 1
    }
}

fn language_index(lang: &str) -> u32 {
    match lang {
        "ja" => 0,
        "en" => 12, // English (British): European release text
        "fr" => 2,
        "de" => 3,
        "es" => 5,
        _ => 4, // Italian
    }
}

pub fn write_config(user_dir: &Path, s: &Settings) -> std::io::Result<()> {
    let path = user_dir.join("config").join("sdl2-config.ini");
    let mut ini = Ini::load(&path);

    // Renderer
    ini.set("Renderer", "backend", 1); // Vulkan
    ini.set("Renderer", "resolution_setup", s.image.compat_scale.min(12));
    ini.set("Renderer", "scaling_filter", scaling_filter(&s.image.upscaler));
    ini.set("Renderer", "fsr_sharpening_slider", (100 - s.image.sharpness.min(100)) * 2);
    ini.set(
        "Renderer",
        "anti_aliasing",
        match s.image.anti_aliasing.as_str() {
            "fxaa" => 1,
            "smaa" => 2,
            _ => 0,
        },
    );
    ini.set(
        "Renderer",
        "max_anisotropy",
        match s.image.anisotropy.as_str() {
            "2x" => 2,
            "4x" => 3,
            "8x" => 4,
            "16x" => 5,
            _ => 0,
        },
    );
    ini.set(
        "Renderer",
        "use_vsync",
        match s.display.vsync.as_str() {
            "immediate" => 0,
            "mailbox" => 1,
            "fifo_relaxed" => 3,
            _ => 2,
        },
    );
    ini.set(
        "Renderer",
        "aspect_ratio",
        match s.display.aspect.as_str() {
            "4:3" => 1,
            "21:9" => 2,
            "16:10" => 3,
            "stretch" => 4,
            _ => 0,
        },
    );
    ini.set("Renderer", "fullscreen_mode", 0); // borderless
    ini.set("Renderer", "gpu_accuracy", if s.advanced.gpu_accuracy == "normal" { 0 } else { 1 });
    ini.set("Renderer", "use_asynchronous_shaders", s.advanced.async_shaders);
    ini.set("Renderer", "use_disk_shader_cache", s.advanced.disk_shader_cache);
    ini.set(
        "Renderer",
        "accelerate_astc",
        match s.advanced.astc_decode.as_str() {
            "cpu" => 0,
            "async" => 2,
            _ => 1,
        },
    );
    ini.set("Renderer", "vram_usage_mode", if s.advanced.vram_mode == "conservative" { 0 } else { 1 });
    let lsfg = s.smoothness.frame_gen == "lsfg";
    ini.set("Renderer", "frame_gen", lsfg);
    ini.set("Renderer", "frame_gen_multiplier", s.smoothness.multiplier.clamp(2, 4));
    ini.set("Renderer", "frame_gen_flow_scale_auto", s.smoothness.flow_scale == 0);
    ini.set("Renderer", "frame_gen_flow_scale", s.smoothness.flow_scale.clamp(25, 100));

    // Core / CPU
    ini.set("Core", "use_speed_limit", s.smoothness.speed_limit);
    ini.set(
        "Cpu",
        "cpu_accuracy",
        match s.advanced.cpu_accuracy.as_str() {
            "accurate" => 1,
            "unsafe" => 2,
            _ => 0,
        },
    );

    // Audio
    ini.set("Audio", "volume", s.audio.volume.min(100));
    ini.set(
        "Audio",
        "output_engine",
        match s.audio.output.as_str() {
            "cubeb" => 1,
            "sdl" => 2,
            _ => 0,
        },
    );

    // System: docked (TV) mode is the 1080p target the game is tuned for.
    ini.set("System", "use_docked_mode", 1);
    ini.set("System", "language_index", language_index(&s.system.language));
    ini.set(
        "System",
        "region_index",
        match s.system.region.as_str() {
            "jpn" => 0,
            "usa" => 1,
            _ => 2,
        },
    );

    // Controls: players get bound to controllers by the engine (Deluxe8 auto-map).
    let players = s.controls.players.clamp(1, 8);
    for p in 0..8 {
        ini.set("Controls", &format!("player_{p}_connected"), p < players);
        ini.set("Controls", &format!("player_{p}_vibration_enabled"), s.controls.rumble);
    }
    ini.set("Controls", "motion_enabled", s.controls.motion);

    // Public room lobby used when hosting.
    let lobby = s.online.lobby_url.trim_start_matches("https://").trim_start_matches("http://");
    if !lobby.is_empty() {
        ini.set("WebService", "web_api_url", lobby);
    }

    ini.save(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_and_inserts() {
        let mut ini = Ini {
            lines: ["[Renderer]", "scaling_filter\\default=true", "scaling_filter=0", "", "[Audio]", "volume=100"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        ini.set("Renderer", "scaling_filter", 6);
        ini.set("Renderer", "frame_gen", true);
        ini.set("System", "region_index", 2);
        let text = ini.lines.join("\n");
        assert!(text.contains("scaling_filter\\default=false\nscaling_filter=6"));
        assert!(text.contains("frame_gen\\default=false\nframe_gen=true\n\n[Audio]"));
        assert!(text.ends_with("[System]\nregion_index\\default=false\nregion_index=2"));
    }
}
