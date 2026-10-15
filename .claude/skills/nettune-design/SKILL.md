---
name: nettune-design
description: NetTune UI design system — tokens, type, density and component rules for the Tauri/React frontend. Read before editing anything under src/ (styles.css, components). Overrides generic frontend-ui-ux defaults where they conflict.
---

# NetTune design system

NetTune is a restrained Linux desktop utility (GNOME/Adwaita-adjacent), not a website.
One window, one column, progressive disclosure. Every control must work; hide what is unsupported.

## Information layers (never mix them visually)

1. **Measurements** — raw observed numbers. Monospace, tabular numerals, unit in muted text. Unavailable is shown as `—` with a reason on hover/expansion, never as `0`.
2. **Findings** — interpretation. Severity icon-glyph + text label + 3px left border tint. Evidence list, explanation, confidence, limitation.
3. **Recommendations** — actions. Imperative title, rationale, then either an *Apply* flow (auto, reversible) or manual steps with copyable commands.

## Tokens (light / dark)

| token | light | dark |
|---|---|---|
| `--bg` | #FAFAFA | #1E1E1E |
| `--surface` | #FFFFFF | #2A2A2A |
| `--border` | #D5D5D5 | #444444 |
| `--text` | #1F1F1F | #EDEDED |
| `--muted` | #5C5C5C | #A8A8A8 |
| `--accent` | #1C6CC9 | #78AEED |
| `--ok` | #1E7A3C | #5FC47F |
| `--warn` | #8A5A00 | #E5B04A |
| `--bad` | #B3261E | #FF8A80 |

Accent only for focus, selection and the single primary button. Semantic colors always paired with a text label.

## Type and spacing

- UI: `system-ui, Cantarell, "Noto Sans", sans-serif`; values: `ui-monospace, "DejaVu Sans Mono", monospace` with `font-variant-numeric: tabular-nums`.
- Sizes 12 / 13 (body) / 15 / 18. Line-height 1.4. Sentence case, no all-caps.
- 4px grid: 4/8/12/16/24. Radius 6px controls, 8px panels. Borders, no shadows, no gradients, no decorative animation.
- Rows 28–32px. Hit targets ≥ 24px (primary button ≥ 32px tall).

## Components

- **Metric row**: muted label left, mono value right. Max ~6 metrics in the summary; details behind `<details>`.
- **Finding**: `<details>` with summary = severity label + title; body = evidence, explanation, confidence, next step.
- **Recommendation**: title, rationale; manual steps in `<pre>`; auto changes show current → proposed, side effects, an explicit confirm step inline (no modal), then Revert.
- **Buttons**: one filled primary per view; secondary outlined; destructive/system-changing uses an inline two-step confirm.

## Accessibility

Contrast ≥ 4.5:1 text, ≥ 3:1 borders/icons. 2px accent focus ring, 2px offset, never removed. Logical tab order; native elements first (`button`, `select`, `details`). `aria-live="polite"` for run status. Respect `prefers-color-scheme` and `prefers-reduced-motion`.
