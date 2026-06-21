# meter

A metering and billing engine for AI agents, built on an immutable double-entry ledger.

meter prices usage, enforces budgets on the request path, and turns the result into invoices — without
ever losing, double-counting, or overspending a credit. The ledger is the single source of truth;
balances are derived, never edited. It is open source and built to self-host in your own VPC,
single-tenant or multi-tenant.

> **Pre-release.** The engine is functional and tested end to end against real Postgres and ClickHouse.
> Schemas and APIs will change until the first tagged release.

## What you get

- **A ledger that can't drift.** Every credit movement is an immutable, double-entry posting. Balances
  are folded from the entry history, never stored and mutated. No overdraft and idempotent ingest are
  property-tested against every ledger backend.
- **Enforcement on the hot path.** Reserve a worst-case estimate before an agent call, settle the
  actuals after, void on failure. An out-of-budget agent is refused before it spends — no overdraft,
  even under concurrency.
- **Pricing that matches how models bill.** Multi-dimensional rate cards (input, output, cache read,
  cache write, reasoning, per-action) translate tokens → cost → credits with a fixed cash value and a
  margin. A dated catalog of provider prices ships in the box.
- **Edits without mutation.** Events are immutable. Correcting one records a new version plus a delta
  posting; killing a failed run reverses its holds and settlements. The audit trail is perfect, but the
  UX behaves as if you edited in place.
- **One place owns money.** The Rust engine is the sole owner of money-truth. The control plane, the
  dashboard, and the SDKs are clients — none of them compute a credit.

## Quickstart

Start the stores, run the engine, and charge a real model price against a funded account.

```bash
# 1. Postgres (money-truth) + ClickHouse (events)
docker compose -f deploy/dev/docker-compose.yml up -d

# 2. The engine — applies its Postgres and ClickHouse migrations on boot, serves on :8080
METER_DATABASE_URL=postgres://meter:meter@localhost:5432/meter \
METER_CLICKHOUSE_URL=http://localhost:8123 \
METER_CLICKHOUSE_USER=meter METER_CLICKHOUSE_PASSWORD=meter METER_CLICKHOUSE_DATABASE=meter \
  cargo run -p meter-engine
```

```bash
# 3. Open an account, fund it, and meter usage in one call
ORG=00000000-0000-0000-0000-000000000001

ACCT=$(curl -s localhost:8080/v1/accounts \
  -d "{\"org_id\":\"$ORG\",\"scope\":\"org\",\"no_overdraft\":true}" | jq -r .id)

curl -s localhost:8080/v1/accounts/$ACCT/grants \
  -d '{"amount":"1000","source":"paid"}' > /dev/null

# Price 1000 input + 500 output tokens against the catalogued Opus rate card, record the
# event, and charge credits — atomically and idempotently.
curl -s localhost:8080/v1/usage -d "{
  \"org_id\": \"$ORG\", \"account\": \"$ACCT\", \"model\": \"claude-opus-4-8\",
  \"idempotency_key\": \"turn-1\",
  \"usage\": { \"input_uncached\": 1000, \"output\": 500 }
}" | jq

curl -s localhost:8080/v1/accounts/$ACCT/balance | jq
```

The `usage` call returns the credits charged, the cost of goods, the customer price, the new balance,
and the recorded event id. Replaying it with the same `idempotency_key` is a no-op.

### Run the whole stack

Bring up Postgres, ClickHouse, the engine (`:8080`), the control plane (`:8090`), the dashboard
(`:3000`), and the docs site (`:3001`) — each service applies its own migrations on boot:

```bash
docker compose -f deploy/docker-compose.yml up --build
```

The dashboard is auth-gated; set `DASHBOARD_PASSWORD` and `DASHBOARD_SESSION_SECRET` (the compose ships
dev defaults). Browse the console at `http://localhost:3000` and the documentation at
`http://localhost:3001`.

## Architecture

meter splits along one hard seam: the data plane owns money, the control plane owns configuration.

| Component | Tech | Owns |
|---|---|---|
| **Engine** | Rust | The data plane and sole owner of money-truth: ingest, the double-entry ledger, reserve/settle enforcement, pricing. Serves HTTP (`:8080`) and gRPC (`:50051`). |
| **Control plane** | TypeScript · Effect + Drizzle | The management API the dashboard hits: orgs, products, API keys + RBAC, alert rules, notifications, signed webhooks. Computes no money — it calls the engine. |
| **Postgres** | — | Money-truth: the ledger, accounts, and the control-plane config schema. |
| **ClickHouse** | — | The usage-event firehose (system of record), the audit log, and analytics rollups — the high-velocity writes, kept off the money database. |
| **Dashboard** | Next.js · shadcn | Operator console: read views over the engine plus config CRUD against the control plane. |
| **SDKs** | TypeScript · Python | Drop-in instrumentation. The hot path (meter / reserve / settle) talks to the engine directly; config goes to the control plane. |

