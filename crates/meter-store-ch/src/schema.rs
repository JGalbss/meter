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

/// `CREATE TABLE events` — the editable event model (ADR 0002/0003): the system of record for usage
/// events. A `ReplacingMergeTree(version)` keyed by `(org_id, id)`, so a status change (amend → the
/// original becomes `amended`; `void_run` → `voided`) is a new row with a higher version; reads use
/// `FINAL` to see the latest version of each event id. `properties` is the customer's JSON.
pub const EVENTS: &str = "\
CREATE TABLE IF NOT EXISTS events (
    id              UUID,
    org_id          UUID,
    idempotency_key String,
    event_time      DateTime64(3, 'UTC'),
    meter           LowCardinality(String),
    account_id      UUID,
    run_id          Nullable(UUID),
    properties      String,
    status          LowCardinality(String),
    supersedes      Nullable(UUID),
    created_at      DateTime64(3, 'UTC'),
    version         UInt64
)
ENGINE = ReplacingMergeTree(version)
PARTITION BY toYYYYMM(event_time)
ORDER BY (org_id, id)";

/// `CREATE TABLE events_dead_letter` — events that failed validation/ingest, kept for inspection and
/// replay (the raw payload plus the error).
pub const EVENTS_DEAD_LETTER: &str = "\
CREATE TABLE IF NOT EXISTS events_dead_letter (
    id          UUID,
    org_id      UUID,
    source      LowCardinality(String),
    payload     String,
    error       String,
    received_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(received_at)
ORDER BY (org_id, received_at)";
