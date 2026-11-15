// SPDX-License-Identifier: GPL-3.0-or-later
// NVIDIA DLSS (super resolution / DLAA + frame generation) through NGX for the Native engine, as kr_upscaler_dlss.dll.
//
// Derived from plugins/dlss/kr_upscaler_dlss.cpp of Racing 8 DX Recomp (GPL-3.0-or-later, commit 9c1a32406d2d, v0.2.4): same NGX
// calls and the same plugin ABI (kr_upscaler_abi.h). Deluxe8 changes: no C++ standard library (the llvm-mingw toolchain that ships
// with the Native engine has no libc++ headers), so the player's PC can build it without Visual Studio; Windows only.
//
// It links NVIDIA's static NGX library (downloaded by the launcher from github.com/NVIDIA/DLSS onto the player's PC), so the result
// is for that PC only and is never redistributed. Only an NVIDIA RTX GPU with a current driver can run it: `create` returns null
// elsewhere and the engine falls back to another upscaler.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#define KR_EXPORT __declspec(dllexport)

#include <vulkan/vulkan.h>

#include "nvsdk_ngx_defs.h"
#include "nvsdk_ngx_helpers.h"
#include "nvsdk_ngx_helpers_vk.h"
#include "nvsdk_ngx_helpers_dlssg_vk.h"
#include "nvsdk_ngx_params.h"
#include "nvsdk_ngx_vk.h"

#include "kr_upscaler_abi.h"

namespace {

template <typename T>
T* Allocate() { // zero-initialised: every member below defaults to zero / null
    return static_cast<T*>(calloc(1, sizeof(T)));
}

struct Dlss {
    KrUpscalerDevice dev;
    NVSDK_NGX_Parameter* caps;
    NVSDK_NGX_Parameter* params;
    NVSDK_NGX_Handle* feature;
    uint32_t rw, rh, ow, oh;
    NVSDK_NGX_PerfQuality_Value quality;
    int depth_inverted;
    PFN_vkDeviceWaitIdle wait_idle;
    PFN_vkCmdPipelineBarrier barrier;
};

bool Ok(NVSDK_NGX_Result r, const char* what) {
    if (NVSDK_NGX_FAILED(r)) {
        fprintf(stderr, "[dlss] %s failed: 0x%08x\n", what, static_cast<unsigned>(r));
        return false;
    }
    return true;
}

wchar_t g_library_dir[MAX_PATH * 4];
wchar_t g_ngx_data_dir[MAX_PATH + 32];

/// Where this library lives: NVIDIA's runtime snippets (nvngx_dlss.dll, nvngx_dlssg.dll) are looked for next to it.
const wchar_t* LibraryDir() {
    HMODULE self = nullptr;
    wcscpy(g_library_dir, L".");
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                            reinterpret_cast<LPCWSTR>(&LibraryDir), &self)) {
        return g_library_dir;
    }
    const DWORD cap = static_cast<DWORD>(sizeof(g_library_dir) / sizeof(g_library_dir[0]));
    const DWORD n = GetModuleFileNameW(self, g_library_dir, cap);
    if (n == 0 || n >= cap) {
        wcscpy(g_library_dir, L".");
        return g_library_dir;
    }
    wchar_t* slash = wcsrchr(g_library_dir, L'\\');
    if (!slash) slash = wcsrchr(g_library_dir, L'/');
    if (slash) *slash = 0;
    return g_library_dir;
}

/// NGX keeps its logs and cache here.
const wchar_t* NgxDataDir() {
    const DWORD n = GetTempPathW(MAX_PATH, g_ngx_data_dir);
    if (n == 0 || n > MAX_PATH) wcscpy(g_ngx_data_dir, L".\\");
    wcscat(g_ngx_data_dir, L"kartrecomp-ngx");
    return g_ngx_data_dir;
}

// The "models": DLSS render presets. Auto leaves the choice to NVIDIA's driver (the transformer model for DLAA/quality/balanced).
struct Preset {
    const char* name;
    unsigned value;
};
const Preset kPresets[] = {
    {"Auto (driver default)", NVSDK_NGX_DLSS_Hint_Render_Preset_Default},
    {"Preset K (transformer, best quality)", NVSDK_NGX_DLSS_Hint_Render_Preset_K},
    {"Preset J (transformer, less ghosting)", NVSDK_NGX_DLSS_Hint_Render_Preset_J},
    {"Preset M (transformer, performance)", NVSDK_NGX_DLSS_Hint_Render_Preset_M},
    {"Preset L (transformer, ultra performance)", NVSDK_NGX_DLSS_Hint_Render_Preset_L},
    {"Preset E (CNN, older)", NVSDK_NGX_DLSS_Hint_Render_Preset_E},
};
constexpr int kPresetCount = static_cast<int>(sizeof(kPresets) / sizeof(kPresets[0]));
int g_model = 0;