Money-truth lives only in the engine, so there is exactly one ledger and no cross-language drift. The
engine ⇄ control-plane contract is **protobuf/gRPC**; the control-plane ⇄ dashboard/customer contract is
**OpenAPI**. Both are generated, never hand-mirrored, and gated against drift in CI.

For the full reasoning — the storage choices, the enforcement mechanics, and the scale-out path behind
the `LedgerBackend` trait — read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and the
[ADRs](docs/adr/).

## Performance

The hot path is benchmarked with `criterion` on an Apple M5 Pro, single node:

| Path | What it measures | Median |
|---|---|---:|
| **Pricing** | A 5-dimension event → COGS → margin → credits, in memory, O(1) | **~191 ns** (~5.2 M ops/s/core) |
| **Reserve → settle** | A full credit reserve + settle with no-overdraft against **Postgres** | **~1.32 ms** |

```bash
cargo bench -p meter-pricing      # pricing hot path, no external dependencies
cargo bench -p meter-store-pg     # durable reserve/settle (spins a throwaway Postgres container)
```

meter's defining path is *synchronous, real-time enforcement* — reserve before a call, settle after,
refuse with no overdraft. That is a different operation from the *ingest-then-aggregate-then-bill*
pipelines of Lago, Metronome, and Orb, and two of those three are closed SaaS that can't be
independently load-tested. [docs/BENCHMARKS.md](docs/BENCHMARKS.md) sets the measured numbers against
each vendor's published claims, side by side and clearly labeled — no invented head-to-head.

## What works today

The Rust engine is functional and tested end to end against real Postgres and ClickHouse.

- **Ledger** — double-entry, append-only; grant / reserve / settle / void / refund; per-session leasing;
  balances derived; no overdraft under concurrency. Property- and conformance-tested against an
  in-memory reference and the Postgres backend.
- **Usage metering** — `POST /v1/usage` prices token usage against the catalog, records the event, and
  charges credits in one idempotent call. Token-priced reserve/settle and run governance too.
- **Pricing & catalog** — multi-dimensional rate cards with margin, versioned and re-rateable; a dated
  catalog of Anthropic, OpenAI, Google, DeepSeek, and Alibaba model prices; a pricing simulator.
- **Events** — immutable, custom-field events with `record` (idempotent), `amend` (a new version), and
  `void_run`; ClickHouse is the system of record (ADR 0003).
- **Invoicing** — a deterministic invoice summed straight from the ledger, so `enforced == billed`.
- **Control plane** — orgs, products, API keys with RBAC (viewer/member/admin) and platform/org scopes,
  alert rules with a budget-evaluation loop, and signed, retried webhooks with a dead-letter log.
- **Dashboard** — a full operator console: usage analytics, an events explorer with amend / void-run,
  accounts and ledger entries, invoices, the audit log, a rate-card catalog viewer, and a pricing
  simulator.
- **Docs site** — concepts, generated API references for both surfaces (rendered from the committed
  OpenAPI contracts), SDK guides, and self-host instructions, with client-side search.
- **Deployment** — engine, control-plane, dashboard, and docs images; a 6-service Docker Compose; a Helm
  chart; and a GHCR publish workflow.

In progress (see [tickets/](tickets/README.md)): the generated TypeScript gRPC client for the
control-plane → engine path, the event-amend delta posting (ADR 0009, proposed), RLS as
defense-in-depth, and more catalog providers.

## Self-hosting

The footprint is the **engine + control plane + Postgres + ClickHouse**. Redpanda, Redis, and a
TigerBeetle ledger backend are opt-in scale-out backends behind stable traits, each activated by a
measured trigger ([ADR 0005](docs/adr/0005-provider-scale-throughput.md)). Docker Compose for local and
dev; Helm for production, including air-gapped private-VPC deployment. The docs site `/self-host` page
is the full guide.

## Documentation

- [VISION](docs/VISION.md) — the problem and the product thesis.
- [ARCHITECTURE](docs/ARCHITECTURE.md) + [ADRs](docs/adr/) — the system design and every decision since.
- [DECISIONS](docs/DECISIONS.md) — the decision log at a glance.
- [SLO](docs/SLO.md) · [BENCHMARKS](docs/BENCHMARKS.md) — the performance contract and measured numbers.
- [SDKS](docs/SDKS.md) — the SDK strategy and provider adapters.
- [docs/](docs/README.md) — the index of everything above.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). meter is AGPL-3.0; contributions are by DCO sign-off
(`git commit -s`), not a CLA. The bar is enterprise quality: correct schemas, real migrations, full
tests, no shortcuts. Money is never a float and the ledger is append-only — see [CLAUDE.md](CLAUDE.md)
for the engineering standards CI enforces.

## License

[AGPL-3.0](LICENSE). Run, modify, and self-host meter freely. If you offer it to others over a network,
your modifications must be released under the same license.
