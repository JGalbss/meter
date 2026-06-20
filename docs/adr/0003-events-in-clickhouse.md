# ADR 0003 — Events live in ClickHouse, not Postgres

**Status:** Accepted (2026-06-20). **Amends** `ARCHITECTURE.md` §4 (ingest) and ADR 0002 (event model).

## Context

Usage events are the firehose: every agent call emits one (often many), at a volume orders of
magnitude above ledger postings. Postgres is the right home for **money-truth** — the double-entry
ledger, balances, idempotency — where transactional integrity is sacred and volume is modest. It is
the wrong home for the event firehose: high-cardinality, append-heavy, analytically-queried data that
would bloat the OLTP primary, contend with the hot enforcement path, and make the rollups the
dashboard needs (usage by model/day/minute) slow.

ADR 0002 defined the editable event model (custom fields, amend-as-new-version, `void_run`,
latest-non-voided reads). It did not pin the storage engine; the interim implementation put events in
Postgres (`meter-event` + `PgEventStore`) to get the model right against a familiar store.

## Decision

**Events live in ClickHouse. Postgres holds only money-truth (the ledger) and control-plane config.**

- The `EventStore` trait (record / get / list / amend / void_run / latest-non-voided) gains a
  **ClickHouse backend** in `meter-store-ch`, which becomes the default and only production event store.
  `PgEventStore` is retired once the ClickHouse backend passes the shared event conformance suite.
- The editable model maps cleanly onto a `ReplacingMergeTree`: an amend is a new row with a higher
  version superseding the prior `event_id`; `void_run` writes voiding rows; "latest non-voided" reads
  use `FINAL` (or `argMax(version)`) + a status filter. This is the same idempotent-dedup mechanism
  already used by `events_raw`.
- The engine's ingest + `/v1/usage` path writes the usage event to ClickHouse (the firehose) and posts
  the credit movement to the Postgres ledger (money-truth). The two are decoupled: ingest is
  effectively-once into ClickHouse; the ledger is the system of record for credits.
- ClickHouse is therefore a **required** component of a metering deployment (not the optional
  analytics-only add-on it was under the interim design). The dev/prod compose run it by default.

## Consequences

- The double-entry ledger and its conservation/no-overdraft invariants are unchanged and remain in
  Postgres — this ADR does not touch money-truth.
- Reconciliation (ADR theme): the ledger (Postgres) and the event firehose (ClickHouse) are reconciled
  by job — settled credits vs priced events — rather than living in one store.
- Late/duplicate events self-correct via ReplacingMergeTree versioning, as today.

## Migration plan

1. Implement `EventStore` over ClickHouse in `meter-store-ch`; run the shared `meter-event`
   conformance suite against it (the in-memory reference stays the oracle).
2. Switch the engine (`meter-api`/`meter-engine`) to the ClickHouse `EventStore`; wire `/v1/usage` and
   `/v1/events` to it; keep the ledger on Postgres.
3. Make ClickHouse required in compose + self-host docs; remove `PgEventStore` from the engine wiring.
4. Reconciliation job: ledger settles vs ClickHouse priced events.

Tracked in `/tickets` EPIC 06 (ingest & event model) and EPIC 07 (ClickHouse).
