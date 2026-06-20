# ADR 0005 — Scaling to provider-grade throughput

Status: accepted (architecture); implementation tracked in `/tickets` (EPIC 16)

## Context

meter must scale to the volume of the largest model providers — on the order of **millions of metering
operations per second** and **billions of usage events per day**. We want that ceiling without
sacrificing the two things that make meter trustworthy: the ledger is sacred (money is never lost,
double-counted, or overspent) and the system stays simple. This ADR records *how* we get there so we
don't paint ourselves into a corner.

## The one true bottleneck

Almost everything in the hot path scales horizontally and is already on the right substrate:

- **Pricing** is pure, in-memory, ~150 ns/call. Not a bottleneck.
- **Events + audit** are append-only firehoses on ClickHouse (ADR 0003/0004), which ingests millions
  of rows/sec per node and scales out as a cluster. Not a bottleneck.
- **The engine** is stateless HTTP — add replicas behind a load balancer.

The bottleneck is the **transactional ledger**: reserve/settle/charge must be ACID with no-overdraft,
which on Postgres means serializing on the account row (`SELECT … FOR UPDATE`). A single hot account is
a single serialization point. This is the deliberate cost of correctness — money cannot live on an
eventually-consistent store. Scaling money throughput is therefore the whole game, and we attack it on
two axes: **remove round-trips** and **swap in a faster transactional backend** — both behind the
existing `LedgerBackend` trait, so the rest of the system never changes (this is where simplicity is
preserved: one seam, swappable implementations).

## Decision — the scaling path, in order of leverage

1. **Per-session leasing (done).** A session leases a block of credits from its parent once, then
   reserves/settles against its own child account locally. This turns "one ledger round-trip per token"
   into "one lease per session," and spreads contention across per-session rows instead of one hot
   parent. This is the single biggest win for agent workloads and is already implemented + conformance-
   tested.

2. **TigerBeetle `LedgerBackend` (the extreme-throughput money store).** TigerBeetle is a purpose-built
   double-entry accounting database doing ~1M+ transfers/sec with built-in no-overdraft. meter's model
   maps onto it directly:

   | meter | TigerBeetle |
   | --- | --- |
   | account (no-overdraft) | account with `flags.debits_must_not_exceed_credits` |
   | grant | transfer: funding → account |
   | reserve | **pending** transfer (two-phase) |
   | settle(actual) | **post_pending** transfer for `actual` (voids the remainder) |
   | void | **void_pending** transfer |
   | charge | posted transfer |
   | lease open/close | conserving transfers parent ↔ session account |
   | idempotency key | transfer `id` / `user_data` (TB rejects duplicate ids) |

   Credits are represented as fixed-point **integers** (`credit × 10^scale`, `u128`) — TB amounts are
   integers, which also removes any float risk. no-overdraft is enforced *by the database*, not by a
   lock we hold. Client: `tigerbeetle-unofficial` (the official Rust client is still a placeholder).
   The backend is proven against the **same** `meter_ledger::conformance` suite as the in-memory oracle
   and Postgres (no-overdraft + idempotency are property-tested identically), so swapping it in is safe.

3. **Firehose scaling.** Events/audit already batch well on ClickHouse; under extreme load use async
   (server-batched) inserts and, if spikes outrun ClickHouse, an optional Redpanda/Kafka buffer in
   front of ingest (behind the `IngestSource` trait) to absorb bursts and replay.

4. **Horizontal scale.** The engine is stateless: scale replicas freely. Postgres/TigerBeetle own the
   money; ClickHouse runs as a cluster. Per-org sharding of the money store is the final lever if a
   single TB cluster is ever the limit.

5. **Benchmarks as SLO gates.** criterion micro-benchmarks (pricing, pg reserve+settle) plus a
   concurrent throughput harness (shared vs leased accounts) report ops/sec and guard against
   regressions. We publish target SLOs and fail CI on regressions, so throughput is measured, not
   assumed.

## Consequences

- The path to provider scale is concrete and backend-swappable; choosing Postgres vs TigerBeetle is a
  deployment decision, not a rewrite — the API, SDKs, and control plane are unaffected.
- Correctness is never traded for speed: every backend passes the same conformance suite, and money
  stays on a transactional store.
- Simplicity holds: the `LedgerBackend` / `EventStore` / `IngestSource` traits are the only seams; each
  scaling lever lives behind one of them.
