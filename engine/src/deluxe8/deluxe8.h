// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Deluxe8 engine integration: launcher handshake (environment), telemetry and the glue
// the overlay needs. This library must not depend on `core` (core -> video_core -> deluxe8).

#pragma once

#include <cstdint>
#include <functional>
#include <string>
#include <string_view>

namespace Deluxe8 {

/// Applies DELUXE8_* environment variables to the global settings. Call once, right after the
/// frontend configuration has been loaded and before the system is initialized:
///   DELUXE8_CONTENT_DIRS  folders holding update/DLC packages (';' on Windows, ':' elsewhere)
///   DELUXE8_TELEMETRY=1   machine-readable "D8> {json}" lines on stdout for the launcher
///   DELUXE8_OVERLAY_FONT  TTF used by the in-game overlay
///   DELUXE8_GAME_VERSION  display version shown in the overlay (e.g. "4.0.1")
void Initialize();

[[nodiscard]] bool TelemetryEnabled() noexcept;

/// Writes "D8> <json>\n" to stdout and flushes (thread-safe). `json` must be one JSON object.
void Emit(std::string_view json);

/// Escapes a string for use inside a JSON string literal (no surrounding quotes).
[[nodiscard]] std::string JsonEscape(std::string_view text);

/// Convenience: {"t":"<type>","msg":"<text>"}.
void EmitMessage(std::string_view type, std::string_view text);

struct PerfSample {
    double guest_fps = 0.0;      ///< frames the game produced per second
    double frametime_ms = 0.0;   ///< average guest frame time
    double speed_percent = 0.0;  ///< emulation speed relative to the console
};

/// Called by the frontend about twice per second with the core's performance counters.
void PublishPerf(const PerfSample& sample);
[[nodiscard]] PerfSample LatestPerf();

/// Hooks implemented by the frontend so the overlay can act on the emulated system without
/// this library depending on it.
struct Actions {
    std::function<void()> request_exit;
    std::function<void(bool paused)> set_paused;
};
void SetActions(Actions actions);
[[nodiscard]] const Actions& GetActions();

[[nodiscard]] std::string GameVersion();
/// Players to bind to controllers (DELUXE8_PLAYERS, 1..8, default 1).
[[nodiscard]] int Players() noexcept;
[[nodiscard]] std::string OverlayFontPath();

} // namespace Deluxe8
