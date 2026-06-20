# Contributing to meter

Thanks for your interest. meter is AGPL-3.0; by contributing you agree your contributions are licensed
under it. We use **DCO sign-off** (not a CLA): add `Signed-off-by: Your Name <you@example.com>` to each
commit (`git commit -s`).

## Prerequisites

- **Rust 1.88** (pinned via `rust-toolchain.toml`).
- **Docker** running — the integration tests spin up Postgres via testcontainers.
- **Node 24 + pnpm 10** for the control plane and SDKs; **bun** for the dashboard (shadcn preset).
- Restore the project skills (design system, transitions, Rust helpers) with
  `npx skills experimental_install` (they are pinned in `skills-lock.json`, not vendored).

## Build, test, lint

```bash
cargo fmt --all --check          # formatting
cargo clippy --workspace --all-targets -- -D warnings   # lints (warnings are errors)
cargo test --workspace           # unit + property + integration (needs Docker)
```

The ledger has a shared conformance suite (`meter_ledger::conformance`) run against every backend, plus
a concurrency no-overdraft test and end-to-end HTTP tests against a real Postgres. Keep all of it green.

TypeScript (control plane + SDK) and the dashboard:

```bash
pnpm install
pnpm exec biome check sdks apps
pnpm --filter "@meter/*" run typecheck
pnpm --workspace-concurrency=1 --filter "@meter/*" run test   # control plane runs on PGlite
cd sdks/python && PYTHONPATH=. python3 -m unittest discover -s tests -p "test_*.py"
cd apps/dashboard && bun install && bun run typecheck && bun run lint && bun run build
```

CI runs four jobs — **rust**, **typescript**, **python**, **dashboard** — all must pass.

## Standards

Read `CLAUDE.md` (engineering guide) and `docs/ARCHITECTURE.md` first. In short:

- **No shortcuts.** Correct schemas, real migrations, full tests; no half-done features or `unwrap` in
  non-test code. Money/credits are exact decimals, never floats. The ledger is append-only.
- **Atomic, well-organized code.** One concept per file; small focused modules; atomic, green commits.
- **Rust:** typed `thiserror` errors, exhaustive `match`, `#![forbid(unsafe_code)]`, sqlx migrations.
- **TypeScript:** Effect idioms, `Schema` at boundaries, named exports, no default exports (except
  Next.js pages); the UI uses the `meter-design-system` skill (shadcn preset + transitions.dev).

## Commits & PRs

Small, focused, green commits with clear messages. PRs should pass CI (fmt, clippy, tests) and keep
`README.md`, `docs/`, and `tickets/` current.
