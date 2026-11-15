// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

#include "deluxe8/deluxe8.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <format>
#include <mutex>
#include <optional>

#include "common/logging.h"
#include "common/settings.h"

namespace Deluxe8 {
namespace {

std::mutex g_emit_mutex;
std::mutex g_state_mutex;
bool g_telemetry = false;
PerfSample g_perf{};
Actions g_actions{};
std::string g_game_version;
std::string g_overlay_font;
int g_players = 1;

std::optional<std::string> GetEnv(const char* name) {
#ifdef _WIN32
    // Use the wide API so non-ASCII paths survive, then convert to UTF-8.
    std::wstring wname(name, name + std::char_traits<char>::length(name));
    const wchar_t* value = _wgetenv(wname.c_str());
    if (value == nullptr || *value == L'\0') {
        return std::nullopt;
    }
    std::string out;
    for (const wchar_t* p = value; *p; ++p) {
        char32_t cp = static_cast<char32_t>(*p);
        if (cp >= 0xD800 && cp <= 0xDBFF && p[1] >= 0xDC00 && p[1] <= 0xDFFF) {
            cp = 0x10000 + ((cp - 0xD800) << 10) + (static_cast<char32_t>(p[1]) - 0xDC00);
            ++p;
        }
        if (cp < 0x80) {
            out += static_cast<char>(cp);
        } else if (cp < 0x800) {
            out += static_cast<char>(0xC0 | (cp >> 6));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        } else if (cp < 0x10000) {
            out += static_cast<char>(0xE0 | (cp >> 12));
            out += static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        } else {
            out += static_cast<char>(0xF0 | (cp >> 18));
            out += static_cast<char>(0x80 | ((cp >> 12) & 0x3F));
            out += static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        }
    }
    return out;
#else
    const char* value = std::getenv(name);
    if (value == nullptr || *value == '\0') {
        return std::nullopt;
    }
    return std::string{value};
#endif
}

std::vector<std::string> SplitPathList(std::string_view list) {
#ifdef _WIN32
    constexpr char separator = ';';
#else
    constexpr char separator = ':';
#endif
    std::vector<std::string> out;
    size_t start = 0;
    while (start <= list.size()) {
        const size_t end = list.find(separator, start);
        const std::string_view item =
            list.substr(start, end == std::string_view::npos ? std::string_view::npos : end - start);
        if (!item.empty()) {
            out.emplace_back(item);
        }
        if (end == std::string_view::npos) {
            break;
        }
        start = end + 1;
    }
    return out;
}

} // namespace

void Initialize() {
    g_telemetry = GetEnv("DELUXE8_TELEMETRY").value_or("") == "1";
    g_game_version = GetEnv("DELUXE8_GAME_VERSION").value_or("");
    g_overlay_font = GetEnv("DELUXE8_OVERLAY_FONT").value_or("");
    if (const auto players = GetEnv("DELUXE8_PLAYERS")) {
        g_players = std::clamp(std::atoi(players->c_str()), 1, 8);
    }

    if (const auto dirs = GetEnv("DELUXE8_CONTENT_DIRS")) {
        for (auto& dir : SplitPathList(*dirs)) {
            LOG_INFO(Frontend, "Deluxe8: external content directory {}", dir);
            Settings::values.external_content_dirs.push_back(std::move(dir));
        }
    }

    Emit(std::format(R"({{"t":"hello","engine":"deluxe8-compat","telemetry":{}}})",
                     g_telemetry ? "true" : "false"));
}

bool TelemetryEnabled() noexcept {
    return g_telemetry;
}

void Emit(std::string_view json) {
    if (!g_telemetry) {
        return;
    }
    std::scoped_lock lock{g_emit_mutex};
    std::fwrite("D8> ", 1, 4, stdout);
    std::fwrite(json.data(), 1, json.size(), stdout);
    std::fputc('\n', stdout);
    std::fflush(stdout);
}

std::string JsonEscape(std::string_view text) {
    std::string out;
    out.reserve(text.size() + 8);
    for (const char c : text) {
        switch (c) {
        case '"':
            out += "\\\"";
            break;
        case '\\':
            out += "\\\\";
            break;
        case '\n':
            out += "\\n";
            break;
        case '\r':
            out += "\\r";
            break;
        case '\t':
            out += "\\t";
            break;
        default:
            if (static_cast<unsigned char>(c) < 0x20) {
                out += std::format("\\u{:04x}", static_cast<unsigned>(c));
            } else {
                out += c;
            }
        }
    }
    return out;
}

void EmitMessage(std::string_view type, std::string_view text) {
    Emit(std::format(R"({{"t":"{}","msg":"{}"}})", JsonEscape(type), JsonEscape(text)));
}

void PublishPerf(const PerfSample& sample) {
    {
        std::scoped_lock lock{g_state_mutex};
        g_perf = sample;
    }
    // NaN/inf (no frames yet) is not valid JSON: report 0 instead.
    const auto finite = [](double v) { return std::isfinite(v) ? v : 0.0; };
    Emit(std::format(R"({{"t":"perf","fps":{:.2f},"frametime_ms":{:.3f},"speed":{:.1f}}})",
                     finite(sample.guest_fps), finite(sample.frametime_ms),
                     finite(sample.speed_percent)));
}

PerfSample LatestPerf() {
    std::scoped_lock lock{g_state_mutex};
    return g_perf;
}

void SetActions(Actions actions) {
    std::scoped_lock lock{g_state_mutex};
    g_actions = std::move(actions);
}

const Actions& GetActions() {
    return g_actions;
}

int Players() noexcept {
    return g_players;
}

std::string GameVersion() {
    return g_game_version;
}

std::string OverlayFontPath() {
    return g_overlay_font;
}

} // namespace Deluxe8
