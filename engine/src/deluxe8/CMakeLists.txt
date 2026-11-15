# SPDX-FileCopyrightText: 2026 Deluxe8 contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Deluxe8 integration library: launcher handshake, telemetry and the in-game overlay.
# Linked by video_core (overlay rendering) and the SDL3 frontend (events, perf counters).

# Dear ImGui (MIT). Eden's CPMUtil pins every download with a SHA-512 of the archive.
AddPackage(
    NAME imgui
    REPO ocornut/imgui
    VERSION v1.93.0
    HASH de41ef101f37d09685bf9e69bacefe06a0c010ecd0e6e7ed1e4efbff82fde27a9c3ae36ed436db5bcde4c61de10da984dfb74d6098586a3f78d8a716864460c2
    DOWNLOAD_ONLY)

set(DELUXE8_IMGUI_SOURCES
    ${imgui_SOURCE_DIR}/imgui.cpp
    ${imgui_SOURCE_DIR}/imgui_draw.cpp
    ${imgui_SOURCE_DIR}/imgui_tables.cpp
    ${imgui_SOURCE_DIR}/imgui_widgets.cpp
    ${imgui_SOURCE_DIR}/backends/imgui_impl_vulkan.cpp)

add_library(deluxe8 STATIC
    deluxe8.cpp
    deluxe8.h
    input_capture.h
    overlay.cpp
    overlay.h
    ${DELUXE8_IMGUI_SOURCES})

target_include_directories(deluxe8 PUBLIC ${imgui_SOURCE_DIR} ${imgui_SOURCE_DIR}/backends)
target_compile_definitions(deluxe8 PUBLIC
    IMGUI_IMPL_VULKAN_NO_PROTOTYPES
    IMGUI_DISABLE_OBSOLETE_FUNCTIONS
    IMGUI_DISABLE_DEFAULT_SHELL_FUNCTIONS)
target_link_libraries(deluxe8 PUBLIC common network Vulkan::Headers PRIVATE SDL3::SDL3)

# Third-party code: keep the build log readable.
if (MSVC)
    set_source_files_properties(${DELUXE8_IMGUI_SOURCES} PROPERTIES COMPILE_OPTIONS "/w")
else()
    set_source_files_properties(${DELUXE8_IMGUI_SOURCES} PROPERTIES COMPILE_OPTIONS "-w")
endif()

# Frontend helpers for the SDL3 frontend only (controller auto-mapping). Kept apart from
# `deluxe8` because they need core/hid_core/input_common, which already depend on video_core.
add_library(deluxe8_frontend STATIC automap.cpp automap.h)
target_link_libraries(deluxe8_frontend PUBLIC deluxe8 core hid_core input_common)