void ApplyPreset(NVSDK_NGX_Parameter* params) {
    if (g_model <= 0 || g_model >= kPresetCount) return;
    const unsigned v = kPresets[g_model].value;
    const char* const keys[] = {NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_DLAA, NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Quality,
                                NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Balanced, NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_Performance,
                                NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_UltraPerformance,
                                NVSDK_NGX_Parameter_DLSS_Hint_Render_Preset_UltraQuality};
    for (const char* key : keys) {
        NVSDK_NGX_Parameter_SetUI(params, key, v);
    }
}

// One NGX session per process, shared by the upscaler and the frame-generation backend of this library.
NVSDK_NGX_Parameter* g_caps = nullptr;
int g_ngx_refs = 0;

bool NgxAcquire(const KrUpscalerDevice* device, NVSDK_NGX_Parameter** caps) {
    if (g_ngx_refs == 0) {
        const wchar_t* paths[] = {LibraryDir()};
        NVSDK_NGX_FeatureCommonInfo info{};
        info.PathListInfo.Path = paths;
        info.PathListInfo.Length = 1;
        NVSDK_NGX_Result r = NVSDK_NGX_VULKAN_Init_with_ProjectID("7c1f0f5e-6e3c-4a3b-9b5a-6d2a64f3a7d1", NVSDK_NGX_ENGINE_TYPE_CUSTOM, "1.0",
                                                               NgxDataDir(), device->instance, device->physical, device->device,
                                                               device->get_instance_proc_addr, device->get_device_proc_addr, &info,
                                                               NVSDK_NGX_Version_API);
        if (!Ok(r, "NVSDK_NGX_VULKAN_Init_with_ProjectID") || !Ok(NVSDK_NGX_VULKAN_GetCapabilityParameters(&g_caps), "GetCapabilityParameters")) {
            return false;
        }
    }
    ++g_ngx_refs;
    *caps = g_caps;
    return true;
}

void NgxRelease(VkDevice device) {
    if (g_ngx_refs > 0 && --g_ngx_refs == 0) {
        NVSDK_NGX_VULKAN_Shutdown1(device);
        g_caps = nullptr;
    }
}

NVSDK_NGX_PerfQuality_Value QualityFor(uint32_t render_w, uint32_t out_w) {
    const double ratio = out_w ? static_cast<double>(render_w) / out_w : 1.0;
    if (ratio >= 0.95) return NVSDK_NGX_PerfQuality_Value_DLAA;
    if (ratio >= 0.62) return NVSDK_NGX_PerfQuality_Value_MaxQuality;   // ~0.667
    if (ratio >= 0.54) return NVSDK_NGX_PerfQuality_Value_Balanced;     // ~0.58
    if (ratio >= 0.42) return NVSDK_NGX_PerfQuality_Value_MaxPerf;      // 0.5
    return NVSDK_NGX_PerfQuality_Value_UltraPerformance;                // ~0.33
}

void ReleaseFeature(Dlss* d) {
    if (d->feature) {
        if (d->wait_idle) d->wait_idle(d->dev.device);
        NVSDK_NGX_VULKAN_ReleaseFeature(d->feature);
        d->feature = nullptr;
    }
}

void* Create(const KrUpscalerDevice* device) {
    if (!device || !device->device || !device->get_device_proc_addr) return nullptr; // no GPU instance: nothing to create
    Dlss* d = Allocate<Dlss>();
    if (!d) return nullptr;
    d->dev = *device;
    d->quality = NVSDK_NGX_PerfQuality_Value_DLAA;
    d->wait_idle = reinterpret_cast<PFN_vkDeviceWaitIdle>(device->get_device_proc_addr(device->device, "vkDeviceWaitIdle"));
    d->barrier = reinterpret_cast<PFN_vkCmdPipelineBarrier>(device->get_device_proc_addr(device->device, "vkCmdPipelineBarrier"));

    if (!NgxAcquire(device, &d->caps)) {
        free(d);
        return nullptr;
    }
    int available = 0;
    NVSDK_NGX_Parameter_GetI(d->caps, NVSDK_NGX_Parameter_SuperSampling_Available, &available);
    if (!available) {
        int needs_driver = 0;
        NVSDK_NGX_Parameter_GetI(d->caps, NVSDK_NGX_Parameter_SuperSampling_NeedsUpdatedDriver, &needs_driver);
        fprintf(stderr, "[dlss] DLSS is not available on this GPU/driver%s\n", needs_driver ? " (the driver needs an update)" : "");
        NgxRelease(device->device);
        free(d);
        return nullptr;
    }
    if (!Ok(NVSDK_NGX_VULKAN_AllocateParameters(&d->params), "AllocateParameters")) {
        NgxRelease(device->device);
        free(d);
        return nullptr;
    }
    return d;
}

