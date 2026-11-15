// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Header-only on purpose: hid_core includes it without linking the deluxe8 library.

#pragma once

#include <atomic>

namespace Deluxe8 {

/// Set while the in-game overlay is open: the emulated controllers then report a neutral
/// state, so menu navigation never reaches the game.
inline std::atomic<bool> g_input_captured{false};

[[nodiscard]] inline bool IsInputCaptured() noexcept {
    return g_input_captured.load(std::memory_order_relaxed);
}

} // namespace Deluxe8
