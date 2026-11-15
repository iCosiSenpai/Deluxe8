# Architecture

Deluxe8 is a launcher with three engines for the player's own copy of Mario Kart 8 Deluxe (base game, update 4.0.1, Booster
Course Pass). Nothing from Nintendo is shipped or downloaded: the player points the launcher at their own game files, keys
and firmware.

```
apps/launcher (Tauri 2: Rust backend + Svelte 5 UI)
  ├─ setup wizard ─ crates/nx: NSZ -> NSP (SHA-256 per NCA), keys / firmware checks
  ├─ preparation ─ content, keys, firmware, engine configuration
  │                 Native: download engine (GitHub) -> kartrecomp-setup (ARM64 -> C -> x86-64) -> DLSS (NVIDIA GitHub)
  ├─ Emulator engine deluxe8-engine = Eden (GPL-3.0) @ engine/upstream.json + engine/patches + engine/src/deluxe8
  ├─ Native engine  mk8d-engine = Racing 8 DX Recomp (GPL-3.0) release, plugins (FSR 3.1, FSR 4, DLSS built on the PC)
  └─ Online engine  Ryujinx Nextendo (MIT) release (downloaded): the Nextendo Network
```

The Emulator engine is called "Compat" in the code (`EngineKind::Compat`, `compat` in settings and paths); players see
"Emulator". The interface is English with an Italian dictionary (`src/lib/i18n/it/*`, `tr('English text')`); the
backend's own messages use `tr!` and `src-tauri/src/i18n_table.rs`; the overlay gets `DELUXE8_LANGUAGE`.

## Engines

| | Emulator (`compat`) | Native |
|---|---|---|
| What | Eden, the Switch emulator, tailored by our patch queue | static recompilation of the game's ARM64 code to x86-64 |
| Comes from | built by `tools/engine.ps1` (bundled with the launcher) | Racing 8 DX Recomp v0.2.4 release (downloaded) or `tools/native.ps1` |
| Game files | base NSP on the command line, update + DLC through `DELUXE8_CONTENT_DIRS` | `--content-base/--content-update/--content-dlc`, recompiled modules in `--module-dir` |
| Graphics | resolution scale, FSR 1 / SGSR / ScaleForce, SMAA, LSFG frame generation | 720p-4K output, render scale, FSR 1, FSR 3.1, DLSS / DLAA, FSR 3 / DLSS / built-in frame generation, HDR10 |
| Online | rooms (local wireless over the internet) | not available |
| Menu in game | Deluxe8 overlay (ImGui), L3+R3 or F1 | the engine's own menu, F10 or L3+R3 |
| Telemetry | `D8> {json}` lines on stdout | `PERF game_fps=...` lines in its log file, read by the launcher |

## Data folder

`%LOCALAPPDATA%\Deluxe8\data` on Windows (the per-user installer puts the program itself in `%LOCALAPPDATA%\Deluxe8`, and
uninstalling leaves the data folder alone), `$XDG_DATA_HOME/deluxe8` on Linux, or `DELUXE8_DATA_DIR`.

| Path | Contents |
|---|---|
| `settings.json` | launcher settings |
| `content/` | converted NSPs (`base/`, `updates/`) and `manifest.json`; packages an earlier preparation left and the new one does not use are removed (the Compat engine loads every package in `updates/`) |
| `compat/` | Compat engine user folder (`DELUXE8_USER_DIR`): keys, firmware, NAND, saves, mods, config |
| `native/{data,cache,config}/kartrecomp` | Native engine data (XDG roots): hard links to the NSPs, keys, firmware, recompiled modules, saves, plugins built on the PC, `kartrecomp_overlay.ini` |
| `engines/native/v<version>` | Native engine downloaded by the launcher |
| `engines/online/v<version>` | Ryujinx Nextendo downloaded by the launcher |
| `online/` | Ryujinx Nextendo's data (`-r`): keys, firmware links, `games/<id>/updates.json` + `dlc.json`, saves, the Nextendo account file (never read by Deluxe8) |
| `downloads/` | archives and SDK files kept for rebuilds |
| `logs/` | `launcher.log`, `engine.log` |