int Configure(void* instance, uint32_t render_w, uint32_t render_h, uint32_t out_w, uint32_t out_h, VkFormat) {
    Dlss* d = static_cast<Dlss*>(instance);
    if (d->feature && d->rw == render_w && d->rh == render_h && d->ow == out_w && d->oh == out_h) {
        return 0;
    }
    ReleaseFeature(d); // the feature is created in Dispatch (NGX records its setup into the frame's command buffer)
    d->rw = render_w;
    d->rh = render_h;
    d->ow = out_w;
    d->oh = out_h;
    d->quality = QualityFor(render_w, out_w);
    return 0;
}

NVSDK_NGX_Resource_VK ImageRes(VkImage image, VkImageView view, VkFormat format, uint32_t w, uint32_t h, bool write,
                               VkImageAspectFlags aspect = VK_IMAGE_ASPECT_COLOR_BIT) {
    const VkImageSubresourceRange range{aspect, 0, 1, 0, 1};
    return NVSDK_NGX_Create_ImageView_Resource_VK(view, image, range, format, w, h, write);
}

struct Barriered {
    VkImage image;
    VkImageAspectFlags aspect;
};

void Transition(PFN_vkCmdPipelineBarrier barrier, VkCommandBuffer cmd, const Barriered* images, uint32_t count, VkImageLayout from,
                VkImageLayout to) {
    VkImageMemoryBarrier b[4];
    if (count > 4) count = 4;
    for (uint32_t i = 0; i < count; ++i) {
        VkImageMemoryBarrier m{};
        m.sType = VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER;
        m.srcAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT | VK_ACCESS_TRANSFER_WRITE_BIT;
        m.dstAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT | VK_ACCESS_TRANSFER_READ_BIT | VK_ACCESS_TRANSFER_WRITE_BIT;
        m.oldLayout = from;
        m.newLayout = to;
        m.srcQueueFamilyIndex = m.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        m.image = images[i].image;
        m.subresourceRange = {images[i].aspect, 0, 1, 0, 1};
        b[i] = m;
    }
    barrier(cmd, VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, VK_PIPELINE_STAGE_ALL_COMMANDS_BIT, 0, 0, nullptr, 0, nullptr, count, b);
}

