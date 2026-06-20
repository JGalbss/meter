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
```

Verified by an integration test against a real ClickHouse container (`testcontainers-modules`):
ingest → idempotent dedup via `FINAL` → aggregation by model.

## Pending

Minute/day `AggregatingMergeTree` materialized views, an `events_dead_letter` table, deterministic
re-rating (`INSERT … SELECT` partition-by-partition), and a query API surfaced over the control plane.
