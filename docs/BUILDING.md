# Building Deluxe8

Everything is built from source on your own PC. Build trees go to `build/`, staged binaries to `out/`, toolchains and
upstream checkouts to `.cache/` (all three are gitignored).

## Windows (x64)

Requirements: Visual Studio 2026 (or 2022) with *Desktop development with C++* and *C++ CMake tools*, Git, PowerShell 7,
Rust (stable), Node.js with pnpm. No administrator rights are needed: LLVM, CMake, glslang, Qt and the Vulkan headers are
downloaded into `.cache/`.

```powershell
pwsh tools/engine.ps1 all          # Emulator engine (Compat) -> out/engine/win-x64 (deluxe8-engine.exe, deluxe8-room.exe)
pwsh tools/native.ps1 all          # Native engine toolchain -> out/native/win-x64 (~45 min the first time)
cd apps/launcher; pnpm install; pnpm tauri dev
```

Installer with the Emulator engine inside: `pnpm tauri build --config src-tauri/tauri.bundle-windows.conf.json` (the plain
`pnpm tauri build` leaves the engine out). Signed releases: `pwsh tools/release.ps1 -Thumbprint ...` or the `release.yml`
workflow, see [RELEASING.md](RELEASING.md). Toolchain downloads (LLVM, CMake, glslang) are pinned by SHA-256 in both
scripts; bump URL and hash together.

`tools/native.ps1` is optional for players: the launcher downloads the Native engine from the Racing 8 DX Recomp release
page when it is missing (see [ARCHITECTURE.md](ARCHITECTURE.md#components-downloaded-by-the-launcher)). Its `plugins` step
also builds a DLSS plugin with MSVC into `out/native/dlss-local/` for this machine only; the launcher builds its own copy
with the engine's toolchain.

### Emulator engine (Compat): day-to-day changes

The Eden tree is `.cache/engine/eden` on branch `deluxe8`; `src/deluxe8` there is a junction to `engine/src/deluxe8`.

1. Edit, commit on `deluxe8` (one logical change per commit, message `deluxe8: ...` or `build: ...`).
2. `pwsh tools/engine.ps1 build` then `pwsh tools/engine.ps1 stage`.
3. `pwsh tools/engine.ps1 export-patches` regenerates `engine/patches/` (that is what gets committed).
4. New upstream revision: update `engine/upstream.json`, `pwsh tools/engine.ps1 patch -Force`, fix conflicts, re-export.

`tools/native.ps1 assemble` recreates `out/native/win-x64` from scratch; a `userdata` folder left there by a
`kartrecomp.exe` run is parked beside it and put back.

## Linux (x86-64)

`tools/engine.sh` mirrors `tools/engine.ps1` (`toolchain`, `fetch`, `patch`, `configure`, `build`, `stage`, `all`,
`export-patches`, `clean`) and stages `deluxe8-engine` / `deluxe8-room` into `out/engine/linux-x64`. It prefers Clang
(Eden needs Clang 19+ or GCC 16+, and Ubuntu 26.04 ships GCC 15). The release workflow runs the complete build, engine
and launcher packages, in an Ubuntu 26.04 container (Clang 21, CMake 4.2).

Packages on Ubuntu 26.04 / Debian 13 (from Eden's `docs/Deps.md`, without the Qt ones):

```sh
sudo apt-get install autoconf cmake g++ gcc clang git patch glslang-tools libglu1-mesa-dev libhidapi-dev libpulse-dev libtool \
  libudev-dev libxext-dev mesa-common-dev nasm ninja-build catch2 liblz4-dev libzstd-dev libssl-dev libavfilter-dev \
  libavcodec-dev libswscale-dev pkg-config zlib1g-dev libva-dev libvdpau-dev libvulkan-dev spirv-tools spirv-headers \
  libusb-1.0-0-dev libxbyak-dev libboost-dev libboost-fiber-dev libboost-context-dev libsdl3-dev libopus-dev \
  libasound2t64 vulkan-utility-libraries-dev
tools/engine.sh all
```

Launcher packages, as [`release.yml`](../.github/workflows/release.yml) builds them (they need `libwebkit2gtk-4.1-dev`,
`librsvg2-dev` and `dpkg-dev`), from `apps/launcher` after `pnpm install`:

- **.deb**: `pnpm tauri build --bundles deb --config src-tauri/tauri.bundle-linux.conf.json --config deb-depends.json`.
  Tauri declares only the launcher's own libraries, so the workflow writes `deb-depends.json` from what `dpkg-shlibdeps`
  says the engine needs (SDL3, FFmpeg, Boost, libusb...).
- **AppImage**: `NO_STRIP=true pnpm tauri bundle --bundles appimage --config src-tauri/tauri.bundle-appimage.conf.json`.
  The engine goes next to the launcher in `usr/bin` (the launcher looks there last), where linuxdeploy also bundles its
  libraries.

Both are experimental: they are built against Ubuntu 26.04's libraries (the .deb installs only there and on its
derivatives) and have not been tried on a Linux desktop yet. Do not run `tools/engine.sh patch` from WSL on a tree that
a Windows build also uses: it replaces the `src/deluxe8` junction with a symlink and resets the branch.

The Native engine on Linux is Racing 8 DX Recomp's AppImage; the launcher does not download it on Linux yet.

## Tests

```powershell
cd apps/launcher; pnpm check                     # svelte-check
cd apps/launcher/src-tauri; cargo clippy --all-targets; cargo test
cd crates/nx; cargo test
```

`apps/launcher/src-tauri/examples/components_check.rs` drives the launcher's Native-engine backend without the UI (download,
DLSS build, preparation, launch) against a scratch data folder:

```powershell
$env:DELUXE8_DATA_DIR = 'F:\scratch\d8'          # never your real data folder
$env:DELUXE8_NATIVE_DIR = 'F:\scratch\d8\none'   # optional: ignore other engines, force the download
cargo run --example components_check -- native dlss status
```

Engine smoke tests and their pitfalls are in [AGENT.md](../AGENT.md#testing).