void Dispatch(void* instance, const KrUpscalerFrame* fr) {
    Dlss* d = static_cast<Dlss*>(instance);
    if (!d->rw || !fr->depth || !fr->motion) {
        return;
    }
    if (!d->feature) {
        ApplyPreset(d->params);
        NVSDK_NGX_DLSS_Create_Params create{};
        create.Feature.InWidth = d->rw;
        create.Feature.InHeight = d->rh;
        create.Feature.InTargetWidth = d->ow;
        create.Feature.InTargetHeight = d->oh;
        create.Feature.InPerfQualityValue = d->quality;
        // LDR colour; motion vectors at the render size and unjittered.
        d->depth_inverted = fr->depth_inverted ? 1 : 0;
        create.InFeatureCreateFlags = NVSDK_NGX_DLSS_Feature_Flags_MVLowRes | (d->depth_inverted ? NVSDK_NGX_DLSS_Feature_Flags_DepthInverted : 0);
        if (!Ok(NGX_VULKAN_CREATE_DLSS_EXT1(d->dev.device, fr->cmd, 1, 1, &d->feature, d->params, &create), "create DLSS feature")) {
            d->rw = 0; // do not retry every frame
            return;
        }
    }

    // NGX reads its inputs as sampled images: make that layout explicit, and give everything back in GENERAL afterwards.
    const Barriered inputs[] = {{fr->color, VK_IMAGE_ASPECT_COLOR_BIT}, {fr->depth, VK_IMAGE_ASPECT_DEPTH_BIT}, {fr->motion, VK_IMAGE_ASPECT_COLOR_BIT}};
    Transition(d->barrier, fr->cmd, inputs, 3, VK_IMAGE_LAYOUT_GENERAL, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);

    NVSDK_NGX_Resource_VK color = ImageRes(fr->color, fr->color_view, fr->color_format, fr->render_width, fr->render_height, false);
    NVSDK_NGX_Resource_VK depth = ImageRes(fr->depth, fr->depth_view, fr->depth_format, fr->render_width, fr->render_height, false,
                                           VK_IMAGE_ASPECT_DEPTH_BIT);
    NVSDK_NGX_Resource_VK motion = ImageRes(fr->motion, fr->motion_view, VK_FORMAT_R16G16_SFLOAT, fr->render_width, fr->render_height, false);
    NVSDK_NGX_Resource_VK output = ImageRes(fr->output, fr->output_view, fr->color_format, fr->output_width, fr->output_height, true);

    NVSDK_NGX_VK_DLSS_Eval_Params eval{};
    eval.Feature.pInColor = &color;
    eval.Feature.pInOutput = &output;
    eval.Feature.InSharpness = 0.0f; // the host sharpens after the upscaler
    eval.pInDepth = &depth;
    eval.pInMotionVectors = &motion;
    eval.InJitterOffsetX = fr->jitter_x;
    eval.InJitterOffsetY = fr->jitter_y;
    eval.InRenderSubrectDimensions = {fr->render_width, fr->render_height};
    eval.InReset = fr->reset ? 1 : 0;
    eval.InMVScaleX = 1.0f;
    eval.InMVScaleY = 1.0f;
    eval.InPreExposure = fr->exposure > 0.0f ? fr->exposure : 1.0f;
    eval.InExposureScale = 1.0f;
    eval.InFrameTimeDeltaInMsec = fr->delta_time_ms > 0.0f ? fr->delta_time_ms : 16.6f;
    Ok(NGX_VULKAN_EVALUATE_DLSS_EXT(fr->cmd, d->feature, d->params, &eval), "evaluate DLSS");

    Transition(d->barrier, fr->cmd, inputs, 3, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL);
}

void Destroy(void* instance) {
    Dlss* d = static_cast<Dlss*>(instance);
    ReleaseFeature(d);
    if (d->params) NVSDK_NGX_VULKAN_DestroyParameters(d->params);
    NgxRelease(d->dev.device);
    free(d);
}

const KrUpscalerApi kApi = {KR_UPSCALER_ABI_VERSION, "dlss", &Create, &Configure, &Dispatch, &Destroy};

} // namespace

// Optional model choice (see UpscalerHost in the engine): applies to the next create.
extern "C" KR_EXPORT void kr_upscaler_set_model(int index) {
    g_model = index;
}
extern "C" KR_EXPORT int kr_upscaler_models(void*, const char** names, int capacity) {
    const int n = kPresetCount < capacity ? kPresetCount : capacity;
    for (int i = 0; i < n; ++i) names[i] = kPresets[i].name;
    return n;
}
extern "C" KR_EXPORT const char* kr_upscaler_active_model(void*) {
    return g_model > 0 && g_model < kPresetCount ? kPresets[g_model].name : "DLSS, driver-chosen preset";
}

extern "C" KR_EXPORT const KrUpscalerApi* kr_upscaler_get_api(void) {
    return &kApi;
}

