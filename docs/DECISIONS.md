# Decision log

Running log of locked decisions. The full reasoning and system design live in `ARCHITECTURE.md`;
this is the quick-reference index of what was decided and why.

| # | Decision | Rationale | Status |
|---|---|---|---|
| 1 | **Rust** for the entire backend (control plane + data plane), not Zig | Performance + memory safety + mature ecosystem (axum/sqlx/tokio); single language = one binary, simplest self-host, no cross-language currency drift | locked |
| 2 | **PostgreSQL** is the system of record for money & config (ledger, accounts, rate cards, budgets, grants, invoices, hierarchy) | Credit transactions are far lower-volume than raw usage; Postgres single-primary + read replicas scales very far (cf. OpenAI: 800M users on one primary) with operational discipline | locked |
| 3 | **ClickHouse** owns the high-volume usage-event firehose + analytics/rollups | OLAP fit for millions of events/sec; proven by Lago & OpenMeter; idempotent ingest via transaction_id; optional for small self-host | locked |
| 4 | **TypeScript only client-side**: Next.js dashboard + TS SDK. No TS backend | Keeps the backend single-language; UI/SDK is where TS shines | locked |
| 5 | **AGPL-3.0** license | Open and self-hostable, but network use of modifications must be shared — prevents a closed competing hosted fork | locked |
| 6 | Backend behind a versioned **OpenAPI contract**; SDK/UI types are codegen'd from it | Single source of truth; makes the all-Rust control-plane decision reversible | locked |
| 7 | **Double-entry ledger**; balances derived, never mutated; all ingest idempotent | Auditable correctness; no lost/double/overspent credits | locked |
| 8 | Real-time enforcement via **reserve → settle** (two-phase) | Stop out-of-budget agents on the hot path with no overdraft; settle with actuals | locked |
| 9 | Quality gates: clippy/rustfmt (Rust), **react-doctor** (millionco) for React, **agent-doctor** (JGalbss) for any Effect-TS | Enforce clean code automatically in CI | locked |
| 10 | Repo: **public, `JGalbss/meter`**, no AI footprint | Founder directive | locked |

## Deferred / to confirm in ARCHITECTURE.md

- **Ingest log**: start with a simple durable path (Postgres outbox / NATS) and make Kafka/Redpanda an
  opt-in scale-out component, rather than requiring Kafka for self-host. (Lago uses Kafka→ClickHouse at
  scale; we want a smaller default footprint.)
- **Hot-path balance store**: whether the reserve/settle counters live in Postgres (advisory locks /
  `SELECT ... FOR UPDATE` on account rows) for v1 vs. a sharded in-memory/Redis layer at scale.
- **Multi-tenancy isolation**: shared-schema + tenant_id (+ RLS) as the default; schema/DB-per-tenant for
  single-tenant self-host. Confirm in ARCHITECTURE.md.
- **TigerBeetle**: evaluated and deferred — excellent for balances/holds but can't do the rich queries we
  need, and adds a stateful system to self-host. Revisit as an optional high-throughput ledger backend.
