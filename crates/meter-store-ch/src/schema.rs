//! ClickHouse DDL for the analytics store.
//!
//! `events_raw` is the usage firehose. It is a `ReplacingMergeTree` keyed by `(org_id, event_id)`
//! with a `version` column, so re-ingesting the same event id is idempotent (the highest version
//! wins after a merge; `FINAL` forces dedup at query time). This store holds **analytics only** —
//! money-truth lives in the engine's Postgres ledger (ADR 0001), so credits here are `Float64`.

/// `CREATE TABLE events_raw` — the usage event firehose.
pub const EVENTS_RAW: &str = "\
CREATE TABLE IF NOT EXISTS events_raw (
    org_id        UUID,
    event_id      UUID,
    account_id    UUID,
    meter         LowCardinality(String),
    model         LowCardinality(String),
    input_tokens  UInt64,
    output_tokens UInt64,
    cache_read    UInt64,
    cache_write   UInt64,
    reasoning     UInt64,
    credits       Float64,
    ts            DateTime64(3, 'UTC'),
    version       UInt64
)
ENGINE = ReplacingMergeTree(version)
PARTITION BY toYYYYMM(ts)
ORDER BY (org_id, event_id)";
