---
name: frontend-ui-ux
description: Use this skill whenever building, editing, reviewing, or critiquing ANY user-facing frontend interface — a component, page, form, modal, nav, design system, or full application — in any framework (React, Vue, Svelte, plain HTML/CSS/JS) or medium (web app, PWA, marketing site, mobile webview). Read it BEFORE writing the first line of markup or styling, before wiring up a component's states, and before declaring UI work finished. Encodes evidence-based interaction design (Nielsen's heuristics, Fitts's/Hick's/Miller's/Jakob's laws), a deliberate visual system, frontend architecture, accessibility (WCAG 2.2 AA) as a hard requirement, and Core Web Vitals performance — plus a Definition-of-Done gate to run before shipping. For Claude Artifact-specific typography/layout treatment defer to artifact-design; for chart/graph color and marks defer to dataviz — this skill is the general engineering + UX doctrine underneath both, and the default for all other frontend/UI work. Triggers on: component, page, layout, form, modal, nav, design system, CSS, styling, accessibility, a11y, responsive, UI, UX, frontend, interface, interaction design, usability, performance, Core Web Vitals, design tokens.
---

# Frontend UI/UX Master Doctrine

You are operating as a principal frontend engineer and UX researcher inside an organization that treats interface quality as measurable, not a matter of taste. Every decision — a color, a component boundary, a transition duration, a focus outline, a line of copy in an error message — must trace back to a reason: a usability heuristic, an accessibility requirement, a performance budget, an existing system convention, or an explicit instruction from whoever asked for the work. **"It looks fine to me" is never sufficient justification for shipping.**

This doctrine is deliberately broader than visual polish. Good UI is five disciplines acting as one:

1. **Interaction & information design** — the reasoning underneath the screen
2. **Visual system** — the look that expresses it
3. **Architecture** — the code that builds it
4. **Accessibility** — what makes it usable by everyone, not most people
5. **Performance** — what makes it usable at all, on a real device and a real network

Treat these as one integrated discipline. A beautiful interface that fails a keyboard-only pass is not done. A fast interface with 3:1 contrast text is not done. A perfectly accessible interface that takes eleven seconds to paint is not done.

## Read the system before you write a line

Before producing anything, spend one pass understanding what already exists. Look for: a `CLAUDE.md` or similar project doc, a design-tokens file or theme config, an existing component library (`components/ui`, a Storybook, a `package.json` dependency on shadcn/Radix/MUI/Chakra/etc.), existing lint or style-guide rules, and the visual/interaction conventions already established in neighboring code. Never introduce a second, competing pattern next to one that already exists — no hand-rolled modal next to an existing `<Dialog>`, no ad hoc spacing values next to an existing spacing scale, no new color next to an existing token system.

**Precedence, always in this order:** the explicit instructions of whoever asked for the work → the project's existing system and conventions → this doctrine's defaults. This doctrine fills gaps; it never overrides an explicit choice already made.

If the task is copy-editing an existing, working interface, match its idiom. If the task is greenfield, everything below is your default.

---

## Pillar 1 — Interaction & information design

This is the part most frontend work skips, and the part that determines whether the result is merely pretty or actually usable. Apply these actively, as implementation rules, not trivia:

**Nielsen's 10 usability heuristics**, translated into what to actually build:

| Heuristic | Build this |
|---|---|
| Visibility of system status | Every action gets feedback within ~100ms, even if the real result takes longer (optimistic UI, a spinner, a disabled-state pulse) — silence reads as broken |
| Match between system and the real world | Labels use the user's vocabulary, not the database schema's field names or internal jargon |
| User control and freedom | Every multi-step or destructive flow has a visible way out — cancel, back, undo — not just a browser back-button escape hatch |
| Consistency and standards | Don't invent a new interaction for something the platform or your own design system already solved |
| Error prevention | Constrain bad input before submission (input types, masks, disabled invalid options) rather than only validating after |
| Recognition rather than recall | Show the available options; don't make the user remember an ID, code, or value from three screens back |
| Flexibility and efficiency of use | Power-user paths (shortcuts, bulk actions) can exist without being the only path a novice sees |
| Aesthetic and minimalist design | Every element on screen must earn its place — remove first, add reluctantly |
| Help users recognize, diagnose, and recover from errors | Error messages state what happened, why, and the specific next action — in plain language, never a bare error code |
| Help and documentation | Only needed when the interface fails to explain itself — a last resort, not a crutch for a confusing flow |

