# Contributing to Deluxe8

Thanks for helping. Start with [AGENT.md](../AGENT.md): it is the guide for humans and coding agents alike, with the hard
rules, the layout, how the Compat engine's patch queue works and the testing pitfalls.

## The rules that are never bent

1. **No Nintendo material** in the repository, in issues or in pull requests: no game files, keys, firmware, saves,
   extracted content or screenshots of the game. Screenshots of the launcher are welcome.
2. **No key values**, anywhere. Talk about keys by their names (`header_key`, `master_key_14`).
3. **No redistribution of NVIDIA DLSS/NGX or AMD signed libraries.** They are downloaded on the player's PC.
4. **No connection to Nintendo's servers.**

## Workflow

- Small, focused pull requests; one logical change per commit.
- Launcher: `cd apps/launcher && pnpm install && pnpm tauri dev`. The UI also runs in a plain browser with mock data
  (`pnpm dev`; add `?page=grafica&engine=native` to open a page directly).
- Compat engine: edit `.cache/engine/eden` on branch `deluxe8`, then `pwsh tools/engine.ps1 build`, `stage` and
  `export-patches`; commit the regenerated `engine/patches/*.patch` (details in [docs/BUILDING.md](../docs/BUILDING.md)).
- Before pushing: `cargo clippy --all-targets -- -D warnings` and `cargo test` in `apps/launcher/src-tauri` and
  `crates/nx`, and `pnpm check` in `apps/launcher`. CI runs the same.

## Style

- Launcher UI text is Italian, sentence case, plain verbs; everything must work with a controller and a keyboard.
- The timing-screen colours mean state (viola = recommended, verde = ready, giallo = attention, rosso = error), never
  decoration.
- Code comments and documentation are in English.

By contributing you agree that your contribution is licensed under the GPL-3.0-or-later.
