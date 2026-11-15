// SPDX-FileCopyrightText: 2026 Deluxe8 contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// In-game overlay (Dear ImGui) drawn on top of the swapchain image by the Vulkan presenter.
// Opens with L3+R3 on any controller (or F1). While open the game receives no input.

#pragma once

#include <cstdint>
#include <mutex>

#include "video_core/vulkan_common/vulkan.h"

// On Linux vulkan.h pulls in Xlib, whose Success and BadValue macros break Eden's enums (nvdata.h, Loader::ResultStatus,
// WebResult::Code) in every file that includes this header: drop them, as Eden's present/layer.cpp does.
#undef Success
#undef BadValue

union SDL_Event;

namespace Deluxe8::Overlay {

struct VulkanTarget {
    VkInstance instance = VK_NULL_HANDLE;
    PFN_vkGetInstanceProcAddr get_instance_proc_addr = nullptr;
    VkPhysicalDevice physical_device = VK_NULL_HANDLE;
    VkDevice device = VK_NULL_HANDLE;
    uint32_t queue_family = 0;
    /// Serializes queue access: the overlay may upload font textures with its own submit.
    std::mutex* queue_mutex = nullptr;

    VkCommandBuffer cmdbuf = VK_NULL_HANDLE;
    VkImage image = VK_NULL_HANDLE;   ///< current swapchain image, in TRANSFER_DST_OPTIMAL
    VkFormat format = VK_FORMAT_UNDEFINED;
    VkExtent2D extent{};
    uint32_t image_count = 0;
};

/// Records the overlay into `target.cmdbuf`. The image is expected in TRANSFER_DST_OPTIMAL and
/// is left in TRANSFER_DST_OPTIMAL. Does nothing (no commands at all) when there is nothing to show.
void RecordVulkan(const VulkanTarget& target);

/// Releases every Vulkan object. Call while the device is idle, before it is destroyed.
void ShutdownVulkan();

/// Main thread: feed every SDL event. Returns true when the event was consumed by the overlay
/// (keyboard input while the menu is open); gamepad events are only observed, never consumed.
bool HandleSdlEvent(const SDL_Event& event);

[[nodiscard]] bool IsOpen() noexcept;

} // namespace Deluxe8::Overlay