**The quantitative laws** — these have actual numbers behind them; use the numbers:

- **Fitts's Law** (Fitts, 1954): acquisition time is a function of target size and distance. Minimum touch target: **44×44pt** (Apple HIG) / **48×48dp** (Material). WCAG 2.2 SC 2.5.8 sets **24×24 CSS px** as the accessibility *floor* — treat 24px as the legal minimum and 44px as the real target for anything primary, frequent, or thumb-reached. Keep destructive actions both large *and* spatially far from their frequently-used, non-destructive neighbors (don't put Delete next to Save at equal size and distance).
- **Hick's Law** (Hick 1952 / Hyman 1953): decision time grows with the number and complexity of choices. Primary navigation stays within **~7 items**. Prefer progressive disclosure over showing every option up front. If more than one element on a screen visually competes to be "the" primary action, that's a Hick's Law violation — fix it.
- **Miller's Law** (Miller, 1956, 7±2): working memory holds a small number of chunks. Group related fields into clusters of 3–5 rather than flat 12-field forms; this is the actual justification for step-wizards over monolithic forms.
- **Jakob's Law**: users spend most of their time on other products, so they expect yours to behave the same way. Match ecosystem conventions (hamburger = menu, trash icon = delete, pull-to-refresh, swipe-to-dismiss, ⌘K = command palette) unless there is a specific, stated reason to diverge.
- **Peak–End Rule**: an experience is judged by its most intense moment and its ending, not its average. Put disproportionate craft into error states, empty states, and the final confirmation/success moment — that's what gets remembered.

**Cognitive load budgeting**: every screen has exactly one job. Anything not serving that job — a stray option, unrelated content, decorative motion — is a tax on the one thing the user came to do. When in doubt, cut.


---

## Pillar 2 — The visual system

Typography, color, layout, and motion are not decoration bolted on afterward — they carry hierarchy and meaning, and every choice must be deliberate enough to defend out loud.

**Typography.** Set a real type scale (a ratio like 1.25, 1.333, or 1.5 — pick one and stay on it) instead of ad hoc pixel values scattered through the codebase. Choose typefaces for a reason tied to the product's actual character; don't default to Inter/Roboto/Arial/Space Grotesk just because they're the path of least resistance. Two families plus one monospace (for code/data) is usually enough. Keep body copy near 45–75 characters per line. Hold to a 4px or 8px baseline spacing unit throughout.

**Color.** Derive the palette from something real — an actual brand mark, a real material, existing brand guidelines — not an evenly-spaced generic set. Keep semantic color (success/warning/danger/info) and brand/accent color as two separate systems that never collide (a "success green" confirmation next to a "brand green" CTA reads as a bug, not a feature). Every foreground/background text pairing must clear WCAG contrast — **4.5:1** for normal text, **3:1** for large text (≥24px, or ≥19px bold) and for meaningful UI component boundaries — check this before shipping, not after a complaint. Design light and dark themes as two deliberate decisions, not an automatic hue inversion.

**Layout.** Let real content set the rhythm; resist the reflex of a symmetric three-column card grid when the content doesn't actually have three equal things to say. A layout system built on flex/grid `gap` beats manually-tuned per-element margins, which silently collapse or double. Wide content (tables, code, diagrams) gets its own `overflow-x: auto` container — the page body never scrolls sideways.

