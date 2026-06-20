# ADR 0001 — Engine / control-plane split and the protobuf seam

**Status:** Accepted (2026-06-19). **Amends** `ARCHITECTURE.md` §3.1, which had locked a single
all-Rust backend.

## Context

`ARCHITECTURE.md` §3.1 chose one all-Rust backend for simplicity and to avoid money-type drift, and
explicitly noted that adopting an Effect-TS control plane would be a legitimate RFC to reopen the
decision. The founder has made that call: split the backend along the plane boundary — a Rust
performance **engine** and a TypeScript **control plane** (Effect + Drizzle on Postgres) that the
frontend hits, with **protobuf/gRPC** between them. Effect + Drizzle is the team's home stack; keeping
the hot path in Rust preserves performance.

## Decision

Two backend services, split by plane:

- **Engine (Rust)** — the data plane and the *sole owner of money-truth*. Owns the ledger (its own
  Postgres schema via `sqlx`), ingest, reserve/settle enforcement, pricing computation (token→credit),
  and usage analytics (ClickHouse). Exposes a gRPC API defined in protobuf. The SDK hot path (ingest,
  reserve, settle, void) talks to the engine **directly**.
- **Control plane (TypeScript — Effect + Drizzle)** — the management plane the frontend hits. Owns
  *config* in its own Postgres schema via Drizzle: orgs/teams/users/roles, API keys, products/agents,
  rate-card config, budget/grant config, webhook subscriptions, invoice presentation. **Computes no
  money.** Calls the engine over gRPC for every money/usage operation (apply grant, read balance, push
  rate-card config, generate invoice from the ledger, read analytics). Exposes the public management
  HTTP API + OpenAPI for the dashboard.
- **Contracts:** protobuf (Buf) is the single source of truth for engine↔control-plane RPC →
  `prost` (Rust) + `ts-proto`/connect (TS), with Buf breaking-change checks in CI. The control plane
  emits OpenAPI for the dashboard and the customer-facing management SDK. Two surfaces, no hand-mirrored
  types.

## Why this keeps the baseline's correctness wins

The financial critic's "two authoritative ledgers / money drift" risk is avoided because **money-truth
lives only in the Rust engine**. The control plane never computes credits or currency — it passes opaque
decimal strings over protobuf and calls the engine for every money operation. So there is still exactly
one ledger and one money implementation; `settle` is still the one priced posting; invoices are still
the engine summing its own ledger (`enforced == billed`).

## Consequences

- **Footprint** grows from "one binary + Postgres" to **engine (Rust) + control-plane (Node) + Postgres
  (two schemas in one instance) + optional ClickHouse**, packaged via Docker Compose. Still no
  Kafka/Redis/TigerBeetle by default.
- **Contributor funnel** splits by plane: control plane needs Effect/TS, engine needs Rust — accepted,
  matches team expertise. `agent-doctor` gates the Effect-TS; `react-doctor` gates the dashboard.
- **Reversible:** because the boundary is protobuf, replacing either side later is a re-implementation
  behind the same contract.

## Repo impact

- New: `apps/control-plane/` (Effect + Drizzle), `proto/` (Buf module), `crates/meter-proto` (generated
  Rust types + gRPC service).
- The Rust crates are the **engine**; `meter-api` is the engine's gRPC/HTTP surface (not the control
  plane); `meter-engine` is the engine binary. The control-plane config DB is Drizzle-managed; the
  engine ledger/event DB is `sqlx`-managed — separate schemas, separate migrations.