// ---------------------------------------------------------------------------------------------------------- frame generation
// DLSS frame generation through NGX (RTX 40 and newer). NGX keeps the previous real frame itself, so every game frame is fed
// through `dispatch`, which asks for the frame halfway to the next one. Tested on an RTX 5070 Ti (Deluxe8, 2026-11-15).
namespace {

struct DlssG {
    KrUpscalerDevice dev;
    NVSDK_NGX_Parameter* caps;
    NVSDK_NGX_Parameter* params;
    NVSDK_NGX_Handle* feature;
    uint32_t rw, rh, ow, oh;
    VkFormat format;
    PFN_vkDeviceWaitIdle wait_idle;
    PFN_vkCmdPipelineBarrier barrier;
};

// Row-major 4x4 inverse (cofactor expansion); returns false when singular.
bool Invert4(const float m[16], float out[16]) {
    float inv[16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15] + m[9] * m[7] * m[14] + m[13] * m[6] * m[11] - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15] - m[8] * m[7] * m[14] - m[12] * m[6] * m[11] + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15] + m[8] * m[7] * m[13] + m[12] * m[5] * m[11] - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14] - m[8] * m[6] * m[13] - m[12] * m[5] * m[10] + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15] - m[9] * m[3] * m[14] - m[13] * m[2] * m[11] + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15] + m[8] * m[3] * m[14] + m[12] * m[2] * m[11] - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15] - m[8] * m[3] * m[13] - m[12] * m[1] * m[11] + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14] + m[8] * m[2] * m[13] + m[12] * m[1] * m[10] - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15] + m[5] * m[3] * m[14] + m[13] * m[2] * m[7] - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15] - m[4] * m[3] * m[14] - m[12] * m[2] * m[7] + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15] + m[4] * m[3] * m[13] + m[12] * m[1] * m[7] - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14] - m[4] * m[2] * m[13] - m[12] * m[1] * m[6] + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11] - m[5] * m[3] * m[10] - m[9] * m[2] * m[7] + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11] + m[4] * m[3] * m[10] + m[8] * m[2] * m[7] - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11] - m[4] * m[3] * m[9] - m[8] * m[1] * m[7] + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10] + m[4] * m[2] * m[9] + m[8] * m[1] * m[6] - m[8] * m[2] * m[5];
    const float det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if (fabsf(det) < 1e-20f) return false;
    for (int i = 0; i < 16; ++i) out[i] = inv[i] / det;
    return true;
}

void Mul4(const float a[16], const float b[16], float out[16]) { // out = a * b
    for (int r = 0; r < 4; ++r)
        for (int c = 0; c < 4; ++c) {
            float v = 0;
            for (int k = 0; k < 4; ++k) v += a[r * 4 + k] * b[k * 4 + c];
            out[r * 4 + c] = v;
        }
}

void* GCreate(const KrUpscalerDevice* device) {
    if (!device || !device->device || !device->get_device_proc_addr) return nullptr;
    DlssG* d = Allocate<DlssG>();
    if (!d) return nullptr;
    d->dev = *device;
    d->wait_idle = reinterpret_cast<PFN_vkDeviceWaitIdle>(device->get_device_proc_addr(device->device, "vkDeviceWaitIdle"));
    d->barrier = reinterpret_cast<PFN_vkCmdPipelineBarrier>(device->get_device_proc_addr(device->device, "vkCmdPipelineBarrier"));
    if (!NgxAcquire(device, &d->caps)) {
        free(d);
        return nullptr;
    }
    int available = 0;
    NVSDK_NGX_Parameter_GetI(d->caps, NVSDK_NGX_Parameter_FrameGeneration_Available, &available);
    if (!available) {
        fprintf(stderr, "[dlss] DLSS frame generation is not available on this GPU/driver\n");
        NgxRelease(device->device);
        free(d);
        return nullptr;
    }
    if (!Ok(NVSDK_NGX_VULKAN_AllocateParameters(&d->params), "AllocateParameters (DLSS-G)")) {
        NgxRelease(device->device);
        free(d);
        return nullptr;
    }
    return d;
}

int GConfigure(void* instance, uint32_t render_w, uint32_t render_h, uint32_t out_w, uint32_t out_h, VkFormat format) {
    DlssG* d = static_cast<DlssG*>(instance);
    if (d->feature && d->rw == render_w && d->rh == render_h && d->ow == out_w && d->oh == out_h && d->format == format) {
        return 0;
    }
    if (d->feature) {
        if (d->wait_idle) d->wait_idle(d->dev.device);
        NVSDK_NGX_VULKAN_ReleaseFeature(d->feature);
        d->feature = nullptr;
    }
    d->rw = render_w;
    d->rh = render_h;
    d->ow = out_w;
    d->oh = out_h;
    d->format = format;
    return 0; // the feature is created in dispatch, on the frame's command buffer
}

