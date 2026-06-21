# Roadmap

Where meter is and where it's going. This is the curated view; [tickets/](tickets/README.md) is the
living, per-epic detail. meter is pre-1.0 — order here is intent, not a commitment, and the schemas and
APIs will keep moving until the first tagged release.

## Shipped

The engine is functional and tested end to end against real Postgres and ClickHouse.

- **Ledger** — double-entry, append-only, over a `LedgerBackend` trait; grant / reserve / settle / void
  / refund, per-session leasing, hold timeouts with heartbeat and settle-after-void. Balances derived.
  No-overdraft and idempotency are property- and conformance-tested against the in-memory reference and
  the Postgres backend.
- **Pricing & catalog** — multi-dimensional rate cards (`provider_cost` / `customer`, margin), the
  standard / graduated / volume / package charge models, per-action charges, versioned cards with
  re-rating and a pricing simulator, and a dated catalog of 11 Anthropic / OpenAI / Google / DeepSeek /
  Alibaba models.
- **Metering & enforcement** — `POST /v1/usage` prices, records, and charges in one idempotent call;
  token-priced reserve/settle with HARD/SOFT limits; `burnable` cost-only usage.
- **Events** — immutable, custom-field events with `record`, `amend` (new version), and `void_run`;
  ClickHouse is the system of record; 202-fast batch ingest; usage analytics (by model, day, and custom
  field) with reconciliation against the source of record.
- **Invoicing** — a deterministic invoice summed straight from the ledger, so `enforced == billed`.
- **Engine surface** — HTTP and gRPC, role-selectable via `METER_ROLES`; a Prometheus `/metrics`
  endpoint; the `meterctl` admin CLI (migrate, seed, balance, grant, price, sweep, void, void-run,
  reconcile, rebuild-rollups).
- **Control plane** — Effect + Drizzle: organizations, products, API keys with RBAC and platform/org
  scopes, alert rules with a budget-evaluation loop, and signed, retried webhooks with a dead-letter
  log. Emits OpenAPI; the dashboard client is generated from it.
- **Dashboard** — a full operator console over both planes: usage analytics, an events explorer with
  amend / void-run, accounts and ledger entries, invoices, the audit log, a rate-card catalog viewer,
  and a pricing simulator.
- **Audit log** — engine middleware records every mutating request; filterable, request-id correlated,
  CSV-exportable; stored in ClickHouse.
- **Docs & contracts** — the public docs site with generated engine + control-plane API references; the
  protobuf contract with `buf lint` + breaking-change gates; wire-protocol versioning policy.
- **Deployment** — engine, control-plane, dashboard, and docs images; a 6-service Docker Compose; a Helm
  chart with Ingress/TLS and air-gapped notes; a GHCR publish workflow.

## In progress

- The generated **TypeScript gRPC client** for the control-plane → engine path (the engine side and the
  proto contract are done).
- **Event amend → ledger delta posting** ([ADR 0009](docs/adr/0009-amend-delta-posting.md), proposed) —
  a usage-aware re-pricing amend that posts the engine-computed delta.
- **Invoice lifecycle** — the `Draft → Grace → Finalized → Void` state machine, credit-notes for
  corrections, and billing periods independent of budget cycles.
- **Control-plane config resources** — teams/users/roles, rate-card and budget/grant configuration,
  surfaced as dashboard write screens.
- **RLS** as defense-in-depth for tenant isolation (the app-level enforcement is shipped, per
  [ADR 0007](docs/adr/0007-tenant-isolation.md)).
- **Catalog scraper** — scheduled auto-updates and more providers, replacing the curated snapshot.
- **Stainless-generated base SDK clients**, replacing the hand-written ones (adapters and run governance
  carry over unchanged).

## Planned

- **`meter-ingest`** — the `IngestSource` trait, a Postgres-outbox default source, an effectively-once
  consumer, and a dead-letter path.
- **Opt-in scale-out backends behind the traits** — a TigerBeetle `LedgerBackend`, a Redpanda/Kafka
  ingest buffer, and a Redis hot-path pre-filter, each activated by a measured trigger
  ([ADR 0005](docs/adr/0005-provider-scale-throughput.md)).
- **Horizontal scale** — stateless engine replicas, a ClickHouse cluster, and per-org sharding of the
  money store as the final lever.
- **Notifications** — email and in-app delivery, enforce-on-threshold (block), and per-user
  subscriptions.
- **Tamper-evident audit** — a hash chain over the audit log.

See [tickets/](tickets/README.md) for the full checklist and the latest status of each item.
