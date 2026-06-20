# meter-store-ch

ClickHouse **analytics** store for meter usage events — an optional add-on for high-volume dashboards.
This store holds analytics only; **money-truth lives in the engine's Postgres ledger** (ADR 0001), so
credits here are `Float64` and aggregations are allowed to be approximate.

## Model

`events_raw` is the usage firehose: a `ReplacingMergeTree(version)` keyed by `(org_id, event_id)` and
partitioned by month. Re-ingesting the same `event_id` is **idempotent** — the highest `version` wins
after a merge, and `FINAL` forces dedup at query time.

## API

```rust
let store = ChStore::new("http://clickhouse:8123");
store.migrate().await?;                 // CREATE TABLE IF NOT EXISTS events_raw
store.insert_events(&rows).await?;      // batch ingest (idempotent on org_id + event_id)
let by_model = store.usage_by_model(org).await?;  // rollup, highest spend first
let by_day = store.usage_by_day(org).await?;      // daily credit/event time series (for charts)
let n = store.event_count(org).await?;            // distinct events (deduped)

store.record_dead_letter(&failed).await?;         // events that failed validation/ingest
let dead = store.list_dead_letter(org).await?;    // inspect / replay
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

Minute/day `AggregatingMergeTree` materialized views, an `events_dead_letter` table, deterministic
re-rating (`INSERT … SELECT` partition-by-partition), and a query API surfaced over the control plane.
