// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "deluxe8/automap.h"

#include <algorithm>
#include <format>
#include <string>
#include <vector>

#include "common/logging.h"
#include "common/param_package.h"
#include "common/settings.h"
#include "deluxe8/deluxe8.h"
#include "hid_core/hid_core.h"
#include "input_common/main.h"

namespace Deluxe8::Input {
namespace {

/// Identity of a device as stored in a binding string ("engine:sdl,port:0,guid:...").
std::string DeviceKey(const Common::ParamPackage& p) {
    return std::format("{}|{}|{}", p.Get("engine", ""), p.Get("guid", ""), p.Get("port", 0));
}

bool BoundTo(const Settings::PlayerInput& player, const std::string& key) {
    const Common::ParamPackage a{player.buttons[Settings::NativeButton::A]};
    return DeviceKey(a) == key;
}

} // namespace

bool AutoMap(InputCommon::InputSubsystem& input, Core::HID::HIDCore& hid, int max_players) {
    std::vector<Common::ParamPackage> pads;
    for (const auto& device : input.GetInputDevices()) {
        if (device.Get("engine", "") == "sdl") {
            pads.push_back(device);
        }
    }
    auto& players = Settings::values.players.GetValue();
    const int count = std::min({max_players, static_cast<int>(pads.size()), static_cast<int>(players.size())});
    bool changed = false;
    for (int i = 0; i < count; ++i) {
        const auto& device = pads[static_cast<size_t>(i)];
        auto& player = players[static_cast<size_t>(i)];
        const std::string key = DeviceKey(device);
        if (BoundTo(player, key)) {
            continue;
        }
        for (const auto& [button, param] : input.GetButtonMappingForDevice(device)) {
            player.buttons[button] = param.Serialize();
        }
        for (const auto& [analog, param] : input.GetAnalogMappingForDevice(device)) {
            player.analogs[analog] = param.Serialize();
        }
        for (const auto& [motion, param] : input.GetMotionMappingForDevice(device)) {
            player.motions[motion] = param.Serialize();
        }
        player.connected = true;
        player.controller_type = Settings::ControllerType::ProController;
        changed = true;
        const std::string name = device.Get("display", "Controller");
        LOG_INFO(Input, "Deluxe8: player {} -> {}", i + 1, name);
        Emit(std::format(R"({{"t":"controller","player":{},"name":"{}"}})", i + 1, JsonEscape(name)));
    }
    if (changed) {
        hid.ReloadInputDevices();
    }
    return changed;
}

} // namespace Deluxe8::Input