int GDispatch(void* instance, const KrFrameGenFrame* fr) {
    DlssG* d = static_cast<DlssG*>(instance);
    if (!d->rw || !fr->motion || !fr->depth) {
        return 1;
    }
    if (!d->feature) {
        NVSDK_NGX_DLSSG_Create_Params create{};
        create.Width = d->ow;
        create.Height = d->oh;
        create.NativeBackbufferFormat = static_cast<unsigned>(d->format);
        create.RenderWidth = d->rw;
        create.RenderHeight = d->rh;
        create.DynamicResolutionScaling = false;
        if (!Ok(NGX_VK_CREATE_DLSSG(fr->cmd, 1, 1, &d->feature, d->params, &create), "create DLSS-G feature")) {
            d->rw = 0; // do not retry every frame
            return 1;
        }
    }

    const Barriered inputs[] = {
        {fr->cur_color, VK_IMAGE_ASPECT_COLOR_BIT}, {fr->depth, VK_IMAGE_ASPECT_DEPTH_BIT}, {fr->motion, VK_IMAGE_ASPECT_COLOR_BIT}};
    Transition(d->barrier, fr->cmd, inputs, 3, VK_IMAGE_LAYOUT_GENERAL, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);

    NVSDK_NGX_Resource_VK backbuffer = ImageRes(fr->cur_color, fr->cur_color_view, fr->color_format, fr->output_width, fr->output_height, false);
    NVSDK_NGX_Resource_VK depth = ImageRes(fr->depth, fr->depth_view, fr->depth_format, fr->render_width, fr->render_height, false,
                                           VK_IMAGE_ASPECT_DEPTH_BIT);
    NVSDK_NGX_Resource_VK motion = ImageRes(fr->motion, fr->motion_view, VK_FORMAT_R16G16_SFLOAT, fr->render_width, fr->render_height, false);
    NVSDK_NGX_Resource_VK out = ImageRes(fr->output, fr->output_view, VK_FORMAT_R16G16B16A16_SFLOAT, fr->output_width, fr->output_height, true);

    NVSDK_NGX_VK_DLSSG_Eval_Params eval{};
    eval.pBackbuffer = &backbuffer;
    eval.pDepth = &depth;
    eval.pMVecs = &motion;
    eval.pOutputInterpFrame = &out;

    NVSDK_NGX_DLSSG_Opt_Eval_Params opt{};
    opt.multiFrameCount = 1;
    opt.multiFrameIndex = 1;
    float inv_vp[16], clip_to_prev[16], prev_to_clip[16], inv_prev[16];
    if (Invert4(fr->view_proj, inv_vp) && Invert4(fr->prev_view_proj, inv_prev)) {
        Mul4(fr->prev_view_proj, inv_vp, clip_to_prev);
        Mul4(fr->view_proj, inv_prev, prev_to_clip);
        memcpy(opt.clipToPrevClip, clip_to_prev, sizeof(clip_to_prev));
        memcpy(opt.prevClipToClip, prev_to_clip, sizeof(prev_to_clip));
    }
    opt.jitterOffset[0] = 0.0f; // jitter is already out of the colour and the motion vectors
    opt.jitterOffset[1] = 0.0f;
    opt.mvecScale[0] = d->rw ? 1.0f / d->rw : 1.0f; // render pixels -> [-1,1]-ish
    opt.mvecScale[1] = d->rh ? 1.0f / d->rh : 1.0f;
    opt.cameraNear = fr->camera_near;
    opt.cameraFar = fr->camera_far;
    opt.cameraFOV = fr->camera_fov_y;
    opt.cameraAspectRatio = d->oh ? static_cast<float>(d->ow) / d->oh : 16.0f / 9.0f;
    opt.colorBuffersHDR = d->format == VK_FORMAT_R16G16B16A16_SFLOAT || d->format == VK_FORMAT_A2B10G10R10_UNORM_PACK32;
    opt.depthInverted = fr->depth_inverted != 0;
    opt.cameraMotionIncluded = true;
    opt.reset = fr->reset != 0;
    opt.motionVectorsInvalidValue = 0.0f;
    opt.motionVectorsDilated = false;
    opt.menuDetectionEnabled = false;
    const bool ok = Ok(NGX_VK_EVALUATE_DLSSG(fr->cmd, d->feature, d->params, &eval, &opt), "evaluate DLSS-G");

    Transition(d->barrier, fr->cmd, inputs, 3, VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL, VK_IMAGE_LAYOUT_GENERAL);
    return ok ? 0 : 1;
}

void GDestroy(void* instance) {
    DlssG* d = static_cast<DlssG*>(instance);
    GConfigure(d, 0, 0, 0, 0, VK_FORMAT_UNDEFINED);
    if (d->params) NVSDK_NGX_VULKAN_DestroyParameters(d->params);
    NgxRelease(d->dev.device);
    free(d);
}

const KrFrameGenApi kGApi = {KR_FRAMEGEN_ABI_VERSION, "dlss-g", &GCreate, &GConfigure, &GDispatch, &GDestroy};

} // namespace

extern "C" KR_EXPORT const KrFrameGenApi* kr_framegen_get_api(void) {
    return &kGApi;
}
