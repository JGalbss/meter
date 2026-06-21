<div align="center">

# meter

**High-performance, open-source metering, billing & invoicing for the agent era.**

</div>

meter turns raw agent and API usage into credits, budgets, and invoices — backed by an immutable,
double-entry ledger that is the single source of truth. It ingests usage at high throughput, enforces
budgets and credit limits in real time on the request hot path, and keeps its concepts deliberately
small. Built to self-host in your own VPC, single-tenant or multi-tenant.

> ⚠️ **Early and under active construction.** Schemas and APIs will change until the first tagged release.

## Why meter

- **Ledger-first.** Every credit movement is an immutable, double-entry transaction. Balances are
  derived, never edited. Everything reconciles back to one auditable source of truth.
- **Built for agents.** Meter by tokens, by credits, by artifacts, or by outcomes the agent actually
  achieves — across an org → team → user hierarchy.
- **Real-time enforcement.** Reserve credits before a call, settle with actuals after. Out-of-budget
  agents are stopped on the hot path with no overdraft.
- **Flexible pricing, few primitives.** Multi-dimensional rate cards (input / output / cache / context
  tiers, per model), token→credit translation with a fixed cash value and margin, volume tiers — without
  the rate-card sprawl of legacy tools.
- **Budgets & grants.** Periodic budgets (e.g. 400 credits/week per product) that are independent of
  invoicing periods; prepaid grants; promotional credits; clear burn-down priority.
- **Batteries-included model rate cards.** An always-current catalog of provider model prices, so you
  don't have to maintain them.
- **Open & self-hostable.** Run it yourself; we also offer a managed version.

## Architecture

meter splits along a hard data-plane / control-plane seam:

| Component | Tech | Responsibility |
|---|---|---|
| **Engine** | Rust | The data plane and **sole owner of money-truth**: event ingestion, the double-entry credit ledger, real-time reserve/settle enforcement, pricing. Exposes gRPC and an HTTP/OpenAPI surface. |
| **Control plane** | TypeScript · Effect + Drizzle | The management API the dashboard hits: orgs/teams/users/roles, products, rate cards, budgets, grants, invoices, webhooks. Computes no money — it calls the engine for all money-truth. |
| **System of record** | PostgreSQL | **Money & config only** — the engine owns the ledger schema; the control plane owns the config schema. The high-velocity firehoses (events, audit) live in ClickHouse. |
| **Events, audit & analytics** | ClickHouse | The usage **event firehose** (system of record for events) + the append-only **audit log** + analytics rollups. The non-transactional, high-velocity writes, kept off the money DB. Required (ADR 0003/0004). |
| **Dashboard** | Next.js / React | Dropbox-quality console on the shadcn design system. |
| **Docs site** | Next.js + MDX | Public documentation: concepts, API reference, SDKs, self-host (`apps/docs`). |
| **SDKs** | TypeScript, Python | Drop-in instrumentation; the hot path (ingest / reserve / settle) talks to the engine directly. |

Money-truth lives only in the engine, so there is exactly one ledger and no cross-language drift. The
engine↔control-plane contract is protobuf; the dashboard/customer contract is OpenAPI.

**Scale.** meter is built toward provider-grade volume (millions of metering ops/sec, billions of
events/day) without softening the ledger. Pricing is in-memory; events and audit are ClickHouse
firehoses; the only true bottleneck is the transactional money path, attacked behind the
`LedgerBackend` seam — per-session **leasing** (one round-trip per session, not per token) and a
**TigerBeetle** backend (two-phase transfers, integer credits, database-enforced no-overdraft) — so
correctness and simplicity are never traded for speed. See [ADR 0005](docs/adr/0005-provider-scale-throughput.md).

Docs: [VISION](docs/VISION.md) · [ARCHITECTURE](docs/ARCHITECTURE.md) · [SLO](docs/SLO.md) ·
[DECISIONS](docs/DECISIONS.md) · [ADRs](docs/adr/) · [tickets](tickets/README.md).

## Performance

The engine's hot path is benchmarked with `criterion`. Measured on an Apple M5 Pro, single node:

