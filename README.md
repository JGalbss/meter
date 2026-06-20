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
| **System of record** | PostgreSQL | Money & config. The engine owns the ledger/event schema; the control plane owns the config schema. |
| **Usage & analytics** | ClickHouse | High-volume usage firehose + rollups. Optional add-on. |
| **Dashboard** | Next.js / React | Dropbox-quality console on the shadcn design system. |
| **SDKs** | TypeScript, Python | Drop-in instrumentation; the hot path (ingest / reserve / settle) talks to the engine directly. |

Money-truth lives only in the engine, so there is exactly one ledger and no cross-language drift. The
engine↔control-plane contract is protobuf; the dashboard/customer contract is OpenAPI.

Docs: [VISION](docs/VISION.md) · [ARCHITECTURE](docs/ARCHITECTURE.md) · [SLO](docs/SLO.md) ·
[DECISIONS](docs/DECISIONS.md) · [ADRs](docs/adr/) · [tickets](tickets/README.md).

## Self-hosting

Minimal footprint: the **engine** + **control plane** **+ PostgreSQL**. Add ClickHouse for high-volume
usage analytics. Redpanda / Redis / TigerBeetle are opt-in scale-out backends behind traits. Docker
Compose for local/dev; Helm for production.

## License

[AGPL-3.0](LICENSE). Run, modify, and self-host meter freely. If you offer it to others over a network,
your modifications must be released under the same license.
