# Local development stack

One command brings up the whole stack for local work:

```bash
pnpm dev
```

It runs `dev:infra` (Postgres + ClickHouse in Docker, waiting until both are healthy), then launches
the engine, control plane, and dashboard together with prefixed, colour-coded logs and a single Ctrl-C
that stops all three.

## Prerequisites

- **Docker** — for the Postgres + ClickHouse containers (`deploy/dev/docker-compose.yml`).
- **Rust toolchain** — the engine runs via `cargo run` (see `rust-toolchain.toml`).
- **Node ≥ 22 + pnpm** — the workspace package manager (the control plane runs under `tsx`).
- **bun** — the dashboard (Next.js) is bun-managed and outside the pnpm workspace.

## Services and ports

| Service       | URL                     | Notes                                              |
| ------------- | ----------------------- | -------------------------------------------------- |
| Postgres      | `127.0.0.1:5433`        | money-truth + control-plane config; user/pass/db `meter` |
| ClickHouse    | `127.0.0.1:8123`        | events firehose; user/pass/db `meter`              |
| engine        | `127.0.0.1:8080` (HTTP) | applies its Postgres + ClickHouse migrations on boot |
| control plane | `127.0.0.1:8090`        | the API the dashboard hits; calls the engine       |
| dashboard     | `127.0.0.1:3000`        | the console (falls back to `:3002` if `:3000` is taken) |

## Per-service scripts

Run any service on its own (all read the same defaults as `pnpm dev`):

```bash
pnpm dev:infra          # start Postgres + ClickHouse (and wait for health)
pnpm dev:infra:down     # stop and remove them
pnpm dev:engine         # cargo run the engine (HTTP only)
pnpm dev:control-plane  # tsx watch the control plane
pnpm dev:dashboard      # bun run the dashboard
```

## Notes

- **Postgres is on host port `5433`, not `5432`** — so it doesn't collide with a Postgres already
  running on the default port. Connect with `psql postgres://meter:meter@127.0.0.1:5433/meter`. (Inside
  the compose network it is still `5432`; only the host mapping changes.)
- **The dev engine serves HTTP only** (`METER_ROLES=http`). Nothing local uses the engine's gRPC
  surface — the control plane and dashboard both call its HTTP API — so the dev stack skips gRPC and
  avoids the `:50051` "address already in use" crash a leftover engine would otherwise cause.
- **The dashboard is auth-gated.** `dev:dashboard` sets dev values for `DASHBOARD_PASSWORD` and
  `DASHBOARD_SESSION_SECRET`; log in with the password `meter`.
- A stale process holding `:3000` (an old dashboard) makes Next.js fall back to `:3002`. A leftover
  engine, control plane, or dashboard from a previous run can also hold its port — stop those processes
  if a service reports a port conflict.

## Versus the production compose

This dev stack runs the apps on the host for fast iteration (engine via `cargo run`, control plane via
`tsx watch`, dashboard via `next dev` with hot reload). For a production-style run where every service
is built and run as a container — engine **and** gRPC, control plane, dashboard, and the docs site —
use the full compose instead:

```bash
docker compose -f deploy/docker-compose.yml up --build
```
