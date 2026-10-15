# Component architecture, state, and CSS methodology

Read this when making a structural decision: where a piece of state should live, whether something deserves to be its own component, or which CSS methodology to follow in a codebase that doesn't already have one established.

## Component boundaries

- **Single responsibility, not premature abstraction.** Two or three similar-looking usages don't yet justify a shared component — wait for the shape to actually repeat before extracting it. An abstraction built from two data points usually guesses wrong about what's actually variable.
- **Composition over configuration.** A component that composes via children/slots/render-props scales better than one with an ever-growing list of boolean and enum props trying to cover every case. If a component's prop list is mostly booleans that each toggle a different rendering branch, it's usually several components wearing one costume.
- **State lives at the lowest common owner.** Lift state only as high as the nearest component that actually needs to coordinate it — not to the top "just in case" a distant future feature needs it. Colocating state with the component that uses it keeps re-renders and mental overhead scoped.
- **Derived state is computed, not stored.** If a value can be calculated from existing props/state during render, don't duplicate it into its own `useState`/store field — that's a sync bug waiting to happen.
- **Container/presentational split where it earns its keep.** Separating data-fetching/business logic from rendering is useful once a component does real work on both fronts — not a mandatory pattern for every component regardless of size.

## Naming and API design

- Name components and props for what a person recognizes, not how the system is built internally — a prop called `isUrgent` communicates more than `variant="type2"`.
- A component's public API (its props) should read like a sentence describing what it does, not an implementation detail leaking out (`showModal` beats `modalVisibilityState`).
- Keep controlled/uncontrolled behavior consistent and explicit — don't silently support both without documenting which one a given prop combination produces.

## CSS methodology

Pick **one** approach per codebase and stay consistent with whatever already exists; don't introduce a second methodology alongside an established one.

- **Utility-first (e.g. Tailwind)** — fast to write, keeps styles colocated with markup, but needs discipline: extract a component/style abstraction once a class list gets long and repeats, rather than copy-pasting a 20-class string across the codebase.
- **CSS Modules / scoped styles** — good default for component-per-file frameworks; keeps specificity naturally low since class names are auto-scoped.
- **BEM or a similar naming convention** — useful in codebases without build-time scoping, since the naming convention itself prevents collisions.
- **Cascade layers (`@layer`)** — the modern answer to "reset vs. component vs. utility vs. override" specificity fights; define layer order explicitly once, and everything within a layer loses to anything in a later layer regardless of selector specificity.

Regardless of methodology: use logical properties (`margin-inline`, `padding-block`, `inset-inline-start`) over physical ones (`margin-left`, `padding-top`) so layouts work correctly in RTL locales without extra overrides. Reach for `font-variant-numeric: tabular-nums` anywhere digits need to align in a column (tables, stat displays, timers).

## Design tokens as the design/code bridge

A token system (color, spacing, type scale, radius, shadow, motion duration) is what lets a design tool and a codebase agree on the same vocabulary. When one doesn't exist yet and the task is more than a one-off tweak, introduce a minimal one rather than letting hardcoded values accumulate — even a small `tokens.css`/`theme.ts` with a dozen named values prevents the slow drift where five different components each hand-roll a slightly different shade of the "same" gray.
