# Benchmarks

This page reports **measured, reproducible** numbers for the meter engine's hot path, and sets them
against the **published** characteristics of comparable billing/metering systems (Lago, Metronome,
Orb). Read the [methodology](#methodology) and [why this is not a head-to-head](#why-this-is-not-a-head-to-head)
sections before quoting anything here — "speed" means different things for a synchronous enforcement
engine than it does for an asynchronous aggregate-then-bill pipeline.

## meter — measured

Single node, no network between client and engine. Hardware: Apple M5 Pro (18 cores), macOS;
Rust 1.88, `criterion` 0.5. Numbers are criterion medians; re-run them with the commands below.

| Path | What it measures | Median | Throughput (1 core) |
|---|---|---:|---:|
| **Pricing** (`meter-pricing`) | A realistic 5-dimension event (input / cache-read / cache-write / output / reasoning) → COGS → margin → credits, in memory | **~191 ns** | **~5.2 M ops/s** |
| **Pricing, cost only** | The same event priced to COGS without the credit translation | ~174 ns | ~5.7 M ops/s |
| **Reserve → settle, durable** (`meter-store-pg`) | A full `reserve` + `settle` round trip with no-overdraft enforcement against **Postgres** (indexed idempotency, settled balance on the account row) | **~1.32 ms** | — |

Notes:
- Pricing is the per-event CPU cost on the metering path and is **O(1)** in ledger history.
- The reserve→settle figure is the **durable money path**: two real DB round trips to a local
  Postgres container (testcontainers, loopback). It includes container + loopback overhead, so it is
  a conservative latency, not a tuned-deployment ceiling. The architecture amortizes this round trip
  with per-session **leasing** (one round trip per session, not per token) and a pluggable
  `LedgerBackend` (e.g. a TigerBeetle backend) — see [ADR 0005](adr/0005-provider-scale-throughput.md).
- The in-memory `LedgerBackend` is a **correctness reference**, not a performance target (it scans
  history for idempotency), so it is deliberately **not** reported as a throughput number.

### Reproduce

```bash
# Pricing hot path (no external dependencies)
cargo bench -p meter-pricing

# Durable reserve/settle against Postgres (needs Docker; spins a throwaway container)
cargo bench -p meter-store-pg
```

Criterion writes full reports (including distributions) to `target/criterion/`.

## Comparison

There is no honest way to publish a single "meter is N× faster than Lago/Metronome/Orb" number, and
this page does not. The systems are architecturally different and two of the three cannot be
self-hosted or load-tested by a third party. What can be compared honestly is **architecture** and
each vendor's **own published** figures.

| | **meter** | **Lago** | **Metronome** | **Orb** |
|---|---|---|---|---|
| Model | Open source, self-hosted | Open source, self-hosted | Closed managed SaaS | Closed managed SaaS |
| Core engine | Rust | Ruby on Rails | Managed (Kafka/Confluent pipeline) | Managed (query-based) |
| Hot-path model | **Synchronous** reserve→settle on the request path | Async ingest → Sidekiq/ClickHouse aggregation | Async ingest → Kafka Streams → hot store → invoice service | Async ingest → query-based billing over raw events |
| Real-time credit enforcement (block an over-budget agent mid-request, no overdraft) | **Yes** — double-entry ledger on the path | No (aggregate-then-bill) | No (aggregate-then-bill) | No (aggregate-then-bill) |
| Self-hostable / independently benchmarkable | Yes | Yes | No | No |
| Vendor-published throughput | *Measured here:* ~5.2 M pricing ops/s/core; ~1.32 ms durable reserve→settle | "up to 1,000,000 billing events/sec" ingest | "100,000+ events/sec" ingest; "10,000+ invoices/sec" | "up to 250,000+ events/sec" ingest |

The throughput cells are **not** like-for-like:

- meter's figures are **measured single-node** numbers for the **synchronous priced-enforcement**
  path (price an event; reserve and settle credits with no-overdraft). They are reproducible with the
  commands above.
- The Lago / Metronome / Orb figures are the **vendors' own marketing claims** for **asynchronous
  event ingest** across a managed/scaled fleet — a different operation (accept an event now,
  aggregate and bill later), not a synchronous per-request enforcement, and not a single-node number.
  They are cited, not verified by us.

### Why this is not a head-to-head

- **Different operation.** meter's defining path is *real-time enforcement*: before an agent call it
  reserves credits and after it settles actuals, refusing the call if the budget is exhausted — on
  the request path, with double-entry no-overdraft guarantees. Lago, Metronome, and Orb are
  fundamentally *ingest-then-aggregate-then-bill*: you stream usage events and they are aggregated and
  priced asynchronously. You cannot block an over-budget request inside an async aggregation pipeline,
  so "events/sec ingested" and "ms to reserve+settle" are not the same quantity.
- **Two of three are closed SaaS.** Metronome and Orb cannot be self-hosted, so a third party cannot
  run them on identical hardware with an identical workload. Any "we benchmarked them" number would be
  fabricated. Lago *is* open source and self-hostable — a fair, self-hosted Lago-vs-meter load test is
  the one extension of this page that would be legitimate, and is left as future work (it needs a
  workload both systems implement identically, e.g. ingest + aggregate, since meter's reserve/settle
  has no Lago equivalent).
- **Vendor claims are marketing, not SLAs.** The cited events/sec figures come from vendor blogs and
  product pages; they describe peak managed-fleet ingest, not a documented per-operation latency SLA.

## Sources

- Lago — [Using ClickHouse to scale an events engine](https://clickhouse.com/blog/lago-using-clickhouse-to-scale-an-events-engine),
  [How to architect billing systems](https://getlago.com/blog/architect-billing-systems)
- Metronome — [Real-time billing with Confluent](https://www.confluent.io/customers/metronome/),
  [How Metronome works](https://docs.metronome.com/guides/get-started/how-metronome-works)
- Orb — [Metering](https://www.withorb.com/products/metering),
  [Query-based billing](https://docs.withorb.com/architecture/query-based-billing)
