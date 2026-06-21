# meter-store-ch

ClickHouse **analytics** store for meter usage events — an optional add-on for high-volume dashboards.
This store holds analytics only; **money-truth lives in the engine's Postgres ledger** (ADR 0001), so
credits here are `Float64` and aggregations are allowed to be approximate.

## Model

`events` is the **system of record** for usage events (ADR 0003): a `ReplacingMergeTree(version)` keyed
by `(org_id, id)` and partitioned by month. Re-ingesting the same `id` is **idempotent** — the highest
`version` wins after a merge, and `FINAL` forces dedup at query time. A status change (amend → `amended`;
`void_run` → `voided`) is a new, higher-version row, so reads resolve the latest version of each event.

Analytics are derived from `events` directly. Two pre-aggregated `SummingMergeTree` rollups, each
maintained by a materialized view and **sign-weighted** so amends/voids cancel exactly (no `FINAL` scan
or read-time JSON parsing), keep the hot reads fast:

- `usage_rollup` — keyed `(org_id, meter, model, day)`; powers `usage_by_model` / `usage_by_day` /
  `event_count`.
- `field_usage_rollup` — keyed `(org_id, field_name, field_value, day)`; powers flexible credit
  **burndown** (`usage_by_field`) over the *promoted* custom fields (`schema::PROMOTED_FIELDS`).
  Burndown by any other field still works via a direct `events`-scan path; the two are equivalent.

## API

```rust
let store = ChStore::new("http://clickhouse:8123");
store.migrate().await?;                            // events + rollups + MVs + dead-letter + audit
store.record(event).await?;                        // ingest one event (idempotent on org_id + key)
let by_model = store.usage_by_model(org).await?;   // rollup, highest spend first
let by_day = store.usage_by_day(org).await?;       // daily credit/event time series (for charts)
let by_team = store.usage_by_field(org, "team").await?;  // flexible burndown by any custom field
let n = store.event_count(org).await?;             // distinct events (deduped)

store.record_dead_letter(&failed).await?;          // events that failed validation/ingest
let dead = store.list_dead_letter(org).await?;     // inspect / replay
```

## EventStore (ADR 0003)

`ChStore` also implements `meter_event::EventStore` — the **system of record for events**. The editable
model (record / get / list / amend / `void_run`, custom-field `properties`) lives in the `events`
`ReplacingMergeTree`: a status change is a new row with the same `id` and a higher `version`, and reads
use `FINAL` to resolve the latest version. It passes the **same** event conformance suite as the
in-memory reference, against a real ClickHouse container.


Verified by an integration test against a real ClickHouse container (`testcontainers-modules`):
ingest → idempotent dedup via `FINAL` → aggregation by model.

## Pending

Deterministic historical re-rating (`INSERT … SELECT` partition-by-partition, sourced from per-event
properties — not the rollup, which collapses cache dimensions), and minute-granularity time-series
rollups for live dashboards.

Shipped: the `usage_rollup` (model/day) and `field_usage_rollup` (custom-field burndown) `SummingMergeTree`
views, the `events_dead_letter` queue, and the query API surfaced over the engine HTTP + gRPC surfaces.