**Motion.** Motion communicates state change or spatial relationship — it is not a decoration. Typical UI transitions run **150–300ms**; larger or spatial transitions can run up to 400–500ms; longer reads as sluggish. Use an eased curve (ease-out for entrances, ease-in for exits, or a deliberate custom cubic-bezier) rather than linear. Always gate motion behind `prefers-reduced-motion`, and keep desktop-only flourishes (custom cursors, heavy parallax) off of primarily-mobile experiences where they aren't seen anyway.

**What not to ship** (the tells of default, un-considered output): gratuitous glassmorphism; a single flat shadow/opacity token applied to everything regardless of elevation; rounded corners applied uniformly with no relationship to hierarchy; a blue-to-purple gradient hero reached for as a default rather than chosen; generic filler copy that could belong to any product ("Build faster. Ship smarter."); interchangeable thin line icons with no relation to what they represent; a completely static page with no response to scroll or interaction, which reads as "template" rather than "built." The exhaustive version of this list, with a "do instead" for each entry, lives in `references/anti-patterns.md`.

---

## Pillar 3 — Architecture

**Semantic HTML first.** Reach for the native element — `button`, `nav`, `dialog`, `details`/`summary`, `table`, `label`, `fieldset` — before reaching for a `div` plus ARIA plus hand-rolled JavaScript. The platform already solved keyboard handling, focus management, and screen-reader semantics for free; a custom reimplementation has to earn back everything it gives up.

**Component boundaries.** Follow single responsibility and genuine reuse, not premature abstraction — three similar-but-not-identical usages don't yet justify a shared component. A component owns its own internal state and lifts only what its parent genuinely needs; don't lift state "just in case." Composition (children, slots, render props) beats a component with twenty boolean configuration props.

**Design tokens** (color, spacing, type scale, radius, shadow, motion duration) are the single source of truth referenced by both the design tool and the code. A hardcoded hex value or pixel number that duplicates an existing token is a bug, not a shortcut. If the project has no token system yet and you're building more than a one-off, introduce one rather than letting magic numbers accumulate.

**CSS architecture.** Pick one methodology per codebase (utility-first, CSS modules, a BEM-like convention) and stay consistent with whatever the project already uses — don't mix two systems. Prefer cascade layers or a clear specificity hierarchy over ad hoc `!important` patches. Use logical properties (`margin-inline`, `padding-block`, `inset-inline-start`) over physical ones (`margin-left`) so the system works in RTL contexts without extra effort. Watch selector specificity carefully — a type selector and an element selector fighting over the same padding/margin is a common, silent source of layout bugs.

**State completeness.** Every view that can load has a loading state (a skeleton that matches the eventual layout, not a generic spinner that causes a layout jump on resolve). Every list that can be empty has an empty state with a next action, not just "No results." Everything that can fail has an error state explaining what happened and what to do about it. Every irreversible action gets a confirmation proportional to its actual cost — a one-tap toggle doesn't need a modal; deleting an account does.

**Responsive by default.** Write mobile-first CSS: base styles target the smallest viewport, and `min-width` media queries add complexity as the viewport grows — never the reverse. Use `clamp()` for fluid type where it fits, instead of a fixed stack of breakpoint overrides. Reach for container queries when a component's layout depends on its container's size, not the viewport's.

Deeper component/state/CSS-methodology guidance lives in `references/component-architecture.md`.

---

## Pillar 4 — Accessibility (non-negotiable)

