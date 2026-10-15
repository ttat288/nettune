# NetTune — agent notes

Linux (Ubuntu/NetworkManager) network diagnostic app. Tauri 2 + React/TS frontend in `src/`, Rust backend in `src-tauri/`.

## Workflow

- Small verifiable milestones; each ends with a command that proves it (`cargo test`, `npm run check`, `npm run tauri build`).
- Concise progress notes. Concise output never means less testing.
- Do not repeat research already captured in README "Linux notes".
- No fabricated data: every number shown in the UI must come from a real measurement; unavailable ≠ 0.

## Layout

- `src-tauri/src/detect.rs` — interface detection (ip -j, /sys, nmcli)
- `src-tauri/src/probe.rs` — ping / DNS / TCP measurements
- `src-tauri/src/rules.rs` — deterministic diagnosis + recommendations (pure, unit tested)
- `src-tauri/src/compare.rs` — before/after comparison (pure, unit tested)
- `src-tauri/src/powersave.rs` — the only system change: NM Wi-Fi power-save, with backup/rollback
- `src/` — React UI; follow `.claude/skills/nettune-design/SKILL.md`

## Commands

- `npm run check` — tsc + cargo fmt --check + clippy + cargo test
- `npm run tauri dev` / `npm run tauri build`

## Skills

- `nettune-design` (project design system) overrides `frontend-ui-ux` defaults.
- `caveman` is optional chat-terseness only; never applies to code, docs or commits.
