# Accessibility checklist — WCAG 2.2 Level AA

Read this before implementing any new interactive pattern, before a final review pass, or when someone reports an accessibility problem. Full spec: [WCAG 2.2 Quick Reference](https://www.w3.org/WAI/WCAG22/quickref/). Reference implementations for common widgets: [WAI-ARIA Authoring Practices Guide (APG)](https://www.w3.org/WAI/ARIA/apg/) — check here before hand-rolling ARIA for a combobox, tabs, a dialog, a listbox, or a data grid; nearly every common pattern already has a vetted example.

Organized by WCAG's own four principles — **P**erceivable, **O**perable, **U**nderstandable, **R**obust (POUR).

## Perceivable

- **Text alternatives (1.1.1):** every image conveying information has real `alt` text describing content or function, not the filename. Purely decorative images get `alt=""` so screen readers skip them.
- **Contrast (1.4.3, 1.4.11):** normal text ≥ **4.5:1** against its background; large text (≥24px, or ≥19px bold) and meaningful UI component boundaries/icons ≥ **3:1**. Check this at design time, not after a complaint — many color pairs that look fine to sighted designers with good vision fail this in practice.
- **Resize/reflow (1.4.4, 1.4.10):** content must remain usable at 200% zoom, and reflow to a single column at 400% zoom / 320px width without requiring horizontal scrolling of the page body.
- **Use of color (1.4.1):** color is never the only way information is conveyed — pair it with an icon, label, underline, or pattern.
- **Non-text contrast (1.4.11):** interactive component boundaries (button outlines, input borders, focus indicators) need ≥3:1 contrast against adjacent colors, same as large text.

## Operable

- **Keyboard (2.1.1, 2.1.2):** everything operable with a mouse is operable with a keyboard alone, with no keyboard trap (a focused element you can tab into but never tab back out of).
- **Focus visible (2.4.7) and focus not obscured (2.4.11, WCAG 2.2 new):** focus indicators must be visible and not hidden behind sticky headers or other overlapping content. Never ship `outline: none` without an equally or more visible custom replacement.
- **Target size (2.5.8, WCAG 2.2 new):** interactive targets are at least **24×24 CSS px**, or have sufficient spacing from neighboring targets, unless the target is inline in a sentence or there's an equivalent same-function larger target elsewhere. Treat 24px as the legal floor and 44px (Apple HIG) / 48dp (Material) as the actual target for anything primary or frequently used.
- **Seizures (2.3.1):** nothing flashes more than three times in any one-second period.
- **Motion actuation / reduced motion:** respect `prefers-reduced-motion`; don't require device-motion gestures (shake, tilt) without a non-motion alternative.
- **Dragging movements (2.5.7, WCAG 2.2 new):** any drag-based interaction (reorder, slider) needs a non-drag alternative (buttons, keyboard input).

## Understandable

- **Labels (2.5.3, 3.3.2):** every input has a real, programmatically associated `<label>` (via `for`/`id` or wrapping); placeholder text is never a substitute for a label, since it disappears on focus/input.
- **Error identification (3.3.1) and suggestion (3.3.3):** errors are described in text (not color alone), identify which field is wrong, explain what's wrong, and suggest the fix. Announce errors via `aria-live` or by moving focus to the error, not just a visual color shift the screen-reader user won't perceive.
- **Consistent navigation/identification (3.2.3, 3.2.4):** repeated UI (nav, icons, patterns) behaves and is labeled the same way across the product.
- **Required fields:** marked in visible text ("Required" or a legend explaining the asterisk), not an unexplained asterisk alone.

## Robust

- **Name, role, value (4.1.2):** every custom interactive component exposes a correct accessible name, role, and state (expanded/collapsed, checked/unchecked, selected) — usually free from a native element, requiring explicit ARIA only when building something the platform has no native equivalent for.
- **Status messages (4.1.3):** important status updates (form submitted, item added to cart, error occurred) are announced to assistive tech via `aria-live` regions without requiring a focus shift for non-critical updates.

## Verification method (do this, don't just reason about it)

1. **Keyboard-only pass** — physically avoid the mouse/trackpad; tab through the entire flow, confirm focus order is logical and every action is reachable.
2. **Zoom pass** — test at 200% and 400% browser zoom; confirm no horizontal scroll on the page body and no content clipped or overlapping.
3. **Screen reader spot-check** — on Linux use Orca (Super+Alt+S on GNOME); confirm form labels, error messages, and custom components are announced sensibly.
4. **Color-blindness simulation** — check that color is never the sole signal, especially for state (error/success/selected).
5. **`prefers-reduced-motion` pass** — enable the OS setting and confirm motion is reduced or removed, not just slowed slightly.