| Path | What it measures | Median |
|---|---|---:|
| **Pricing** | 5-dimension event → COGS → margin → credits (in memory, O(1)) | **~191 ns** (~5.2 M ops/s/core) |
| **Reserve → settle** | Full credit reserve + settle with no-overdraft against **Postgres** | **~1.32 ms** |

Reproduce: `cargo bench -p meter-pricing` and `cargo bench -p meter-store-pg` (the latter spins a
throwaway Postgres container via Docker).

**vs. Lago, Metronome, Orb.** meter's defining path is *synchronous real-time enforcement* — reserve
credits before an agent call, settle actuals after, refuse the call with no overdraft — backed by a
double-entry ledger. Lago, Metronome, and Orb are *ingest-then-aggregate-then-bill* pipelines: usage
events are streamed and priced asynchronously. So "speed" is not one quantity across them, and two of
the three (Metronome, Orb) are closed SaaS that can't be self-hosted or independently load-tested. The
[benchmarks page](docs/BENCHMARKS.md) lays out the architectural comparison and each vendor's published
throughput claims **side by side, clearly labeled** — meter's numbers are measured and reproducible
here; the competitors' are their own marketing figures for a different (async) operation, not a
head-to-head we ran.

## Quickstart (engine)

```bash
# 1. Start Postgres (money-truth) + ClickHouse (events, ADR 0003)
docker compose -f deploy/dev/docker-compose.yml up -d postgres clickhouse

# 2. Run the engine (applies ledger migrations on Postgres + event migrations on ClickHouse on boot)
METER_DATABASE_URL=postgres://meter:meter@localhost:5432/meter \
  METER_CLICKHOUSE_URL=http://localhost:8123 cargo run -p meter-engine

# 3. Exercise the ledger
curl localhost:8080/health
ACC=$(curl -s localhost:8080/v1/accounts -d '{"org_id":"11111111-1111-1111-1111-111111111111","scope":"org","no_overdraft":true}' | jq -r .id)
curl -s localhost:8080/v1/accounts/$ACC/grants -d '{"amount":"100","source":"paid"}'
curl -s localhost:8080/v1/accounts/$ACC/balance
```

## Quickstart (full stack)

Bring up the whole stack — Postgres + ClickHouse + the engine (`:8080`) + the control plane (`:8090`) +
the dashboard (`:3000`) + the docs site (`:3001`) — each service applying its migrations on boot:

```bash
docker compose -f deploy/docker-compose.yml up --build
```

The control plane (config + notifications/alerts/webhooks) is the API the dashboard hits; it calls the
engine for money-truth. The dashboard is auth-gated (signed-cookie session) — set `DASHBOARD_PASSWORD`
and `DASHBOARD_SESSION_SECRET` (compose ships dev defaults). Browse the console at
`http://localhost:3000` and the documentation at `http://localhost:3001`.

For dashboard development against the running stack, run it in dev mode instead:

```bash
cd apps/dashboard && bun install \
  && METER_CONTROL_PLANE_URL=http://localhost:8090 \
     METER_ENGINE_URL=http://localhost:8080 \
     DASHBOARD_SESSION_SECRET=$(openssl rand -hex 32) DASHBOARD_PASSWORD=changeme \
     bun run dev
```

React code is reviewed by **react-doctor** (advisory PR workflow + `bun run doctor`).

## Status — what works today

The Rust engine is functional and tested end-to-end against real Postgres:

- **Ledger** — double-entry, append-only; grant / reserve / settle / void; balances derived; no
  overdraft under concurrency (property- and conformance-tested; in-memory reference + Postgres backend).
- **Pricing** — multi-dimensional rate cards; token→credit translation with margin (`meter-pricing`).
- **Enforcement** — reserve→settle priced via rate cards (`meter-enforcement`).
- **Events** — editable, custom-field usage events: record (idempotent), amend (append-only version),
  `void_run`; latest-non-voided reads. The system of record is **ClickHouse** (ADR 0003); conformance-
  tested identically to the in-memory reference.
- **Invoicing** — deterministic invoice summed from the ledger (`enforced == billed`).
- **Catalog** — curated model rate-card snapshot (`meter-ratecards`).
- **Engine** — the `meter` binary serving HTTP; `meterctl` admin CLI; Docker image + compose.

