// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

namespace InputCommon {
class InputSubsystem;
}
namespace Core::HID {
class HIDCore;
}

namespace Deluxe8::Input {

/// Binds physical SDL controllers to the emulated players in connection order (first
/// controller = player 1, ...), using the same per-device default layouts as Eden's Qt
/// "auto-map". Players without a controller keep their current binding (player 1 stays on
/// the keyboard). Safe to call repeatedly (late enumeration, hot-plug): returns true only
/// when a binding changed, in which case the HID devices have already been reloaded.
bool AutoMap(InputCommon::InputSubsystem& input, Core::HID::HIDCore& hid, int max_players);

} // namespace Deluxe8::Input