The NSPs exist once: the Native engine's content folder holds hard links to them (a copy only across volumes). The Native
setup tool moves the files it is given, so the launcher only ever hands it paths that are already in its own folder.

## Launcher ↔ engine contract

Compat: `DELUXE8_USER_DIR`, `DELUXE8_CONTENT_DIRS`, `DELUXE8_TELEMETRY=1`, `DELUXE8_OVERLAY_FONT`, `DELUXE8_GAME_VERSION`,
`DELUXE8_PLAYERS`; events on stdout `D8> {json}`: `hello`, `running`, `perf` (fps, frametime_ms, speed), `overlay` (open),
`setting` (key, value), `controller`, `room`. eden-cli keeps a redirected stdout (patch 0009) and registers the NSP it boots so
updates from the content folders patch its RomFS (patch 0008).

Native: `KARTRECOMP_APP_NAME=kartrecomp`, `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_CONFIG_HOME` = `<data>/native/...`,
`KARTRECOMP_NATIVE_GPU=native`, `KARTRECOMP_NATIVE_HEIGHT`, `SUYU_CMD_PERF_SAMPLE=1`; the launcher's graphics choices go into
`kartrecomp_overlay.ini` (row indexes: `quality` 0 off, 1 100%, 2 67%, 3 58%, 4 50%, 5 33%, 6 150%, 7 200%; `hdr_paper` and
`hdr_peak` are option indexes, not nits; `speed_percent` 0 = off). Modules are rebuilt when the engine's `ENGINE_VERSION`, the
update or the build patches (`module/deluxe8.json`) change.

## Components downloaded by the launcher

Pinned in `apps/launcher/src-tauri/src/components.rs` by URL, size and SHA-256; a `.part` file is renamed only after the
check.

| Component | Source | Size | Where it goes |
|---|---|---|---|
| Native engine | `github.com/galarianpikachu2021-hue/Racing-8-DX-Recomp/releases/download/v0.2.4/KartRecomp-windows-x64.zip` | 213 MB | `engines/native/v0.2.4` |
| Ryujinx Nextendo 1.10.6 (Online engine) | `git.nextendo.network/NextendoNetwork/Ryujinx-Nextendo/releases/download/v1.10.6/` (Windows zip, Linux tar.gz) | 42 / 40 MB | `engines/online/v1.10.6` |
| DLSS SDK 310.9.16 (12 headers, `nvsdk_ngx_d.lib`, `nvngx_dlss.dll`, `nvngx_dlssg.dll`, licence) | `raw.githubusercontent.com/NVIDIA/DLSS/9981c635.../` | 71 MB | `downloads/dlss-sdk-310.9.16` |
| Vulkan headers 1.4.363 | `raw.githubusercontent.com/KhronosGroup/Vulkan-Headers/6802bb47.../` | 1.5 MB | `downloads/vulkan-headers-1.4.363` |

The DLSS plugin is compiled from `apps/launcher/src-tauri/resources/dlss-plugin` with the Native engine's llvm-mingw
toolchain and installed with NVIDIA's runtime DLLs into `native/data/kartrecomp/plugins`. NVIDIA's files never leave the
player's PC and are never part of a Deluxe8 release.

## Patches for game behaviour

- **60 fps split screen (3-4 players), update 4.0.1**: `main+0xafece8` `b.lo` → `b` (found by Racing 8 DX Recomp,
  `docs/FRAMERATE.md`). Compat: the launcher writes an IPS for build ID `158C0C7399AABD7BF83795C2EF3484C7` into the mods
  folder (`Deluxe8 60fps split-screen`); Native: `kartrecomp-setup --patch split60` at build time. Loading verified; the
  effect in a 3-4 player race not yet measured.
