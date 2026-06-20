# ADR 0004 — The audit log lives in ClickHouse

Status: accepted

## Context

The engine records an audit entry for every mutating request (actor, method, path, status, time).
Originally this lived in Postgres (`audit_log` table, `PgAuditLog`).

Two kinds of write are "high velocity," and they have **opposite** storage needs:

- **Money movements** (the ledger: reserve / settle / void / charge) are high-velocity *and* require
  ACID + row-level locking to guarantee no-overdraft. They must stay in a transactional store
  (Postgres today, a `LedgerBackend` like TigerBeetle for extreme throughput). This is ADR 0001.
- **Non-transactional firehoses** (usage events, the audit log) are high-velocity, append-only, never
  updated, and read analytically. They belong in ClickHouse. Usage events already moved there in
  ADR 0003.

The audit log is written on *every* mutating request — including the reserve/settle/usage hot path —
so it scales with the busiest traffic. Keeping it in Postgres put avoidable write load on the money
database for data that has none of money's transactional requirements.

## Decision

Move the audit log to ClickHouse. It is an append-only `MergeTree` table (`audit`), written
best-effort by the audit middleware and read newest-first by `GET /v1/audit`. Postgres now holds
**money + config only**; ClickHouse holds **events + audit** (the firehoses).

The engine uses a single `ChStore` for both events and the audit log (one ClickHouse connection,
distinct tables). `PgAuditLog`, the Postgres `audit_log` table, and its migration are removed — no
dead code.

## Consequences

- The money database carries only money-truth writes; the high-velocity audit stream is offloaded to
  the store built for it.
- The audit log is not transactionally coupled to the audited action — it never was (it is recorded
  best-effort after the response), so nothing is lost by the move.
- ClickHouse is required for the engine (already true since ADR 0003).
- `GET /v1/audit` is unchanged for callers; the dashboard audit view is unaffected.
