# Performance & SLO contract (v1)

> Phase-0 deliverable per `ARCHITECTURE.md` §13. These targets are the **design contract**, benchmarked
> from Phase 1 (the moment the ledger exists) under fault injection — not Phase-5 marketing. Numbers marked
> **TBD** are filled by the Phase-1/2 load + chaos harness against realistic skew, then published.
>
> Rule: every phase's exit criteria include hitting its slice of this contract **under fault injection**, so
> a wrong assumption surfaces in the phase that made it.

## Workload assumptions (state them, then test against them)

- **Zipfian tenant skew:** a handful of tenants are ~90% of total volume (agent workloads are extremely skewed).
- **`max_tokens` ≫ actual:** worst-case reservation is much larger than the settled actual; the overage tail is rare.
- **Bursty per-tenant concurrency:** many concurrent agent calls share one org/team credit pool → hot-account contention is the default, not the exception (hence per-session leasing in v1).
- **Late data:** usage can arrive after `event_time`; billing buckets on business time and self-corrects within the dispute window.

## Latency SLOs (hot path) — from ADR §5.2

| Operation | p50 | p99 | On store timeout / uncertainty |
|---|---|---|---|
| SOFT gate decision (leased) | < 0.3 ms | < 1.5 ms | fail-open, conservative local fallback |
| HARD reserve — Redis pre-check (accelerated) | < 0.5 ms | < 2 ms | proceed to durable gate |
| HARD reserve — durable hold (accelerated) | < 5 ms | < 25 ms | **fail-closed (DENY)** |
| HARD reserve — durable hold (Postgres default) | < 3 ms | < 15 ms | **fail-closed (DENY)** |
| settle | < 5 ms | < 25 ms | retry idempotently; never drop |

## Throughput targets

| Path | Sustained | Burst | Notes |
|---|---|---|---|
| **Event ingest — ClickHouse batch, ExactlyOnce (default), 1 node** | **≥ 600k events/s** | — | `record_batch`; cross-call dedup keeps the rollup exactly-once. Measured ~0.68M/node. |
| **Event ingest — ClickHouse batch, Append, 1 node** | **≥ 1M events/s** | — | dedup delegated upstream (Kafka EOS, ADR 0005) / reconciled. Measured ~1.0–1.1M/node. |
| Event ingest — provider scale (target) | ≥ 1M events/s | billions/day | reached single-node (Append) or by stateless-engine + CH-cluster fan-out (ExactlyOnce) |
| Ledger transfers/s — Postgres + leasing (Zipfian) | TBD | TBD | with vs. without leasing, both measured |
| Ledger transfers/s — TigerBeetle accelerator | TBD | TBD | only if it passes bill-equivalence |
| Invoice generation (tenant with N events) | TBD | — | deterministic recompute |

### Usage read latency (must stay flat as events grow to hundreds of millions)

Dashboard/credit reads (`usage_by_model`, `usage_by_day`, `event_count`) are served from the
pre-aggregated `usage_rollup` (a sign-weighted `SummingMergeTree` maintained by a materialized view),
so they are **O(rollup groups), not O(events)** — latency does not grow with the event firehose.

| Read | Target | Measured @ 1M events |
|---|---|---|
| `event_count` (org) | < 25 ms | ~2–3 ms |
| `usage_by_model` (org) | < 25 ms | ~3–6 ms |
| `usage_by_day` (org) | < 25 ms | ~2–4 ms |
| credit `balance` (account) | < 5 ms | O(1) by design† |

† Balance reads never sum the full ledger: `settled_credits` is a denormalized running balance on
`ledger_accounts`, and `held` sums only *open* holds via the partial index
`ledger_holds_open_idx … WHERE status = 'open'` — so a balance read stays constant-time even with
hundreds of millions of historical ledger entries. (User/team/product config reads are bounded sets,
not part of the firehose.)

> Method: `cargo test -p meter-store-ch --test throughput -- --ignored --nocapture` against a real
> ClickHouse container (single node, laptop). The harness sweeps batch-size × concurrency, reports
> ingest events/sec for both modes, measures the read aggregates, and **proves exactly-once by replaying
> the entire load with identical keys** (the live count is unchanged). `METER_BENCH_EVENTS=N` scales it.
> Baseline for contrast: the pre-batch single-event path measured ~1.1k events/sec.

## Correctness invariant (hard CI gate, from ADR §5.5)

> Under **N concurrent reservers with injected faults** — kill the durable store leader mid-reservation,
> restart from disk, partition Redis↔ledger, drop/duplicate settle callbacks, fire hold-timeout races —
> the **authoritative balance is never negative**, **no hold is leaked**, and **every settled call is
> charged exactly once.** Run against **both** ledger backends (Postgres default, TigerBeetle accelerator).

Plus: `SUM(credits in posted ledger transfers for period) == SUM(credits on the invoice)` per account per
period, to **0 micro-credits**, with a hard block on invoice finalization if they differ.

## Per-phase exit criteria (filled as phases land)

- **Phase 1 (ledger):** correctness invariant above is green in CI; HARD reserve p99 < 15 ms (Postgres default); per-session leasing converts hot-account writes to ≤ thousands/s at the target concurrency; outstanding-hold ceiling `concurrency × avg-reservation` stays well under a representative weekly budget.
- **Phase 2 (ingest):** effectively-once under duplicate/drop injection; dead-letter never silently drops; sustained outbox TPS target met; reconciliation job proves zero aggregate drift.
- **Phase 3 (pricing):** token→credit translation rounds once; re-rating a historical event stream is deterministic and byte-stable.
- **Phase 4 (invoicing):** enforced==billed reconciliation is exact; fragment invalidation recomputes only affected fragments.
