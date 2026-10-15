# Anti-pattern gallery

Scan this before calling visual or structural work finished. Each entry: the tell, why it's a problem, what to do instead.

## Visual tells of generic/unconsidered output

| Tell | Why it's a problem | Do instead |
|---|---|---|
| Inter / Roboto / Arial / Space Grotesk used *by default* | Reads as "no decision was made" — these are the safe defaults every template ships with | Choose a typeface for a reason tied to the product's actual character, or deliberately keep a system font stack as a considered choice for a content-heavy doc |
| Blue-to-purple gradient hero | Traces directly to Tailwind's `indigo-500` default; it has become the universal tell of AI-generated design | Derive color from the actual brand/subject; if a gradient is warranted, make it a considered, subject-specific pair, not the default indigo/violet |
| Symmetric three-column "icon + heading + paragraph" card grid | The reflexive layout for any list of 3 things, regardless of whether the content actually has 3 equal parts | Let the real content set the layout — asymmetry, full-bleed sections, or a simple hairline-divided list often communicate better |
| Glassmorphism, uniform rounded corners, one flat shadow token applied everywhere | Decorative complexity with no relationship to hierarchy or elevation | Reserve elevation/blur for where it communicates actual depth or layering; keep corner radius intentional, not uniform by reflex |
| Generic filler headline copy ("Build faster. Ship smarter.") | Could belong to literally any product — says nothing specific | Write copy that only *this* product/brand could say, using real facts and real voice |
| Interchangeable thin line icons unrelated to the brand | Decoration with no semantic connection to what they represent | Choose icons (or skip them) based on what actually clarifies meaning here |
| A completely static page — no scroll response, no reveals, nothing that reacts | Reads as "template," not "built" | Add a real, lightweight motion system where it fits (scroll-reveal, count-up, subtle parallax) — but keep it purposeful and respect `prefers-reduced-motion` |
| Numbered markers (01 / 02 / 03) on content that isn't actually sequential | Implies an order or process that doesn't exist | Only number real sequences; use plain headings or hairline dividers for a non-sequential list |

## Engineering anti-patterns

| Tell | Why it's a problem | Do instead |
|---|---|---|
| `div` + `onClick` standing in for `<button>` | Loses keyboard operability, focus, and screen-reader semantics for free | Use the native interactive element; only add ARIA when there's genuinely no native equivalent |
| `outline: none` with no replacement | Removes the only visible sign of keyboard focus | Style a clear custom focus state if the default doesn't fit the design |
| `!important` as a patch for a specificity fight | Papers over the actual cascade problem instead of fixing it, and the next override needs `!important` too | Structure selectors/cascade layers so specificity is predictable; fix the root conflict |
| A hardcoded hex/px value duplicating an existing token | Two sources of truth for the same value drift apart over time | Reference the existing token; add one if it's genuinely missing |
| Premature shared abstraction from 2–3 similar-looking usages | Guesses at a shape for future requirements that may never come, and contorts current call sites to fit | Keep them separate until a third or fourth real usage makes the shared shape obvious |
| Missing loading/empty/error state ("it'll never be empty in practice") | It will be empty, or fail, in production — this always surfaces as a bug report later | Design all three states as part of the initial implementation, not an afterthought |
| Layout built with per-element margins instead of `gap` | Margins collapse or double unpredictably between siblings | Lay out sibling groups with flex/grid and `gap` |
| Client-side fetch gating the largest above-the-fold element | Directly costs LCP — nothing paints until the fetch resolves | Server-render or statically know the hero content where possible |
| New dependency added for something a few lines of code could do | Ongoing bundle-size and maintenance tax for a one-time convenience | Write the few lines directly unless the dependency earns its long-term cost |