- **Usage metering** — `POST /v1/usage` prices token usage via the catalog (`model` + token counts),
  records the event, and charges credits in one idempotent call (the core loop, end-to-end tested).
- **Budgets & alerts** — `GET /v1/accounts/{id}/budget?…&limit` returns usage vs limit with a threshold
  status (`ok` / `warning` ≥80% / `exceeded` ≥100%).

Engine HTTP endpoints under `/v1`: `usage` (meter), `accounts` (open · balance · grants · entries ·
events · invoice · budget · usage-by-day), `reservations` (reserve · settle · void), `leases`
(open · close), `events` (record · batch · get · amend), `runs/{id}/void`, `orgs` (usage-by-model ·
usage-by-day · event-count), `audit`.

Beyond the engine:

- **Control plane** (`apps/control-plane`, TypeScript · Effect + Drizzle) — the config + ops API the
  dashboard hits: organizations, products, **API keys with RBAC** (viewer/member/admin roles enforced
  by the auth middleware), notifications (pull/read/ack), alert rules with a **budget-evaluation loop**
  (asks the engine to classify usage, raises notifications on escalation), and **signed, retried
  webhooks** with a dead-letter log. Request-id + structured access-log middleware for tracing. Applies
  migrations on boot; Docker image + compose service; e2e-tested over an in-process server.
- **Audit log** — engine middleware records every mutating request; `GET /v1/audit`. Stored in
  ClickHouse (ADR 0004) — a high-velocity append-only firehose, kept off the money database.
- **SDKs** (`sdks/typescript`, `sdks/python`) — drop-in client + run governance (`withRun`) + usage
  adapters for Anthropic, OpenAI, Vercel AI SDK, Gemini/Vertex, Bedrock, and LangChain/LangGraph.
- **Dashboard** (`apps/dashboard`, Next.js + shadcn preset) — a full operator console: overview (with a
  top-models-by-spend summary), organizations, products, API keys (mint + RBAC role select),
  notifications, alert rules, and webhooks (wired to the control plane); plus engine-read views — usage
  analytics (usage-by-model + daily credit burn), an events explorer with **amend / void-run** actions,
  accounts (balance + ledger entries), invoices (month-to-date statement), the audit log, a
  **rate-card catalog** viewer, and a **pricing simulator** (re-rate a usage profile across two
  catalogued models). Shipped as a Docker image; run-verified.
- **Docs site** (`apps/docs`, Next.js + MDX) — concepts, a narrative API reference plus **generated
  references for both the engine and control-plane surfaces** (rendered from their committed OpenAPI
  contracts, drift-checked in CI), SDK guides with provider adapters, and self-host instructions (incl.
  air-gapped). Client-side **search** (Pagefind over a static export); built and typechecked in CI;
  shipped as a Docker image.

- **ClickHouse** (`meter-store-ch`, required) — the **system of record for events** (editable model:
  record/amend/void via `ReplacingMergeTree` + `FINAL`) plus the **audit log** and **usage analytics**
  (ADR 0003/0004). Integration-tested against a real ClickHouse container. Money-truth stays in the
  Postgres ledger; the high-velocity firehoses live here.
- **Deployment** — engine, control-plane, dashboard, and docs Docker images; a 6-service docker-compose
  (Postgres · ClickHouse · engine · control plane · dashboard · docs); a Helm chart (toggleable
  in-cluster Postgres/ClickHouse, Ingress/TLS) and a GHCR publish workflow.

In progress (see [tickets](tickets/README.md)): the protobuf engine↔control-plane contract and its
generated TypeScript client (the OpenAPI surface and the generated dashboard client are done), and a
rate-card catalog scraper for more providers.

## Self-hosting

Minimal footprint: the **engine** + **control plane** **+ PostgreSQL**. Add ClickHouse for high-volume
usage analytics. Redpanda / Redis / TigerBeetle are opt-in scale-out backends behind traits. Docker
Compose for local/dev; Helm for production.

## License

[AGPL-3.0](LICENSE). Run, modify, and self-host meter freely. If you offer it to others over a network,
your modifications must be released under the same license.
