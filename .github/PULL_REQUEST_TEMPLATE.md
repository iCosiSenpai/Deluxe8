## What and why

<!-- What changes, and the problem it solves. Link the issue if there is one. -->

## How it was tested

<!-- Commands run (cargo test, pnpm check, engine smoke test...), hardware for engine changes. -->

## Checklist

- [ ] No Nintendo material: no game files, keys, firmware, saves, extracted content or game screenshots (AGENT.md, rule 1).
- [ ] No key values anywhere: code, logs, tests, docs, commit messages (rule 2).
- [ ] No NVIDIA DLSS/NGX or AMD signed binaries added (rule 3).
- [ ] Engine changes: made in `.cache/engine/eden` and exported with `pwsh tools/engine.ps1 export-patches`.
- [ ] `cargo clippy --all-targets -- -D warnings`, `cargo test` and `pnpm check` pass.
