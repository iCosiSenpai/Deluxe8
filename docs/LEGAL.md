# Legal notes

Not legal advice. This page says what Deluxe8 is, what it ships and under which licences.

## What Deluxe8 is

Deluxe8 is a launcher plus patches to two open-source projects. It is meant for people who own a Nintendo Switch and a
copy of Mario Kart 8 Deluxe and want to play **their own copy** on their PC.

- It ships **no Nintendo code or content**: no game files, keys, firmware, system software, artwork, music or saves.
- It **never downloads** games, keys or firmware, and it never connects to Nintendo's servers. Online play uses room
  servers run by players (local wireless and LAN over the internet).
- The player supplies the game, the update and the DLC as dumped from their own console, plus the console's keys
  (`prod.keys`, `title.keys`) and firmware. Whether dumping and decrypting your own copy is allowed depends on where you
  live: check your local law.
- Issues or pull requests that link to, ask for or contain games, keys or firmware are closed.

## Licences

Deluxe8's own code (launcher, `crates/nx`, `engine/src/deluxe8`, the patches in `engine/patches`, scripts and
documentation) is licensed under the **GNU General Public License v3.0 or later** ([LICENSE](../LICENSE)).

| Component | Licence | How Deluxe8 uses it |
|---|---|---|
| [Eden](https://git.eden-emu.dev/eden-emu/eden) (yuzu / sudachi lineage) | GPL-3.0-or-later | The Compat engine shipped in the installer is Eden at the commit in [`engine/upstream.json`](../engine/upstream.json) plus `engine/patches` and `engine/src/deluxe8`; all of it is in this repository or upstream, at the tag of the release. `LICENSE-eden.txt` is installed next to the engine. |
| [Racing 8 DX Recomp](https://github.com/galarianpikachu2021-hue/Racing-8-DX-Recomp) | GPL-3.0 | The Native engine. Downloaded by the launcher from that project's GitHub releases (pinned by SHA-256), not shipped by Deluxe8. Its own third-party notices come with it. |
| [Dear ImGui](https://github.com/ocornut/imgui) | MIT | The Compat engine's in-game overlay. |
| [Archivo](https://github.com/Omnibus-Type/Archivo) by Omnibus-Type | SIL Open Font License 1.1 | The launcher's and overlay's typeface; `OFL.txt` is installed with the font. |
| [Tauri](https://tauri.app), [Svelte](https://svelte.dev) and the Rust crates and npm packages in the lock files | MIT and/or Apache-2.0 (a few BSD, ISC, Zlib, MPL-2.0) | The launcher. |
| AMD FidelityFX (FSR) | MIT | Part of the Native engine's plugins. |
| NVIDIA DLSS SDK | NVIDIA DLSS SDK licence | **Not distributed.** When the player turns DLSS on, the launcher shows the licence and downloads the SDK files from [github.com/NVIDIA/DLSS](https://github.com/NVIDIA/DLSS) straight to their PC, where the plugin is compiled. The files never leave that PC. |
| Khronos Vulkan headers | Apache-2.0 / MIT | Downloaded with the DLSS SDK to compile the plugin. |
| Lossless Scaling | proprietary | **Not distributed.** If the player owns it on Steam, the Compat engine uses their own `Lossless.dll`. |

## Trademarks

Nintendo, Nintendo Switch, Mario Kart and related names are trademarks of Nintendo. NVIDIA, RTX and DLSS are trademarks of
NVIDIA Corporation. AMD, Radeon and FidelityFX are trademarks of Advanced Micro Devices, Inc. They are used only to say
what Deluxe8 works with. Deluxe8 is an independent project, not affiliated with, endorsed or sponsored by any of them;
its name, logo and artwork are its own.
