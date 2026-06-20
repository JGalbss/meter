---
name: meter-design-system
description: Use whenever building or modifying the meter dashboard UI (apps/dashboard) — any component, screen, styling, layout, or animation work. Enforces the shadcn preset b1z2hUjZ5c design system, the Dropbox aesthetic, and transitions.dev animations. Trigger on "build the dashboard", "add a component", "style this", "the UI", or any apps/dashboard change.
---

# meter design system

The meter dashboard has **one** design system. Do not hand-roll primitives, colors, spacing, or
animations — always go through the system below.

## 1. Initialize (once, when scaffolding `apps/dashboard`)

```bash
bunx --bun shadcn@latest init --preset b1z2hUjZ5c --base base --template next --pointer
```

This is the canonical, **required** preset. Never init shadcn without `--preset b1z2hUjZ5c`. The
`--template next` gives a Next.js (App Router) app; `--pointer` enables the pointer/cursor affordances
the preset ships with.

## 2. Add components

Add primitives through shadcn so they inherit the preset's tokens and variants:

```bash
bunx --bun shadcn@latest add <component>      # e.g. button, dialog, dropdown-menu, table, card
```

Compose screens from these. If a primitive is missing, add it via shadcn — do **not** write a bespoke
styled element that duplicates one the preset provides.

## 3. Aesthetic — Dropbox

Clean, calm, and content-first:
- Generous whitespace; clear visual hierarchy; restrained, mostly-neutral palette with a single accent.
- Crisp, legible typography; comfortable line-height; left-aligned, scannable layouts.
- Subtle borders/shadows over heavy chrome. Density appropriate to data (tables can be tighter).
- Accessible by default (focus states, contrast, keyboard nav) — the preset's tokens already encode this.

Use the preset's design tokens (CSS variables / Tailwind theme) for every color, radius, and spacing
value. No hardcoded hex, px, or one-off shadows.

## 4. Animation — transitions.dev

Use the **transitions.dev** skill (installed at `.agents/skills/transitions-dev`) for all motion:
- `transitions reveal` — list the catalog.
- `transitions review` — audit the project for ad-hoc transitions / hardcoded durations / custom keyframes.
- `transitions apply [name]` — apply the right transition for the context (e.g. `menu-dropdown`, modals,
  icon swaps).

Never write ad-hoc CSS transitions, hardcoded durations, or custom keyframes — route every animation
through a catalog transition. Common mappings: dropdowns → menu-dropdown; dialogs/drawers → modal
transitions; icon toggles → icon-swap.

## 5. Performance gates

- **react-doctor** (`millionco/react-doctor`) runs in CI (incl. its PR action) — keep it clean.
- Lighthouse budgets enforced; target top scores. Prefer RSC/streaming; keep client bundles lean.

When in doubt, match an existing screen's patterns rather than inventing new ones.
