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

meter is a **Rust** backend over **PostgreSQL** and **ClickHouse**:

| Component | Tech | Responsibility |
|---|---|---|
| **Engine** | Rust | Event ingestion, the double-entry credit ledger, real-time reserve/settle enforcement. Ships as a single binary. |
| **System of record** | PostgreSQL | Money & configuration: ledger, accounts, rate cards, budgets, grants, invoices, the org/team/user hierarchy. |
| **Usage & analytics** | ClickHouse | The high-volume usage-event firehose and rollups. Optional for small deployments. |
| **Dashboard** | Next.js / React | Stripe-quality console for usage, budgets, pricing, and invoices. |
| **SDKs** | TypeScript, Python | Drop-in instrumentation that wraps agent/LLM calls like OpenTelemetry spans and emits usage automatically. |

The control plane and data plane share one Rust codebase and one money type — no cross-language drift on
currency math. See [`docs/VISION.md`](docs/VISION.md) and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Self-hosting

Minimal footprint: the `meter` binary **+ PostgreSQL**. Add ClickHouse when you need high-volume usage
analytics. Docker Compose for local/dev; Helm for production.

## License

[AGPL-3.0](LICENSE). Run, modify, and self-host meter freely. If you offer it to others over a network,
your modifications must be released under the same license.
