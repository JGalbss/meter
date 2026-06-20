//! `ClickHouse` DDL for the event store (ADR 0003) and the audit log (ADR 0004).
//!
//! `events` is the system of record for usage events; usage analytics are derived from it directly
//! (`FINAL` + `status = 'recorded'`, so amends and voids are reflected — see `lib.rs`). The append-only
//! `audit` firehose also lives here (ADR 0004): it is written on every mutating request, so it is
//! high-velocity and non-transactional — a ClickHouse fit, kept off the money database. Money-truth
//! lives in the engine's Postgres ledger (ADR 0001), never here.

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

/// `CREATE TABLE audit` — append-only log of mutating requests (ADR 0004). High-velocity and
/// never updated, so a plain `MergeTree` ordered by time; reads take the most recent rows.
pub const AUDIT: &str = "\
CREATE TABLE IF NOT EXISTS audit (
    id         UUID,
    actor      LowCardinality(String),
    method     LowCardinality(String),
    path       String,
    status     Int32,
    created_at DateTime64(3, 'UTC')
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(created_at)
ORDER BY (created_at, id)";