Accessibility is a hard requirement for any real interface, not an optional pass at the end. Treat **WCAG 2.2 Level AA** as the floor. The full POUR-organized checklist with concrete success criteria lives in `references/accessibility.md` — **read it before implementing any new interactive pattern** (a modal, an autocomplete, tabs, a custom dropdown, a data table with actions) rather than improvising ARIA from memory; the [ARIA Authoring Practices Guide](https://www.w3.org/WAI/ARIA/apg/) has a vetted reference implementation for nearly every common pattern.

The rules that apply to essentially everything, regardless of pattern:

- Every interactive element is reachable and operable by keyboard alone, in a logical tab order, with a **visible** focus state — never `outline: none` without an equally visible replacement.
- Every image conveying information gets real `alt` text describing its content or function, not its filename; purely decorative images get `alt=""`.
- Color is never the only signal — pair it with an icon, label, or shape, since roughly 1 in 12 men have some form of color vision deficiency and cannot rely on hue alone.
- Every input has a real, programmatically associated `<label>`. Errors are announced (via `aria-live` or explicit focus movement to the error), not conveyed by color alone. Required fields are marked in visible text, not an asterisk with no legend.
- Nothing flashes more than three times per second (seizure risk, WCAG 2.3.1); all motion respects `prefers-reduced-motion`.
- Verify by doing, not by reasoning about it: run a keyboard-only pass (physically don't touch the mouse), test at 200% and 400% browser zoom, and spot-check with a screen reader (Orca on Linux) before calling accessibility work finished.

---

## Pillar 5 — Performance (the felt experience)

Ground every performance decision in **Core Web Vitals**, measured at the 75th percentile of real page views: **LCP ≤ 2.5s**, **INP ≤ 200ms**, **CLS ≤ 0.1**. These are not arbitrary — they're the published thresholds for what users perceive as "fast" and "stable." (NetTune is a Tauri desktop app: Web Vitals do not apply literally; keep the UI responsive and the bundle small.)

- **CLS discipline** — reserve space for images, embeds, and ads with explicit `width`/`height` or `aspect-ratio` *before* they load. Never inject content above existing content after the initial render. Load custom fonts with `font-display: swap` or preload the exact weights actually used, so text doesn't invisibly reflow when the web font arrives.
- **LCP discipline** — identify the largest above-the-fold element (usually a hero image or heading) and prioritize it: preload it, don't lazy-load it, and don't hide it behind a client-side fetch that has to resolve before anything paints.
- **INP discipline** — keep the main thread free. Break up long JavaScript tasks, debounce expensive input handlers, and avoid reading and writing layout in the same frame repeatedly (layout thrashing).
- **Ship less** — code-split by route, lazy-load below-the-fold and off-screen content, serve images at the size the layout actually needs (not the source resolution), and question any new dependency that exists to do something a few lines of code could do directly.

---

## Definition of Done — the gate before you say "finished"

Run this checklist against your own work before declaring UI work complete. Every "no" is something to fix, not a note for later.

- [ ] Does every interactive element have a visible focus state and work with keyboard alone?
- [ ] Does every text/background pairing meet 4.5:1 contrast (3:1 for large text and meaningful UI boundaries)?
- [ ] Does the layout hold at 320px width and at 400% browser zoom without horizontal scroll on the page body?
- [ ] Does it work in both light and dark themes (if themable), and does it respect `prefers-reduced-motion`?
- [ ] Is there a loading, empty, and error state for everything that can load, be empty, or fail?
- [ ] Would removing any single element on screen make the result worse? (If not, remove it.)
- [ ] Does the largest above-the-fold element load fast, without causing layout shift?
- [ ] Is there a hardcoded color/spacing/size value duplicating a token that already exists in the project?
- [ ] Could you defend every visual choice — this color, this type size, this spacing, this animation duration — out loud, with a reason, to a design reviewer?
- [ ] Have you actually looked at it rendered in a browser (or run the app), not just read the source back to yourself?

---

## Reference index

- `references/accessibility.md` — the full WCAG 2.2 AA checklist organized by Perceivable/Operable/Understandable/Robust
- `references/anti-patterns.md` — the full gallery of visual and engineering anti-patterns, each with a "do instead"
- `references/component-architecture.md` — component composition, state placement, and CSS methodology guidance

Load a reference file when the task actually calls for that depth (a new interactive pattern, a performance regression, a design-system decision) — don't front-load all of them for a small, well-scoped change.
