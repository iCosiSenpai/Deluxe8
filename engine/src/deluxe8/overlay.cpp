// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The Vulkan WSI headers pull in <windows.h>: keep its macros away from std::min/max.
#ifdef _WIN32
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#endif

#include "deluxe8/overlay.h" // first: it also drops Xlib's Success/BadValue macros (Linux)

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cfloat>
#include <cmath>
#include <deque>
#include <filesystem>
#include <format>
#include <functional>
#include <string>
#include <vector>

#include <SDL3/SDL.h>
#include <imgui.h>
#include <imgui_impl_vulkan.h>

#include "common/logging.h"
#include "common/settings.h"
#include "deluxe8/deluxe8.h"
#include "deluxe8/input_capture.h"
#include "network/network.h"
#include "network/room_member.h"

#ifdef small
#undef small // rpcndr.h
#endif

namespace Deluxe8::Overlay {
namespace {

using Clock = std::chrono::steady_clock;

// ------------------------------------------------------------------------------------------
// Palette (kept in sync with the launcher's design tokens).
constexpr ImU32 Rgba(int r, int g, int b, int a = 255) {
    return IM_COL32(r, g, b, a);
}
constexpr ImU32 kAsfalto = Rgba(0x16, 0x1b, 0x26, 236);
constexpr ImU32 kPannello = Rgba(0x1e, 0x24, 0x33, 255);
constexpr ImU32 kPannello2 = Rgba(0x25, 0x2c, 0x3e, 255);
constexpr ImU32 kLinea = Rgba(0x2c, 0x34, 0x47, 255);
constexpr ImU32 kGesso = Rgba(0xf2, 0xef, 0xe8, 255);
constexpr ImU32 kGesso2 = Rgba(0xb9, 0xbc, 0xc6, 255);
constexpr ImU32 kGesso3 = Rgba(0x7d, 0x83, 0x96, 255);
constexpr ImU32 kViola = Rgba(0x9b, 0x5c, 0xff, 255);
constexpr ImU32 kVerde = Rgba(0x2e, 0xd4, 0x7a, 255);
constexpr ImU32 kGiallo = Rgba(0xff, 0xd2, 0x3f, 255);
constexpr ImU32 kRosso = Rgba(0xff, 0x3b, 0x30, 255);

// ------------------------------------------------------------------------------------------
// Input (main thread produces, present thread consumes).

enum class Nav : uint8_t { Up, Down, Left, Right, Confirm, Back, PrevTab, NextTab, Toggle };

struct PadState {
    SDL_JoystickID id = 0;
    SDL_Gamepad* pad = nullptr;
    bool l3 = false;
    bool r3 = false;
    bool chord_latched = false;
    int stick_x = 0; // -1, 0, 1 after threshold
    int stick_y = 0;
};

std::mutex g_input_mutex;
std::deque<Nav> g_nav_queue;
std::array<std::atomic<bool>, 4> g_held{}; // up, down, left, right from d-pad/stick
std::vector<PadState> g_pads; // main thread only
std::atomic<bool> g_open{false};

void Push(Nav nav) {
    std::scoped_lock lock{g_input_mutex};
    if (g_nav_queue.size() < 64) {
        g_nav_queue.push_back(nav);
    }
}

PadState* FindPad(SDL_JoystickID id) {
    for (auto& pad : g_pads) {
        if (pad.id == id) {
            return &pad;
        }
    }
    return nullptr;
}

PadState& OpenPad(SDL_JoystickID id) {
    if (PadState* existing = FindPad(id)) {
        return *existing;
    }
    PadState state;
    state.id = id;
    state.pad = SDL_OpenGamepad(id); // reference counted; input_common keeps its own handle
    g_pads.push_back(state);
    return g_pads.back();
}

bool IsConfirmLabel(SDL_GamepadButtonLabel label) {
    return label == SDL_GAMEPAD_BUTTON_LABEL_A || label == SDL_GAMEPAD_BUTTON_LABEL_CROSS;
}
bool IsBackLabel(SDL_GamepadButtonLabel label) {
    return label == SDL_GAMEPAD_BUTTON_LABEL_B || label == SDL_GAMEPAD_BUTTON_LABEL_CIRCLE;
}

void SetHeld(int index, bool value) {
    const bool was = g_held[index].exchange(value);
    if (value && !was && g_open.load()) {
        Push(static_cast<Nav>(index)); // Up/Down/Left/Right share the first four indices
    }
}

void OnGamepadButton(const SDL_GamepadButtonEvent& e, bool down) {
    PadState& pad = OpenPad(e.which);
    const auto button = static_cast<SDL_GamepadButton>(e.button);
    if (button == SDL_GAMEPAD_BUTTON_LEFT_STICK) {
        pad.l3 = down;
    } else if (button == SDL_GAMEPAD_BUTTON_RIGHT_STICK) {
        pad.r3 = down;
    }
    if (pad.l3 && pad.r3 && !pad.chord_latched) {
        pad.chord_latched = true;
        Push(Nav::Toggle);
        return;
    }
    if (!pad.l3 && !pad.r3) {
        pad.chord_latched = false;
    }
    switch (button) {
    case SDL_GAMEPAD_BUTTON_DPAD_UP:
        return SetHeld(0, down);
    case SDL_GAMEPAD_BUTTON_DPAD_DOWN:
        return SetHeld(1, down);
    case SDL_GAMEPAD_BUTTON_DPAD_LEFT:
        return SetHeld(2, down);
    case SDL_GAMEPAD_BUTTON_DPAD_RIGHT:
        return SetHeld(3, down);
    default:
        break;
    }
    if (!down || !g_open.load()) {
        return;
    }
    if (button == SDL_GAMEPAD_BUTTON_LEFT_SHOULDER) {
        return Push(Nav::PrevTab);
    }
    if (button == SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER) {
        return Push(Nav::NextTab);
    }
    if (button == SDL_GAMEPAD_BUTTON_START) {
        return Push(Nav::Toggle);
    }
    const SDL_GamepadButtonLabel label =
        pad.pad ? SDL_GetGamepadButtonLabel(pad.pad, button) : SDL_GAMEPAD_BUTTON_LABEL_UNKNOWN;
    if (IsConfirmLabel(label) || (label == SDL_GAMEPAD_BUTTON_LABEL_UNKNOWN &&
                                  button == SDL_GAMEPAD_BUTTON_SOUTH)) {
        Push(Nav::Confirm);
    } else if (IsBackLabel(label) || (label == SDL_GAMEPAD_BUTTON_LABEL_UNKNOWN &&
                                      button == SDL_GAMEPAD_BUTTON_EAST)) {
        Push(Nav::Back);
    }
}

void OnGamepadAxis(const SDL_GamepadAxisEvent& e) {
    PadState& pad = OpenPad(e.which);
    constexpr int kOn = 20000;
    constexpr int kOff = 12000; // hysteresis
    auto quantize = [&](int previous, int value) {
        if (previous == 0) {
            return value > kOn ? 1 : (value < -kOn ? -1 : 0);
        }
        return std::abs(value) < kOff ? 0 : previous;
    };
    if (e.axis == SDL_GAMEPAD_AXIS_LEFTX) {
        pad.stick_x = quantize(pad.stick_x, e.value);
        SetHeld(2, pad.stick_x < 0);
        SetHeld(3, pad.stick_x > 0);
    } else if (e.axis == SDL_GAMEPAD_AXIS_LEFTY) {
        pad.stick_y = quantize(pad.stick_y, e.value);
        SetHeld(0, pad.stick_y < 0);
        SetHeld(1, pad.stick_y > 0);
    }
}

// ------------------------------------------------------------------------------------------
// Menu model bound to the live settings.

struct Choice {
    const char* label;
    int value;
};

struct Item {
    enum class Kind { Choice, Slider, Toggle, Action, Info };
    Kind kind = Kind::Info;
    std::string label;
    std::string hint;
    std::vector<Choice> choices;
    int min = 0, max = 100, step = 5;
    std::function<int()> get;
    std::function<void(int)> set;
    std::function<std::string(int)> format;
    std::function<std::string()> info;
    std::function<void()> action;
    bool next_launch = false; ///< applied at the next start of the game
};

struct Tab {
    const char* name;
    std::vector<Item> items;
};

enum class HudMode { Off, Compact, Full };
std::atomic<int> g_hud_mode{static_cast<int>(HudMode::Compact)};

void EmitSetting(std::string_view key, int value) {
    Emit(std::format(R"({{"t":"setting","key":"{}","value":{}}})", key, value));
}

template <typename Enum, typename SettingT>
Item MakeEnumChoice(std::string label, std::string hint, SettingT& setting, const char* key,
                    std::vector<Choice> choices, bool next_launch = false) {
    Item item;
    item.kind = Item::Kind::Choice;
    item.label = std::move(label);
    item.hint = std::move(hint);
    item.choices = std::move(choices);
    item.next_launch = next_launch;
    item.get = [&setting] { return static_cast<int>(setting.GetValue()); };
    item.set = [&setting, key](int v) {
        setting.SetValue(static_cast<Enum>(v));
        EmitSetting(key, v);
    };
    return item;
}

std::vector<Tab> BuildMenu() {
    using namespace Settings;
    auto& v = Settings::values;
    std::vector<Tab> tabs;

    // ---- Image
    Tab image{L("Image", "Immagine"), {}};
    image.items.push_back(MakeEnumChoice<ScalingFilter>(
        L("Scaling filter", "Filtro di scala"), L("How the internal image is brought to the screen's resolution.", "Come l'immagine interna viene portata alla risoluzione dello schermo."),
        v.scaling_filter, "scaling_filter",
        {{"FSR 1", int(ScalingFilter::Fsr)},
         {"SGSR", int(ScalingFilter::Sgsr)},
         {L("SGSR edges", "SGSR bordi"), int(ScalingFilter::SgsrEdge)},
         {"ScaleForce", int(ScalingFilter::ScaleForce)},
         {"Lanczos", int(ScalingFilter::Lanczos)},
         {L("Bicubic", "Bicubico"), int(ScalingFilter::Bicubic)},
         {L("Bilinear", "Bilineare"), int(ScalingFilter::Bilinear)},
         {"Area", int(ScalingFilter::Area)},
         {"MMPX", int(ScalingFilter::Mmpx)},
         {L("Sharp pixels", "Pixel nitidi"), int(ScalingFilter::NearestNeighbor)}}));
    {
        Item sharp;
        sharp.kind = Item::Kind::Slider;
        sharp.label = L("FSR sharpness", "Nitidezza FSR");
        sharp.hint = L("FSR 1 only. Higher values = a sharper image.", "Solo con FSR 1. Valori alti = immagine più nitida.");
        sharp.min = 0;
        sharp.max = 100;
        sharp.step = 5;
        // Eden's slider is an attenuation (0 = sharpest, 200 = softest): show it as sharpness.
        sharp.get = [&v = Settings::values] { return std::clamp(100 - v.fsr_sharpening_slider.GetValue() / 2, 0, 100); };
        sharp.set = [&v = Settings::values](int s) {
            v.fsr_sharpening_slider.SetValue((100 - s) * 2);
            EmitSetting("fsr_sharpening_slider", (100 - s) * 2);
        };
        sharp.format = [](int s) { return std::format("{}%", s); };
        image.items.push_back(std::move(sharp));
    }
    image.items.push_back(MakeEnumChoice<AntiAliasing>(
        "Anti-aliasing", L("Smooths jagged edges after rendering.", "Ammorbidisce i bordi seghettati dopo il rendering."), v.anti_aliasing,
        "anti_aliasing",
        {{L("None", "Nessuno"), int(AntiAliasing::None)}, {"FXAA", int(AntiAliasing::Fxaa)},
         {"SMAA", int(AntiAliasing::Smaa)}}));
    image.items.push_back(MakeEnumChoice<ResolutionSetup>(
        L("Internal resolution", "Risoluzione interna"), L("Multiplies the game's native resolution (1080p at 1x).", "Moltiplica la risoluzione nativa del gioco (1080p a 1x)."),
        v.resolution_setup, "resolution_setup",
        {{L("0.5x", "0,5x"), int(ResolutionSetup::Res1_2X)},
         {L("0.75x", "0,75x"), int(ResolutionSetup::Res3_4X)},
         {"1x (1080p)", int(ResolutionSetup::Res1X)},
         {L("1.5x", "1,5x"), int(ResolutionSetup::Res3_2X)},
         {"2x (4K)", int(ResolutionSetup::Res2X)},
         {"3x", int(ResolutionSetup::Res3X)},
         {"4x (8K)", int(ResolutionSetup::Res4X)}},
        true));
    image.items.push_back(MakeEnumChoice<AspectRatio>(
        L("Aspect ratio", "Formato"), L("The game is made for 16:9; other ratios stretch the image.", "Il gioco è pensato per 16:9; gli altri formati allungano l'immagine."),
        v.aspect_ratio, "aspect_ratio",
        {{"16:9", int(AspectRatio::R16_9)}, {"16:10", int(AspectRatio::R16_10)},
         {"21:9", int(AspectRatio::R21_9)}, {"4:3", int(AspectRatio::R4_3)},
         {L("Fit the window", "Adatta alla finestra"), int(AspectRatio::Stretch)}}));
    image.items.push_back(MakeEnumChoice<AnisotropyMode>(
        L("Anisotropic filtering", "Filtro anisotropico"), L("Sharper textures in the distance, almost free.", "Texture più definite in lontananza, costo quasi nullo."),
        v.max_anisotropy, "max_anisotropy",
        {{L("Automatic", "Automatico"), int(AnisotropyMode::Automatic)}, {"2x", int(AnisotropyMode::X2)},
         {"4x", int(AnisotropyMode::X4)}, {"8x", int(AnisotropyMode::X8)},
         {"16x", int(AnisotropyMode::X16)}},
        true));
    tabs.push_back(std::move(image));

    // ---- Smoothness
    Tab smooth{L("Smoothness", "Fluidità"), {}};
#ifdef HAS_LSFG
    {
        Item fg;
        fg.kind = Item::Kind::Choice;
        fg.label = "Frame generation";
        fg.hint = L("Lossless Scaling (LSFG): frames interpolated between the game's.", "Lossless Scaling (LSFG): fotogrammi interpolati tra quelli del gioco.");
        fg.choices = {{L("Off", "Spenta"), 0}, {"2x", 2}, {"3x", 3}, {"4x", 4}};
        fg.get = [&v = Settings::values] {
            return v.frame_gen.GetValue() ? static_cast<int>(v.frame_gen_multiplier.GetValue()) : 0;
        };
        fg.set = [&v = Settings::values](int m) {
            v.frame_gen.SetValue(m != 0);
            if (m != 0) {
                v.frame_gen_multiplier.SetValue(static_cast<u32>(m));
            }
            EmitSetting("frame_gen_multiplier", m);
        };
        smooth.items.push_back(std::move(fg));
    }
    {
        Item flow;
        flow.kind = Item::Kind::Slider;
        flow.label = L("Flow scale", "Scala del flusso");
        flow.hint = L("Precision of the motion estimate. 0 = automatic.", "Precisione del calcolo del movimento. 0 = automatica.");
        flow.min = 0;
        flow.max = 100;
        flow.step = 5;
        flow.get = [&v = Settings::values] {
            return v.frame_gen_flow_scale_auto.GetValue()
                       ? 0
                       : static_cast<int>(v.frame_gen_flow_scale.GetValue());
        };
        flow.set = [&v = Settings::values](int s) {
            v.frame_gen_flow_scale_auto.SetValue(s == 0);
            if (s != 0) {
                v.frame_gen_flow_scale.SetValue(static_cast<u32>(std::max(s, 25)));
            }
            EmitSetting("frame_gen_flow_scale", s);
        };
        flow.format = [](int s) { return s == 0 ? std::string{L("Automatic", "Automatica")} : std::format("{}%", s); };
        smooth.items.push_back(std::move(flow));
    }
#else
    {
        Item na;
        na.label = "Frame generation";
        na.info = [] { return std::string{L("Not included in this build", "Non inclusa in questa build")}; };
        smooth.items.push_back(std::move(na));
    }
#endif
    {
        Item limit;
        limit.kind = Item::Kind::Toggle;
        limit.label = L("Console speed", "Velocità come su console");
        limit.hint = L("Turn it off for testing only: the game would run faster than normal.", "Spegnilo solo per test: il gioco andrebbe più veloce del normale.");
        limit.get = [&v = Settings::values] { return v.use_speed_limit.GetValue() ? 1 : 0; };
        limit.set = [&v = Settings::values](int on) {
            v.use_speed_limit.SetValue(on != 0);
            EmitSetting("use_speed_limit", on);
        };
        smooth.items.push_back(std::move(limit));
    }
    {
        Item hud;
        hud.kind = Item::Kind::Choice;
        hud.label = L("Performance overlay", "Indicatore prestazioni");
        hud.hint = L("FPS and frame times in the corner of the screen.", "FPS e tempi del fotogramma nell'angolo dello schermo.");
        hud.choices = {{L("Off", "Spento"), int(HudMode::Off)},
                       {L("Essential", "Essenziale"), int(HudMode::Compact)},
                       {L("Full", "Completo"), int(HudMode::Full)}};
        hud.get = [] { return g_hud_mode.load(); };
        hud.set = [](int m) {
            g_hud_mode.store(m);
            EmitSetting("hud", m);
        };
        smooth.items.push_back(std::move(hud));
    }
    tabs.push_back(std::move(smooth));

    // ---- Audio
    Tab audio{"Audio", {}};
    {
        Item vol;
        vol.kind = Item::Kind::Slider;
        vol.label = "Volume";
        vol.min = 0;
        vol.max = 100;
        vol.step = 5;
        vol.get = [&v = Settings::values] { return static_cast<int>(v.volume.GetValue()); };
        vol.set = [&v = Settings::values](int x) {
            v.volume.SetValue(static_cast<u8>(x));
            EmitSetting("volume", x);
        };
        vol.format = [](int x) { return std::format("{}%", x); };
        audio.items.push_back(std::move(vol));
    }
    {
        Item mute;
        mute.kind = Item::Kind::Toggle;
        mute.label = L("Mute", "Muto");
        mute.get = [&v = Settings::values] { return v.audio_muted.GetValue() ? 1 : 0; };
        mute.set = [&v = Settings::values](int on) {
            v.audio_muted.SetValue(on != 0);
            EmitSetting("audio_muted", on);
        };
        audio.items.push_back(std::move(mute));
    }
    tabs.push_back(std::move(audio));

    // ---- Online
    Tab online{"Online", {}};
    {
        Item room;
        room.label = L("Room", "Stanza");
        room.info = [] {
            if (auto member = Network::GetRoomMember().lock(); member && member->IsConnected()) {
                const auto info = member->GetRoomInformation();
                return std::format("{} ({}/{})", info.name, member->GetMemberInformation().size(),
                                   info.member_slots);
            }
            return std::string{L("Not connected", "Non connesso")};
        };
        online.items.push_back(std::move(room));

        Item players;
        players.label = L("Players", "Giocatori");
        players.info = [] {
            std::string list;
            if (auto member = Network::GetRoomMember().lock(); member && member->IsConnected()) {
                for (const auto& m : member->GetMemberInformation()) {
                    if (!list.empty()) {
                        list += ", ";
                    }
                    list += m.nickname;
                }
            }
            return list.empty() ? std::string{"-"} : list;
        };
        online.items.push_back(std::move(players));

        Item leave;
        leave.kind = Item::Kind::Action;
        leave.label = L("Leave the room", "Lascia la stanza");
        leave.action = [] {
            if (auto member = Network::GetRoomMember().lock(); member && member->IsConnected()) {
                member->Leave();
                Emit(R"({"t":"room","state":"left"})");
            }
        };
        online.items.push_back(std::move(leave));
    }
    tabs.push_back(std::move(online));

    // ---- Game
    Tab game{L("Match", "Partita"), {}};
    {
        Item resume;
        resume.kind = Item::Kind::Action;
        resume.label = L("Resume", "Riprendi");
        resume.action = [] { Push(Nav::Toggle); };
        game.items.push_back(std::move(resume));

        Item quit;
        quit.kind = Item::Kind::Action;
        quit.label = L("Quit the game", "Esci dal gioco");
        quit.hint = L("Closes the game and goes back to the launcher. Save first if you are in a Grand Prix.", "Chiude il gioco e torna al launcher. Salva prima, se sei in un Gran Premio.");
        quit.action = [] {
            if (const auto& exit = GetActions().request_exit) {
                exit();
            }
        };
        game.items.push_back(std::move(quit));
    }
    tabs.push_back(std::move(game));
    return tabs;
}

// ------------------------------------------------------------------------------------------
// Present-thread state.

struct MenuState {
    std::vector<Tab> tabs;
    int tab = 0;
    int row = 0;
    int confirm_row = -1; ///< row waiting for a second confirm (destructive actions)
};

struct FrameStats {
    std::array<float, 180> frametimes{};
    size_t head = 0;
    size_t count = 0;
    Clock::time_point last{};
    float present_fps = 0.0f;
    int frames_this_second = 0;
    Clock::time_point second_start{};
};

struct ImageTarget {
    VkImage image = VK_NULL_HANDLE;
    VkImageView view = VK_NULL_HANDLE;
    VkFramebuffer framebuffer = VK_NULL_HANDLE;
    VkExtent2D extent{};
};

struct Garbage {
    VkImageView view = VK_NULL_HANDLE;
    VkFramebuffer framebuffer = VK_NULL_HANDLE;
    VkRenderPass render_pass = VK_NULL_HANDLE;
    uint64_t frame = 0;
};

struct VulkanFns {
    PFN_vkGetDeviceProcAddr GetDeviceProcAddr = nullptr;
    PFN_vkGetDeviceQueue GetDeviceQueue = nullptr;
    PFN_vkCreateRenderPass CreateRenderPass = nullptr;
    PFN_vkDestroyRenderPass DestroyRenderPass = nullptr;
    PFN_vkCreateImageView CreateImageView = nullptr;
    PFN_vkDestroyImageView DestroyImageView = nullptr;
    PFN_vkCreateFramebuffer CreateFramebuffer = nullptr;
    PFN_vkDestroyFramebuffer DestroyFramebuffer = nullptr;
    PFN_vkCmdPipelineBarrier CmdPipelineBarrier = nullptr;
    PFN_vkCmdBeginRenderPass CmdBeginRenderPass = nullptr;
    PFN_vkCmdEndRenderPass CmdEndRenderPass = nullptr;
    PFN_vkDeviceWaitIdle DeviceWaitIdle = nullptr;
};

struct Renderer {
    bool initialized = false;
    bool failed = false;
    VkInstance instance = VK_NULL_HANDLE;
    PFN_vkGetInstanceProcAddr gipa = nullptr;
    VkDevice device = VK_NULL_HANDLE;
    VkFormat format = VK_FORMAT_UNDEFINED;
    VkRenderPass render_pass = VK_NULL_HANDLE;
    VulkanFns fn{};
    std::vector<ImageTarget> images;
    std::vector<Garbage> garbage;
    uint64_t frame = 0;
    ImGuiContext* context = nullptr;
    ImFont* font = nullptr;
    MenuState menu;
    FrameStats stats;
    Clock::time_point start{};
    Clock::time_point last_frame{};
    Clock::time_point repeat_at{};
    int repeat_dir = -1;
};

Renderer g_r;

PFN_vkVoidFunction LoadVk(const char* name, void* /*user*/) {
    PFN_vkVoidFunction f = nullptr;
    if (g_r.device != VK_NULL_HANDLE && g_r.fn.GetDeviceProcAddr != nullptr) {
        f = g_r.fn.GetDeviceProcAddr(g_r.device, name);
    }
    if (f == nullptr) {
        f = g_r.gipa(g_r.instance, name);
    }
    return f;
}

template <typename T>
bool LoadInto(T& out, const char* name) {
    out = reinterpret_cast<T>(LoadVk(name, nullptr));
    return out != nullptr;
}

VkRenderPass CreateRenderPass(VkFormat format) {
    const VkAttachmentDescription attachment{
        .flags = 0,
        .format = format,
        .samples = VK_SAMPLE_COUNT_1_BIT,
        .loadOp = VK_ATTACHMENT_LOAD_OP_LOAD,
        .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
        .stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        .stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
        .initialLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        .finalLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
    };
    const VkAttachmentReference color_ref{0, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL};
    const VkSubpassDescription subpass{
        .flags = 0,
        .pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS,
        .inputAttachmentCount = 0,
        .pInputAttachments = nullptr,
        .colorAttachmentCount = 1,
        .pColorAttachments = &color_ref,
        .pResolveAttachments = nullptr,
        .pDepthStencilAttachment = nullptr,
        .preserveAttachmentCount = 0,
        .pPreserveAttachments = nullptr,
    };
    const VkSubpassDependency dependency{
        .srcSubpass = VK_SUBPASS_EXTERNAL,
        .dstSubpass = 0,
        .srcStageMask = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
        .dstStageMask = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
        .srcAccessMask = 0,
        .dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT | VK_ACCESS_COLOR_ATTACHMENT_READ_BIT,
        .dependencyFlags = 0,
    };
    const VkRenderPassCreateInfo info{
        .sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        .pNext = nullptr,
        .flags = 0,
        .attachmentCount = 1,
        .pAttachments = &attachment,
        .subpassCount = 1,
        .pSubpasses = &subpass,
        .dependencyCount = 1,
        .pDependencies = &dependency,
    };
    VkRenderPass pass = VK_NULL_HANDLE;
    if (g_r.fn.CreateRenderPass(g_r.device, &info, nullptr, &pass) != VK_SUCCESS) {
        return VK_NULL_HANDLE;
    }
    return pass;
}

void CollectGarbage(bool all) {
    auto it = g_r.garbage.begin();
    while (it != g_r.garbage.end()) {
        if (all || g_r.frame >= it->frame + 16) {
            if (it->framebuffer) {
                g_r.fn.DestroyFramebuffer(g_r.device, it->framebuffer, nullptr);
            }
            if (it->view) {
                g_r.fn.DestroyImageView(g_r.device, it->view, nullptr);
            }
            if (it->render_pass) {
                g_r.fn.DestroyRenderPass(g_r.device, it->render_pass, nullptr);
            }
            it = g_r.garbage.erase(it);
        } else {
            ++it;
        }
    }
}

void RetireImages() {
    for (auto& img : g_r.images) {
        g_r.garbage.push_back({img.view, img.framebuffer, VK_NULL_HANDLE, g_r.frame});
    }
    g_r.images.clear();
}

std::string PickFont() {
    std::string path = OverlayFontPath();
    std::error_code ec;
    if (!path.empty() && std::filesystem::exists(path, ec)) {
        return path;
    }
#ifdef _WIN32
    for (const char* candidate : {"C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\arial.ttf"}) {
        if (std::filesystem::exists(candidate, ec)) {
            return candidate;
        }
    }
#else
    for (const char* candidate : {"/usr/share/fonts/TTF/DejaVuSans.ttf",
                                  "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                                  "/usr/share/fonts/noto/NotoSans-Regular.ttf"}) {
        if (std::filesystem::exists(candidate, ec)) {
            return candidate;
        }
    }
#endif
    return {};
}

bool InitRenderer(const VulkanTarget& t) {
    g_r.instance = t.instance;
    g_r.gipa = t.get_instance_proc_addr;
    g_r.device = t.device;
    g_r.fn.GetDeviceProcAddr = reinterpret_cast<PFN_vkGetDeviceProcAddr>(
        g_r.gipa(g_r.instance, "vkGetDeviceProcAddr"));
    if (g_r.fn.GetDeviceProcAddr == nullptr) {
        return false;
    }
    bool ok = true;
    ok &= LoadInto(g_r.fn.GetDeviceQueue, "vkGetDeviceQueue");
    ok &= LoadInto(g_r.fn.CreateRenderPass, "vkCreateRenderPass");
    ok &= LoadInto(g_r.fn.DestroyRenderPass, "vkDestroyRenderPass");
    ok &= LoadInto(g_r.fn.CreateImageView, "vkCreateImageView");
    ok &= LoadInto(g_r.fn.DestroyImageView, "vkDestroyImageView");
    ok &= LoadInto(g_r.fn.CreateFramebuffer, "vkCreateFramebuffer");
    ok &= LoadInto(g_r.fn.DestroyFramebuffer, "vkDestroyFramebuffer");
    ok &= LoadInto(g_r.fn.CmdPipelineBarrier, "vkCmdPipelineBarrier");
    ok &= LoadInto(g_r.fn.CmdBeginRenderPass, "vkCmdBeginRenderPass");
    ok &= LoadInto(g_r.fn.CmdEndRenderPass, "vkCmdEndRenderPass");
    ok &= LoadInto(g_r.fn.DeviceWaitIdle, "vkDeviceWaitIdle");
    if (!ok) {
        return false;
    }

    g_r.context = ImGui::CreateContext();
    ImGui::SetCurrentContext(g_r.context);
    ImGuiIO& io = ImGui::GetIO();
    io.IniFilename = nullptr;
    io.LogFilename = nullptr;
    io.ConfigFlags |= ImGuiConfigFlags_NoMouseCursorChange;

    const std::string font = PickFont();
    if (!font.empty()) {
        ImFontConfig config;
        config.OversampleH = 2;
        g_r.font = io.Fonts->AddFontFromFileTTF(font.c_str(), 20.0f, &config);
    }
    if (g_r.font == nullptr) {
        g_r.font = io.Fonts->AddFontDefault();
    }

    if (!ImGui_ImplVulkan_LoadFunctions(VK_API_VERSION_1_1, LoadVk, nullptr)) {
        return false;
    }

    g_r.format = t.format;
    g_r.render_pass = CreateRenderPass(t.format);
    if (g_r.render_pass == VK_NULL_HANDLE) {
        return false;
    }

    VkQueue queue = VK_NULL_HANDLE;
    g_r.fn.GetDeviceQueue(t.device, t.queue_family, 0, &queue);

    ImGui_ImplVulkan_InitInfo init{};
    init.ApiVersion = VK_API_VERSION_1_1;
    init.Instance = t.instance;
    init.PhysicalDevice = t.physical_device;
    init.Device = t.device;
    init.QueueFamily = t.queue_family;
    init.Queue = queue;
    init.DescriptorPoolSize = 32;
    init.MinImageCount = 2;
    init.ImageCount = std::max<uint32_t>(2, t.image_count);
    init.PipelineInfoMain.RenderPass = g_r.render_pass;
    init.PipelineInfoMain.Subpass = 0;
    init.PipelineInfoMain.MSAASamples = VK_SAMPLE_COUNT_1_BIT;
    if (!ImGui_ImplVulkan_Init(&init)) {
        return false;
    }

    g_r.menu.tabs = BuildMenu();
    g_r.start = Clock::now();
    g_r.last_frame = g_r.start;
    g_r.stats.second_start = g_r.start;
    g_r.initialized = true;
    LOG_INFO(Render_Vulkan, "Deluxe8 overlay ready (font: {})", font.empty() ? "default" : font);
    return true;
}

ImageTarget* TargetFor(const VulkanTarget& t) {
    for (auto& img : g_r.images) {
        if (img.image == t.image) {
            if (img.extent.width == t.extent.width && img.extent.height == t.extent.height) {
                return &img;
            }
            g_r.garbage.push_back({img.view, img.framebuffer, VK_NULL_HANDLE, g_r.frame});
            img = {};
            break;
        }
    }
    // Swapchain images we have never seen: the old set is gone (recreated swapchain).
    if (g_r.images.size() >= std::max<uint32_t>(t.image_count, 1)) {
        RetireImages();
    }
    ImageTarget target;
    target.image = t.image;
    target.extent = t.extent;
    const VkImageViewCreateInfo view_info{
        .sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        .pNext = nullptr,
        .flags = 0,
        .image = t.image,
        .viewType = VK_IMAGE_VIEW_TYPE_2D,
        .format = t.format,
        .components = {},
        .subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1},
    };
    if (g_r.fn.CreateImageView(g_r.device, &view_info, nullptr, &target.view) != VK_SUCCESS) {
        return nullptr;
    }
    const VkFramebufferCreateInfo fb_info{
        .sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
        .pNext = nullptr,
        .flags = 0,
        .renderPass = g_r.render_pass,
        .attachmentCount = 1,
        .pAttachments = &target.view,
        .width = t.extent.width,
        .height = t.extent.height,
        .layers = 1,
    };
    if (g_r.fn.CreateFramebuffer(g_r.device, &fb_info, nullptr, &target.framebuffer) != VK_SUCCESS) {
        g_r.fn.DestroyImageView(g_r.device, target.view, nullptr);
        return nullptr;
    }
    // Reuse an emptied slot if there is one.
    for (auto& img : g_r.images) {
        if (img.image == VK_NULL_HANDLE) {
            img = target;
            return &img;
        }
    }
    g_r.images.push_back(target);
    return &g_r.images.back();
}

void HandleFormatChange(const VulkanTarget& t) {
    if (t.format == g_r.format) {
        return;
    }
    RetireImages();
    g_r.garbage.push_back({VK_NULL_HANDLE, VK_NULL_HANDLE, g_r.render_pass, g_r.frame});
    g_r.format = t.format;
    g_r.render_pass = CreateRenderPass(t.format);
    ImGui_ImplVulkan_PipelineInfo info{};
    info.RenderPass = g_r.render_pass;
    info.Subpass = 0;
    info.MSAASamples = VK_SAMPLE_COUNT_1_BIT;
    ImGui_ImplVulkan_CreateMainPipeline(&info);
}

// ------------------------------------------------------------------------------------------
// Menu logic.

void SetOpen(bool open) {
    if (g_open.exchange(open) == open) {
        return;
    }
    g_input_captured.store(open);
    if (open) {
        g_r.menu.tabs = BuildMenu(); // re-read values that may have changed elsewhere
        g_r.menu.row = std::min<int>(g_r.menu.row, static_cast<int>(g_r.menu.tabs[g_r.menu.tab].items.size()) - 1);
    }
    g_r.menu.confirm_row = -1;
    Emit(std::format(R"({{"t":"overlay","open":{}}})", open ? "true" : "false"));
}

int StepChoice(const Item& item, int direction) {
    const int current = item.get();
    int index = 0;
    for (size_t i = 0; i < item.choices.size(); ++i) {
        if (item.choices[i].value == current) {
            index = static_cast<int>(i);
            break;
        }
    }
    index = std::clamp(index + direction, 0, static_cast<int>(item.choices.size()) - 1);
    return item.choices[static_cast<size_t>(index)].value;
}

void ApplyNav(Nav nav) {
    if (nav == Nav::Toggle) {
        SetOpen(!g_open.load());
        return;
    }
    if (!g_open.load()) {
        return;
    }
    MenuState& m = g_r.menu;
    if (m.tabs.empty()) {
        return;
    }
    auto& items = m.tabs[static_cast<size_t>(m.tab)].items;
    const int count = static_cast<int>(items.size());
    switch (nav) {
    case Nav::Up:
        m.row = (m.row - 1 + count) % count;
        m.confirm_row = -1;
        break;
    case Nav::Down:
        m.row = (m.row + 1) % count;
        m.confirm_row = -1;
        break;
    case Nav::PrevTab:
    case Nav::NextTab: {
        const int tabs = static_cast<int>(m.tabs.size());
        m.tab = (m.tab + (nav == Nav::NextTab ? 1 : -1) + tabs) % tabs;
        m.row = 0;
        m.confirm_row = -1;
        break;
    }
    case Nav::Left:
    case Nav::Right: {
        Item& item = items[static_cast<size_t>(m.row)];
        const int dir = nav == Nav::Right ? 1 : -1;
        if (item.kind == Item::Kind::Choice) {
            item.set(StepChoice(item, dir));
        } else if (item.kind == Item::Kind::Slider) {
            item.set(std::clamp(item.get() + dir * item.step, item.min, item.max));
        } else if (item.kind == Item::Kind::Toggle) {
            item.set(dir > 0 ? 1 : 0);
        }
        break;
    }
    case Nav::Confirm: {
        Item& item = items[static_cast<size_t>(m.row)];
        if (item.kind == Item::Kind::Toggle) {
            item.set(item.get() ? 0 : 1);
        } else if (item.kind == Item::Kind::Choice) {
            const int next = StepChoice(item, 1);
            item.set(next == item.get() ? item.choices.front().value : next);
        } else if (item.kind == Item::Kind::Action && item.action) {
            const bool destructive = item.label == L("Quit the game", "Esci dal gioco");
            if (destructive && m.confirm_row != m.row) {
                m.confirm_row = m.row;
            } else {
                m.confirm_row = -1;
                item.action();
            }
        }
        break;
    }
    case Nav::Back:
        if (m.confirm_row >= 0) {
            m.confirm_row = -1;
        } else {
            SetOpen(false);
        }
        break;
    default:
        break;
    }
}

void PumpInput() {
    std::deque<Nav> events;
    {
        std::scoped_lock lock{g_input_mutex};
        events.swap(g_nav_queue);
    }
    for (const Nav nav : events) {
        ApplyNav(nav);
    }
    // Auto-repeat for held directions (gamepad d-pad / stick).
    const auto now = Clock::now();
    int held = -1;
    for (int i = 0; i < 4; ++i) {
        if (g_held[i].load()) {
            held = i;
            break;
        }
    }
    if (held != g_r.repeat_dir) {
        g_r.repeat_dir = held;
        g_r.repeat_at = now + std::chrono::milliseconds(380);
    } else if (held >= 0 && now >= g_r.repeat_at) {
        ApplyNav(static_cast<Nav>(held));
        g_r.repeat_at = now + std::chrono::milliseconds(110);
    }
}

// ------------------------------------------------------------------------------------------
// Drawing.

void Text(ImDrawList* dl, ImVec2 pos, float size, ImU32 color, std::string_view text) {
    dl->AddText(g_r.font, size, pos, color, text.data(), text.data() + text.size());
}

float TextWidth(float size, std::string_view text) {
    return g_r.font->CalcTextSizeA(size, FLT_MAX, 0.0f, text.data(), text.data() + text.size()).x;
}

void UpdateStats() {
    FrameStats& s = g_r.stats;
    const auto now = Clock::now();
    if (s.last.time_since_epoch().count() != 0) {
        const float ms = std::chrono::duration<float, std::milli>(now - s.last).count();
        s.frametimes[s.head] = ms;
        s.head = (s.head + 1) % s.frametimes.size();
        s.count = std::min(s.count + 1, s.frametimes.size());
    }
    s.last = now;
    ++s.frames_this_second;
    const float elapsed = std::chrono::duration<float>(now - s.second_start).count();
    if (elapsed >= 0.5f) {
        s.present_fps = static_cast<float>(s.frames_this_second) / elapsed;
        s.frames_this_second = 0;
        s.second_start = now;
    }
}

void DrawHud(ImDrawList* dl, float scale) {
    const auto mode = static_cast<HudMode>(g_hud_mode.load());
    if (mode == HudMode::Off) {
        return;
    }
    const PerfSample perf = LatestPerf();
    const FrameStats& s = g_r.stats;
    const float pad = 10.0f * scale;
    const float big_px = 22.0f * scale;
    const float small_px = 15.0f * scale;
    const ImVec2 origin{16.0f * scale, 16.0f * scale};

    const std::string fps = std::format("{:.0f}", perf.guest_fps > 0 ? perf.guest_fps : static_cast<double>(s.present_fps));
    const bool generating = s.present_fps > perf.guest_fps * 1.5 && perf.guest_fps > 1.0;
    const std::string sub = generating ? std::vformat(L("Game FPS  {:.0f} on screen", "FPS gioco  {:.0f} a schermo"), std::make_format_args(s.present_fps))
                                       : std::string{"FPS"};
    const float width = std::max(TextWidth(big_px, fps) + TextWidth(small_px, sub) + pad * 3.0f,
                                 mode == HudMode::Full ? 260.0f * scale : 0.0f);
    const float height = mode == HudMode::Full ? 96.0f * scale : big_px + pad * 2.0f;
    dl->AddRectFilled(origin, {origin.x + width, origin.y + height}, kAsfalto, 6.0f * scale);

    const ImU32 fps_color = perf.guest_fps >= 58.0 || perf.guest_fps == 0.0 ? kVerde
                            : perf.guest_fps >= 50.0                       ? kGiallo
                                                                            : kRosso;
    Text(dl, {origin.x + pad, origin.y + pad}, big_px, fps_color, fps);
    Text(dl, {origin.x + pad * 2.0f + TextWidth(big_px, fps), origin.y + pad + (big_px - small_px) * 0.7f},
         small_px, kGesso2, sub);
    if (mode != HudMode::Full) {
        return;
    }
    const std::string line =
        std::vformat(L("{:.1f} ms   speed {:.0f}%", "{:.1f} ms   velocità {:.0f}%"), std::make_format_args(perf.frametime_ms, perf.speed_percent));
    Text(dl, {origin.x + pad, origin.y + pad + big_px + 4.0f * scale}, small_px, kGesso2, line);

    // Frame-time graph of presented frames (16.7 ms line = 60 fps).
    const ImVec2 g0{origin.x + pad, origin.y + height - pad - 24.0f * scale};
    const ImVec2 g1{origin.x + width - pad, origin.y + height - pad};
    dl->AddRectFilled(g0, g1, kPannello);
    const float target_y = g1.y - (16.7f / 50.0f) * (g1.y - g0.y);
    dl->AddLine({g0.x, target_y}, {g1.x, target_y}, kLinea, 1.0f);
    if (s.count > 1) {
        const size_t n = s.count;
        const float step = (g1.x - g0.x) / static_cast<float>(s.frametimes.size() - 1);
        ImVec2 prev{};
        for (size_t i = 0; i < n; ++i) {
            const size_t idx = (s.head + s.frametimes.size() - n + i) % s.frametimes.size();
            const float ms = std::min(s.frametimes[idx], 50.0f);
            const ImVec2 p{g0.x + step * static_cast<float>(s.frametimes.size() - n + i),
                           g1.y - (ms / 50.0f) * (g1.y - g0.y)};
            if (i > 0) {
                dl->AddLine(prev, p, ms > 25.0f ? kGiallo : kGesso2, 1.2f * scale);
            }
            prev = p;
        }
    }
}

void DrawToast(ImDrawList* dl, float scale, const ImVec2& display) {
    const float age = std::chrono::duration<float>(Clock::now() - g_r.start).count();
    if (age > 7.0f || g_open.load()) {
        return;
    }
    const float alpha = age < 6.0f ? 1.0f : 7.0f - age;
    const float size = 18.0f * scale;
    const std::string text = L("L3 + R3 (or F1) opens the Deluxe8 menu", "L3 + R3 (o F1) apre il menu Deluxe8");
    const float w = TextWidth(size, text) + 28.0f * scale;
    const ImVec2 p0{(display.x - w) * 0.5f, display.y - 72.0f * scale};
    const ImVec2 p1{p0.x + w, p0.y + size + 20.0f * scale};
    dl->AddRectFilled(p0, p1, IM_COL32(0x16, 0x1b, 0x26, static_cast<int>(230 * alpha)), 8.0f * scale);
    dl->AddRectFilled(p0, {p0.x + 4.0f * scale, p1.y}, IM_COL32(0x9b, 0x5c, 0xff, static_cast<int>(255 * alpha)));
    Text(dl, {p0.x + 16.0f * scale, p0.y + 10.0f * scale}, size,
         IM_COL32(0xf2, 0xef, 0xe8, static_cast<int>(255 * alpha)), text);
}

std::string ValueText(const Item& item) {
    switch (item.kind) {
    case Item::Kind::Choice: {
        const int value = item.get();
        for (const auto& c : item.choices) {
            if (c.value == value) {
                return c.label;
            }
        }
        return "?";
    }
    case Item::Kind::Slider:
        return item.format ? item.format(item.get()) : std::to_string(item.get());
    case Item::Kind::Toggle:
        return item.get() ? L("Yes", "Sì") : L("No", "No");
    case Item::Kind::Info:
        return item.info ? item.info() : std::string{};
    case Item::Kind::Action:
        return {};
    }
    return {};
}

void DrawMenu(ImDrawList* dl, float scale, const ImVec2& display) {
    MenuState& m = g_r.menu;
    if (m.tabs.empty()) {
        return;
    }
    dl->AddRectFilled({0, 0}, display, IM_COL32(8, 10, 16, 120)); // dim the game

    const float margin = 28.0f * scale;
    const float width = std::min(620.0f * scale, display.x - margin * 2.0f);
    const ImVec2 p0{margin, margin};
    const ImVec2 p1{margin + width, display.y - margin};
    dl->AddRectFilled(p0, p1, kAsfalto, 12.0f * scale);
    dl->AddRect(p0, p1, kLinea, 12.0f * scale, 1.0f);

    const float pad = 26.0f * scale;
    float y = p0.y + pad;
    Text(dl, {p0.x + pad, y}, 30.0f * scale, kGesso, "Deluxe8");
    const std::string version = GameVersion().empty() ? "MK8 Deluxe" : "MK8 Deluxe " + GameVersion();
    Text(dl, {p1.x - pad - TextWidth(16.0f * scale, version), y + 10.0f * scale}, 16.0f * scale,
         kGesso3, version);
    y += 52.0f * scale;

    // Tabs
    float x = p0.x + pad;
    const float tab_size = 18.0f * scale;
    for (size_t i = 0; i < m.tabs.size(); ++i) {
        const bool active = static_cast<int>(i) == m.tab;
        const float w = TextWidth(tab_size, m.tabs[i].name);
        Text(dl, {x, y}, tab_size, active ? kGesso : kGesso3, m.tabs[i].name);
        if (active) {
            dl->AddRectFilled({x, y + tab_size + 6.0f * scale}, {x + w, y + tab_size + 9.0f * scale}, kGesso);
        }
        x += w + 26.0f * scale;
    }
    y += tab_size + 24.0f * scale;
    dl->AddLine({p0.x + pad, y}, {p1.x - pad, y}, kLinea, 1.0f);
    y += 10.0f * scale;

    // Rows
    const auto& items = m.tabs[static_cast<size_t>(m.tab)].items;
    const float label_size = 19.0f * scale;
    const float row_h = 50.0f * scale;
    for (size_t i = 0; i < items.size(); ++i) {
        const Item& item = items[i];
        const bool selected = static_cast<int>(i) == m.row;
        const ImVec2 r0{p0.x + pad * 0.5f, y};
        const ImVec2 r1{p1.x - pad * 0.5f, y + row_h};
        if (selected) {
            dl->AddRectFilled(r0, r1, kPannello2, 6.0f * scale);
            dl->AddRectFilled(r0, {r0.x + 4.0f * scale, r1.y}, kViola, 2.0f * scale);
        }
        const float text_y = y + (row_h - label_size) * 0.5f;
        const ImU32 label_color = item.kind == Item::Kind::Info ? kGesso2 : kGesso;
        std::string label = item.label;
        if (item.kind == Item::Kind::Action && m.confirm_row == static_cast<int>(i)) {
            label = L("Press again to confirm", "Premi di nuovo per confermare");
        }
        Text(dl, {r0.x + pad * 0.5f + 6.0f * scale, text_y}, label_size,
             m.confirm_row == static_cast<int>(i) ? kGiallo : label_color, label);

        std::string value = ValueText(item);
        if (!value.empty()) {
            const bool adjustable = item.kind == Item::Kind::Choice || item.kind == Item::Kind::Slider ||
                                    item.kind == Item::Kind::Toggle;
            if (adjustable && selected) {
                value = "\xE2\x80\xB9  " + value + "  \xE2\x80\xBA"; // ‹ value ›
            }
            const float vw = TextWidth(label_size, value);
            Text(dl, {r1.x - pad * 0.5f - vw, text_y}, label_size,
                 item.kind == Item::Kind::Info ? kGesso3 : (selected ? kGesso : kGesso2), value);
        }
        if (item.kind == Item::Kind::Slider) {
            const float frac = static_cast<float>(item.get() - item.min) /
                               static_cast<float>(std::max(1, item.max - item.min));
            const float bx0 = r0.x + pad * 0.5f + 6.0f * scale;
            const float bx1 = r1.x - pad * 0.5f;
            const float by = r1.y - 7.0f * scale;
            dl->AddRectFilled({bx0, by}, {bx1, by + 2.0f * scale}, kLinea);
            dl->AddRectFilled({bx0, by}, {bx0 + (bx1 - bx0) * frac, by + 2.0f * scale},
                              selected ? kViola : kGesso3);
        }
        y += row_h + 4.0f * scale;
    }

    // Hint for the selected row
    const Item& current = items[static_cast<size_t>(std::clamp<int>(m.row, 0, static_cast<int>(items.size()) - 1))];
    std::string hint = current.hint;
    if (current.next_launch) {
        hint += hint.empty() ? "" : " ";
        hint += L("Applies at the next start.", "Si applica al prossimo avvio.");
    }
    const float hint_size = 16.0f * scale;
    if (!hint.empty()) {
        y += 10.0f * scale;
        dl->PushClipRect({p0.x + pad, y}, {p1.x - pad, y + hint_size * 3.0f});
        dl->AddText(g_r.font, hint_size, {p0.x + pad, y}, kGesso2, hint.c_str(), nullptr,
                    width - pad * 2.0f);
        dl->PopClipRect();
    }

    // Footer: controller hints
    const float fy = p1.y - pad - hint_size;
    dl->AddLine({p0.x + pad, fy - 14.0f * scale}, {p1.x - pad, fy - 14.0f * scale}, kLinea, 1.0f);
    float fx = p0.x + pad;
    for (const auto& [key, what] : std::array<std::pair<const char*, const char*>, 4>{{
             {L("D-pad", "Croce"), L("choose", "scegli")}, {"A", L("confirm", "conferma")}, {"B", L("close", "chiudi")}, {"LB RB", L("tabs", "schede")}}}) {
        const float kw = TextWidth(hint_size, key) + 12.0f * scale;
        dl->AddRectFilled({fx, fy - 3.0f * scale}, {fx + kw, fy + hint_size + 3.0f * scale}, kPannello2,
                          4.0f * scale);
        Text(dl, {fx + 6.0f * scale, fy}, hint_size, kGesso, key);
        fx += kw + 8.0f * scale;
        Text(dl, {fx, fy}, hint_size, kGesso3, what);
        fx += TextWidth(hint_size, what) + 22.0f * scale;
    }
}

} // namespace

// ------------------------------------------------------------------------------------------

bool IsOpen() noexcept {
    return g_open.load();
}

bool HandleSdlEvent(const SDL_Event& event) {
    switch (event.type) {
    case SDL_EVENT_GAMEPAD_ADDED:
        OpenPad(event.gdevice.which);
        return false;
    case SDL_EVENT_GAMEPAD_REMOVED: {
        auto it = std::find_if(g_pads.begin(), g_pads.end(),
                               [&](const PadState& p) { return p.id == event.gdevice.which; });
        if (it != g_pads.end()) {
            if (it->pad) {
                SDL_CloseGamepad(it->pad);
            }
            g_pads.erase(it);
        }
        return false;
    }
    case SDL_EVENT_GAMEPAD_BUTTON_DOWN:
    case SDL_EVENT_GAMEPAD_BUTTON_UP:
        OnGamepadButton(event.gbutton, event.type == SDL_EVENT_GAMEPAD_BUTTON_DOWN);
        return false;
    case SDL_EVENT_GAMEPAD_AXIS_MOTION:
        OnGamepadAxis(event.gaxis);
        return false;
    case SDL_EVENT_KEY_DOWN:
    case SDL_EVENT_KEY_UP: {
        const bool down = event.type == SDL_EVENT_KEY_DOWN;
        if (event.key.scancode == SDL_SCANCODE_F1) {
            if (down && !event.key.repeat) {
                Push(Nav::Toggle);
            }
            return true;
        }
        if (!g_open.load()) {
            return false;
        }
        if (down) {
            switch (event.key.scancode) {
            case SDL_SCANCODE_UP:
            case SDL_SCANCODE_KP_8:
                Push(Nav::Up);
                break;
            case SDL_SCANCODE_DOWN:
            case SDL_SCANCODE_KP_2:
                Push(Nav::Down);
                break;
            case SDL_SCANCODE_LEFT:
            case SDL_SCANCODE_KP_4:
                Push(Nav::Left);
                break;
            case SDL_SCANCODE_RIGHT:
            case SDL_SCANCODE_KP_6:
                Push(Nav::Right);
                break;
            case SDL_SCANCODE_RETURN:
            case SDL_SCANCODE_KP_ENTER:
            case SDL_SCANCODE_SPACE:
                Push(Nav::Confirm);
                break;
            case SDL_SCANCODE_ESCAPE:
            case SDL_SCANCODE_BACKSPACE:
                Push(Nav::Back);
                break;
            case SDL_SCANCODE_Q:
            case SDL_SCANCODE_PAGEUP:
                Push(Nav::PrevTab);
                break;
            case SDL_SCANCODE_E:
            case SDL_SCANCODE_TAB:
            case SDL_SCANCODE_PAGEDOWN:
                Push(Nav::NextTab);
                break;
            default:
                break;
            }
        }
        return true; // the game never sees keys while the menu is open
    }
    default:
        return false;
    }
}

void RecordVulkan(const VulkanTarget& t) {
    if (g_r.failed || t.image == VK_NULL_HANDLE || t.extent.width == 0 || t.extent.height == 0) {
        return;
    }
    if (g_r.initialized) {
        ImGui::SetCurrentContext(g_r.context);
        PumpInput();
        UpdateStats();
    } else {
        // Toggle requests before the first frame still count.
        std::scoped_lock lock{g_input_mutex};
        for (const Nav nav : g_nav_queue) {
            if (nav == Nav::Toggle) {
                g_open.store(!g_open.load());
                g_input_captured.store(g_open.load());
            }
        }
        g_nav_queue.clear();
    }

    const bool hud = g_hud_mode.load() != static_cast<int>(HudMode::Off);
    const bool toast = std::chrono::duration<float>(Clock::now() - g_r.start).count() < 7.0f ||
                       !g_r.initialized;
    if (!g_open.load() && !hud && !toast) {
        ++g_r.frame;
        CollectGarbage(false);
        return;
    }

    if (!g_r.initialized) {
        if (!InitRenderer(t)) {
            LOG_ERROR(Render_Vulkan, "Deluxe8 overlay disabled: Vulkan initialisation failed");
            g_r.failed = true;
            return;
        }
        PumpInput();
        UpdateStats();
    }
    HandleFormatChange(t);
    ImageTarget* target = TargetFor(t);
    if (target == nullptr) {
        return;
    }

    const auto now = Clock::now();
    ImGuiIO& io = ImGui::GetIO();
    io.DisplaySize = ImVec2(static_cast<float>(t.extent.width), static_cast<float>(t.extent.height));
    io.DeltaTime = std::max(1e-4f, std::chrono::duration<float>(now - g_r.last_frame).count());
    g_r.last_frame = now;

    ImGui_ImplVulkan_NewFrame();
    ImGui::NewFrame();
    ImDrawList* dl = ImGui::GetForegroundDrawList();
    const float scale = std::clamp(io.DisplaySize.y / 1080.0f, 0.7f, 2.5f);
    DrawHud(dl, scale);
    DrawToast(dl, scale, io.DisplaySize);
    if (g_open.load()) {
        DrawMenu(dl, scale, io.DisplaySize);
    }
    ImGui::Render();

    const VkImageSubresourceRange range{VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    const VkImageMemoryBarrier to_color{
        .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .pNext = nullptr,
        .srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
        .dstAccessMask = VK_ACCESS_COLOR_ATTACHMENT_READ_BIT | VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
        .oldLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .newLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        .srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .image = t.image,
        .subresourceRange = range,
    };
    g_r.fn.CmdPipelineBarrier(t.cmdbuf, VK_PIPELINE_STAGE_TRANSFER_BIT,
                              VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT, 0, 0, nullptr, 0,
                              nullptr, 1, &to_color);

    const VkRenderPassBeginInfo begin{
        .sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
        .pNext = nullptr,
        .renderPass = g_r.render_pass,
        .framebuffer = target->framebuffer,
        .renderArea = {{0, 0}, t.extent},
        .clearValueCount = 0,
        .pClearValues = nullptr,
    };
    g_r.fn.CmdBeginRenderPass(t.cmdbuf, &begin, VK_SUBPASS_CONTENTS_INLINE);
    {
        // Texture uploads inside RenderDrawData submit on the shared graphics queue.
        std::unique_lock<std::mutex> lock;
        if (t.queue_mutex) {
            lock = std::unique_lock{*t.queue_mutex};
        }
        ImGui_ImplVulkan_RenderDrawData(ImGui::GetDrawData(), t.cmdbuf);
    }
    g_r.fn.CmdEndRenderPass(t.cmdbuf);

    const VkImageMemoryBarrier back_to_transfer{
        .sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .pNext = nullptr,
        .srcAccessMask = VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
        .dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT,
        .oldLayout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
        .newLayout = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .srcQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED,
        .image = t.image,
        .subresourceRange = range,
    };
    g_r.fn.CmdPipelineBarrier(t.cmdbuf, VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
                              VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0, nullptr, 1,
                              &back_to_transfer);
    ++g_r.frame;
    CollectGarbage(false);
}

void ShutdownVulkan() {
    if (!g_r.initialized) {
        return;
    }
    g_r.fn.DeviceWaitIdle(g_r.device);
    ImGui::SetCurrentContext(g_r.context);
    ImGui_ImplVulkan_Shutdown();
    RetireImages();
    if (g_r.render_pass) {
        g_r.garbage.push_back({VK_NULL_HANDLE, VK_NULL_HANDLE, g_r.render_pass, g_r.frame});
        g_r.render_pass = VK_NULL_HANDLE;
    }
    CollectGarbage(true);
    ImGui::DestroyContext(g_r.context);
    g_r = Renderer{};
}

} // namespace Deluxe8::Overlay
