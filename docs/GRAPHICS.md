# Graphics: resolution, upscaling, frame generation

Status as of November 2026. "Native" = static recompilation engine, "Emulator" (called Compat in the code) = the tailored Eden engine.

## Why the two engines differ

Temporal upscalers (DLSS Super Resolution/DLAA, FSR 2/3.1/4, XeSS) and in-engine frame generation need **per-pixel motion
vectors, depth and a jittered camera**. An emulator only sees the console GPU's command stream, so it cannot supply them:
The Emulator engine is limited to *spatial* filters, native-resolution rendering and *optical-flow* frame generation (which only needs
two finished frames). The Native engine translates the game's own graphics calls to Vulkan and reconstructs a depth-based
motion-vector pipeline with sub-pixel jitter, so temporal upscalers and FSR/DLSS frame generation work there.

## Availability

| Technology | Kind | Emulator | Native | RTX 5070 Ti |
|---|---|---|---|---|
| Internal resolution 0.5x–8x | supersampling | yes | 720p–4K output, scene 33–200% | yes |
| AMD FSR 1 (EASU + RCAS) | spatial | yes | yes | yes |
| Snapdragon GSR 1, ScaleForce, Lanczos, bicubic, area, MMPX | spatial | yes | — | yes |
| SMAA / FXAA | post AA | yes | — | yes |
| AMD FSR 3.1 upscaling | temporal | — | yes | yes (vendor-agnostic) |
| NVIDIA DLSS Super Resolution / DLAA | temporal ML | — | yes (tested, plugin built on the PC) | yes |
| AMD FSR 4 (ML) | temporal ML | — | plugin | **no**: AMD validates RDNA only; other GPUs get FSR 3.1.6 |
| AMD FSR 3 frame generation | interpolation (MV) | — | yes, 2x | yes |
| NVIDIA DLSS Frame Generation | interpolation (MV) | — | yes, 2x (tested) | yes |
| Lossless Scaling LSFG | optical flow | yes, 2x–4x (your `Lossless.dll`) | — | yes |
| NVIDIA Smooth Motion | driver FG (optical flow) | yes, via NVIDIA App | yes | yes (RTX 40/50, Vulkan) |
| HDR10 | output | — | yes | yes |
| ReShade FX post-processing | post | build option (Eden) | — | yes |

## Can FSR be tested on an RTX 5070 Ti?

Yes, except FSR 4. FSR 1, FSR 2 and FSR 3.1 (upscaling *and* frame generation) are vendor-agnostic and run on NVIDIA:
AMD lists FSR 2.3.4 / 3.1.6 for "Radeon RX 500 and above (or equivalent from other GPU vendors)". FSR 4's ML models are
validated only on Radeon RDNA GPUs; on any other GPU the SDK falls back to FSR 3.1.6. Community INT8 hacks exist but
are not something Deluxe8 ships.

## DLSS on the RTX 5070 Ti

The RTX 50 series supports the full DLSS 4.5 stack (transformer Super Resolution, DLAA, Frame Generation, Multi Frame
Generation up to 6x and Dynamic MFG). In Deluxe8 DLSS is available in the Native engine through a plugin that loads
NVIDIA's NGX runtime; the plugin is built on the player's PC and **never redistributed**.

**Tested 2026-11-15** (RTX 5070 Ti, driver 621.01, game 4.0.1, 1440p output, scripted Grand Prix race on Mario Kart
Stadium): DLSS Super Resolution at 67% (3D 1707x960 → 2560x1440, driver-chosen preset) and DLSS frame generation (2x)
both initialise and run; game speed holds 60 fps (120 presented), GPU time 1.3 ms per frame for upscaling (3.4 ms with
frame generation), no visible ghosting in motion. Upstream had never run the plugin on NVIDIA hardware. The plugin
provides 2x frame generation only (`multiFrameCount = 1`); 3x/4x come from the engine's built-in interpolator.

### How the launcher installs it (Windows, no Visual Studio)

1. Downloads, from the pinned commits and checked by size and SHA-256: NVIDIA's DLSS SDK 310.9.16 files from
   github.com/NVIDIA/DLSS (headers, `nvsdk_ngx_d.lib`, `nvngx_dlss.dll`, `nvngx_dlssg.dll`, licence; ~71 MB) and the
   Vulkan 1.4.363 headers from github.com/KhronosGroup/Vulkan-Headers.
2. Compiles `apps/launcher/src-tauri/resources/dlss-plugin/` (Racing 8's plugin rewritten without the C++ standard
   library) with the llvm-mingw toolchain that ships with the Native engine. `msvc_shim.cpp` supplies the few pieces of
   MSVC's static runtime that NVIDIA's static library expects (operator new/delete, thread-safe statics, /GS handlers).
3. Installs `kr_upscaler_dlss.dll`, `kr_framegen_dlss.dll` and NVIDIA's two runtime DLLs into
   `<data>\native\data\kartrecomp\plugins`, where the engine looks for plugins built on the PC.

The same plugin built this way and with MSVC (`tools/native.ps1 plugins`) gave identical results in the race test.

### Measuring

Screenshots taken by the engine (`KARTRECOMP_NATIVE_SHOTS`) stall the renderer for ~0.4 s each at 1440p: measure fps
without them. `SUYU_CMD_PERF_SAMPLE=1` logs `PERF game_fps=...` once per second; the launcher reads these lines for its
fps readout.

## Frame generation without integration

- **NVIDIA Smooth Motion** (RTX 40/50): driver-level frame generation that also works on Vulkan applications, enabled per
  program in the NVIDIA App. NVIDIA explicitly mentions emulators locked to fixed frame rates as a target.
- **Lossless Scaling (LSFG)**: the Emulator engine integrates Eden's port of lsfg-vk. Deluxe8 copies *your* `Lossless.dll` from your
  Steam installation into the engine's data folder; it is never bundled.

All interpolation adds latency (about half a source frame at 2x). MK8D runs its game logic at 60 Hz, so frame generation
improves smoothness on high-refresh displays but not responsiveness. The "Competitivo" preset turns it off.

## Sources

- Khronos, "NVIDIA Adds Vulkan Support to Smooth Motion Frame Generation", 2025-06-02 —
  https://www.khronos.org/news/permalink/nvidia-adds-vulkan-support-to-smooth-motion-frame-generation-a-boost-for-many-games-and-emulators
- AMD GPUOpen, FSR / FSR Redstone requirements — https://gpuopen.com/fsr/
- NVIDIA, "DLSS 4.5 Dynamic Multi Frame Generation & Multi Frame Generation 6X Available Now" —
  https://www.nvidia.com/en-us/geforce/news/dlss-4-5-dynamic-multi-frame-generation-6x-mode-released/
- Racing 8 DX Recomp (FSR 3.1, FSR 3 FG, DLSS plugins, HDR10) — https://github.com/galarianpikachu2021-hue/Racing-8-DX-Recomp
- Eden (FSR 1, SGSR, LSFG port of lsfg-vk) — https://git.eden-emu.dev/eden-emu/eden
