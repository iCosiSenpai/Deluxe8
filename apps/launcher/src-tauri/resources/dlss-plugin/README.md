# DLSS plugin sources (built on the player's PC)

The launcher compiles these files into the Native engine's DLSS plugin (`kr_upscaler_dlss.dll` + `kr_framegen_dlss.dll`)
after downloading NVIDIA's DLSS SDK files from github.com/NVIDIA/DLSS onto the player's PC. See `src/components.rs` and
`docs/GRAPHICS.md`.

| File | Origin |
|---|---|
| `kr_upscaler_dlss.cpp` | Racing 8 DX Recomp `plugins/dlss/kr_upscaler_dlss.cpp` (GPL-3.0-or-later, commit 9c1a32406d2d), rewritten without the C++ standard library |
| `kr_upscaler_abi.h` | Racing 8 DX Recomp `plugins/dlss/kr_upscaler_abi.h` (plugin ABI 1), `<cstdint>` → `<stdint.h>` |
| `forwarder.c` | Racing 8 DX Recomp `plugins/windows/forwarder.c` (unchanged) |
| `msvc_shim.cpp` | Deluxe8: the parts of MSVC's static runtime that NVIDIA's static NGX library expects |

Nothing from NVIDIA is in this folder or in the repository. When the Native engine is updated to a release with a new
plugin ABI, update these files from the same upstream commit.
