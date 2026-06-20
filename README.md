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
| **Engine** | Rust | The data plane and **sole owner of money-truth**: event ingestion, the double-entry credit ledger, real-time reserve/settle enforcement, pricing. Exposes gRPC. |
| **Control plane** | TypeScript · Effect + Drizzle | The management API the dashboard hits: orgs/teams/users/roles, products, rate cards, budgets, grants, invoices, webhooks. Computes no money — it calls the engine over gRPC. |
| **System of record** | PostgreSQL | **Money & config only** — the engine owns the ledger schema; the control plane owns the config schema. Events live in ClickHouse (ADR 0003). |
| **Events & analytics** | ClickHouse | The usage **event firehose** (system of record for events) + rollups. Required (ADR 0003). |
| **Dashboard** | Next.js / React | Dropbox-quality console on the shadcn design system. |
| **SDKs** | TypeScript, Python | Drop-in instrumentation; the hot path (ingest / reserve / settle) talks to the engine directly. |

Money-truth lives only in the engine, so there is exactly one ledger and no cross-language drift. The
engine↔control-plane contract is protobuf; the dashboard/customer contract is OpenAPI.

Docs: [VISION](docs/VISION.md) · [ARCHITECTURE](docs/ARCHITECTURE.md) · [SLO](docs/SLO.md) ·
[DECISIONS](docs/DECISIONS.md) · [ADRs](docs/adr/) · [tickets](tickets/README.md).

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

Bring up Postgres + the engine (`:8080`) + the control plane (`:8090`), each applying its migrations on
boot:

```bash
docker compose -f deploy/docker-compose.yml up --build
```

The control plane (config + notifications/alerts/webhooks) is the API the dashboard hits; it calls the
engine for money-truth. Run the dashboard against it:

```bash
cd apps/dashboard && bun install \
  && METER_CONTROL_PLANE_URL=http://localhost:8090 \
     METER_ENGINE_URL=http://localhost:8080 \
     DASHBOARD_SESSION_SECRET=$(openssl rand -hex 32) DASHBOARD_PASSWORD=changeme \
     bun run dev
```

The dashboard is auth-gated (signed-cookie session); set `DASHBOARD_SESSION_SECRET` and
`DASHBOARD_PASSWORD` or it stays locked. React code is reviewed by **react-doctor** (advisory PR
workflow + `bun run doctor`).

## Status — what works today

The Rust engine is functional and tested end-to-end against real Postgres:

- **Ledger** — double-entry, append-only; grant / reserve / settle / void; balances derived; no
  overdraft under concurrency (property- and conformance-tested; in-memory reference + Postgres backend).
- **Pricing** — multi-dimensional rate cards; token→credit translation with margin (`meter-pricing`).
- **Enforcement** — reserve→settle priced via rate cards (`meter-enforcement`).
- **Events** — editable, custom-field usage events: record (idempotent), amend (append-only version),
  `void_run`; latest-non-voided reads.
- **Invoicing** — deterministic invoice summed from the ledger (`enforced == billed`).
- **Catalog** — curated model rate-card snapshot (`meter-ratecards`).
- **Engine** — the `meter` binary serving HTTP; `meterctl` admin CLI; Docker image + compose.

- **Usage metering** — `POST /v1/usage` prices token usage via the catalog (`model` + token counts),
  records the event, and charges credits in one idempotent call (the core loop, end-to-end tested).
- **Budgets & alerts** — `GET /v1/accounts/{id}/budget?…&limit` returns usage vs limit with a threshold
  status (`ok` / `warning` ≥80% / `exceeded` ≥100%).

Engine HTTP endpoints under `/v1`: `usage` (meter), `accounts` (open · balance · grants · entries ·
events · invoice · budget · usage-by-day), `reservations` (reserve · settle · void), `events`
(record · get · amend), `runs/{id}/void`, `audit`.

Beyond the engine:

- **Control plane** (`apps/control-plane`, TypeScript · Effect + Drizzle) — the config + ops API the
  dashboard hits: organizations, products, notifications (pull/read/ack), alert rules with a
  **budget-evaluation loop** (asks the engine to classify usage, raises notifications on escalation),
  and **signed, retried webhooks** with a dead-letter log. Applies migrations on boot; Docker image +
  compose service; e2e-tested over an in-process server.
- **Audit log** — engine middleware records every mutating request; `GET /v1/audit`.
- **SDKs** (`sdks/typescript`, `sdks/python`) — drop-in client + run governance (`withRun`) + usage
  adapters for Anthropic, OpenAI, Vercel AI SDK, Gemini/Vertex, Bedrock, and LangChain/LangGraph.
- **Dashboard** (`apps/dashboard`, Next.js + shadcn preset) — overview, organizations, notifications,
  alert rules, and webhooks, wired to the control plane.

- **Analytics (ClickHouse, optional):** `meter-store-ch` — `events_raw` firehose
  (`ReplacingMergeTree`, idempotent on `org_id`+`event_id`) + usage-by-model rollups, integration-tested
  against a real ClickHouse container. Money-truth stays in the engine; ClickHouse is analytics only.

In progress (see [tickets](tickets/README.md)): OpenAPI emission + Stainless-generated SDKs, protobuf
engine⇄control-plane contract, ClickHouse rollup MVs + query API, RBAC, dashboard usage charts,
throughput benchmarks.

## Self-hosting

Minimal footprint: the **engine** + **control plane** **+ PostgreSQL**. Add ClickHouse for high-volume
usage analytics. Redpanda / Redis / TigerBeetle are opt-in scale-out backends behind traits. Docker
Compose for local/dev; Helm for production.

## License

[AGPL-3.0](LICENSE). Run, modify, and self-host meter freely. If you offer it to others over a network,
your modifications must be released under the same license.
