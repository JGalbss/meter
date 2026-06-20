//! `ClickHouse` store for meter events + usage analytics.
//!
//! Per ADR 0003 this is the **system of record for events**: the editable event model (`EventStore`)
//! lives here in `events`, alongside the `events_dead_letter` queue. Usage analytics are derived
//! directly from `events` (`FINAL` + `status = 'recorded'`), so amends and voids are reflected
//! without a second source to keep in sync. Money-truth stays in the Postgres ledger (ADR 0001).
//! Status changes are versioned rows in a `ReplacingMergeTree`; reads use `FINAL`.

#![forbid(unsafe_code)]

mod event_store;
mod schema;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clickhouse::Client;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// A failure talking to `ClickHouse`.
#[derive(Debug, thiserror::Error)]
pub enum ChError {
    #[error("clickhouse: {0}")]
    Client(#[from] clickhouse::error::Error),
}

/// Usage aggregated by model for one organization.
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    pub events: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

/// Daily usage totals for an organization (a time series for charts). `day` is `YYYY-MM-DD`.
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DayUsage {
    pub day: String,
    pub events: u64,
    pub credits: f64,
}

/// An event that failed validation/ingest, kept for inspection and replay.
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    #[serde(with = "clickhouse::serde::uuid")]
    pub id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub org_id: Uuid,
    pub source: String,
    pub payload: String,
    pub error: String,
    #[serde(with = "clickhouse::serde::time::datetime64::millis")]
    pub received_at: OffsetDateTime,
}

#[derive(clickhouse::Row, Deserialize)]
struct CountRow {
    n: u64,
}

/// One recorded mutating action (ADR 0004). Returned by reads; serialized for the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub actor: String,
    pub method: String,
    pub path: String,
    pub status: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// The `audit` row as stored in ClickHouse (RowBinary codecs); converted to [`AuditEntry`] on read.
#[derive(clickhouse::Row, Serialize, Deserialize)]
struct AuditRow {
    #[serde(with = "clickhouse::serde::uuid")]
    id: Uuid,
    actor: String,
    method: String,
    path: String,
    status: i32,
    #[serde(with = "clickhouse::serde::time::datetime64::millis")]
    created_at: OffsetDateTime,
}

fn audit_row_to_entry(row: AuditRow) -> AuditEntry {
    AuditEntry {
        id: row.id.to_string(),
        actor: row.actor,
        method: row.method,
        path: row.path,
        status: row.status,
        created_at: row.created_at,
    }
}

/// How the ingest path enforces per-event idempotency (ADR 0005). The `events` system of record is
/// always idempotent on read (a `ReplacingMergeTree` dedups on `(org_id, id)`); this lever only
/// controls how the **pre-aggregated `usage_rollup`** is kept exactly-once at write time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum IngestMode {
    /// Default. A batch's already-recorded events are filtered out before insert (one index lookup
    /// per org), so an idempotent retry never adds a second contribution to the rollup. Correct out
    /// of the box; the dedup read costs throughput and grows with table size.
    #[default]
    ExactlyOnce,
    /// Append-only: skip the cross-call dedup read for maximum throughput (events are still deduped
    /// *within* a batch). Safe when ingest is made exactly-once upstream (a Redpanda/Kafka EOS buffer,
    /// per ADR 0005) or when the rollup is periodically reconciled from the `events` SoR. The SoR
    /// itself stays idempotent regardless of this setting.
    Append,
}

/// The `ClickHouse` store over a `ClickHouse` server.
#[derive(Clone)]
pub struct ChStore {
    client: Client,
    /// Strictly-increasing version source for the `events` `ReplacingMergeTree` (so a status change
    /// always supersedes the prior row). Seeded from the wall clock; monotonic within a process.
    version_seq: Arc<AtomicU64>,
    ingest_mode: IngestMode,
}

impl ChStore {
    /// Connect to `ClickHouse` over its HTTP interface (e.g. `http://127.0.0.1:8123`).
    pub fn new(url: &str) -> Self {
        let seed =
            u64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000).unwrap_or(0);
        Self {
            client: Client::default().with_url(url),
            version_seq: Arc::new(AtomicU64::new(seed)),
            ingest_mode: IngestMode::default(),
        }
    }

    /// Select the ingest idempotency mode (see [`IngestMode`]). Defaults to [`IngestMode::ExactlyOnce`].
    #[must_use]
    pub fn with_ingest_mode(mut self, mode: IngestMode) -> Self {
        self.ingest_mode = mode;
        self
    }

    /// The configured ingest idempotency mode.
    #[must_use]
    pub fn ingest_mode(&self) -> IngestMode {
        self.ingest_mode
    }

    /// The next strictly-increasing version for an `events` row.
    fn next_version(&self) -> u64 {
        self.version_seq.fetch_add(1, Ordering::SeqCst)
    }

    /// Apply the schema (idempotent): the events system of record, its pre-aggregated usage rollup
    /// (+ the materialized view that maintains it), the dead-letter queue, and the audit log.
    pub async fn migrate(&self) -> Result<(), ChError> {
        self.client.query(schema::EVENTS).execute().await?;
        self.client.query(schema::USAGE_ROLLUP).execute().await?;
        self.client.query(schema::USAGE_ROLLUP_MV).execute().await?;
        self.client
            .query(schema::EVENTS_DEAD_LETTER)
            .execute()
            .await?;
        self.client.query(schema::AUDIT).execute().await?;
        Ok(())
    }

    /// Readiness check: confirm `ClickHouse` is reachable and answering queries.
    pub async fn ping(&self) -> Result<(), ChError> {
        self.client.query("SELECT 1").execute().await?;
        Ok(())
    }

    /// Append an audit entry (ADR 0004). High-velocity, best-effort — kept off the money database.
    pub async fn record_audit(
        &self,
        actor: &str,
        method: &str,
        path: &str,
        status: i32,
    ) -> Result<(), ChError> {
        let row = AuditRow {
            id: Uuid::now_v7(),
            actor: actor.to_owned(),
            method: method.to_owned(),
            path: path.to_owned(),
            status,
            created_at: OffsetDateTime::now_utc(),
        };
        let mut insert = self.client.insert("audit")?;
        insert.write(&row).await?;
        insert.end().await?;
        Ok(())
    }

    /// The most recent audit entries, newest first.
    pub async fn list_audit(&self, limit: i64) -> Result<Vec<AuditEntry>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT id, actor, method, path, status, created_at \
                 FROM audit ORDER BY created_at DESC, id DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all::<AuditRow>()
            .await?;
        Ok(rows.into_iter().map(audit_row_to_entry).collect())
    }

    /// Usage aggregated by model for an organization, highest spend first.
    ///
    /// Reads the pre-aggregated [`usage_rollup`](schema::USAGE_ROLLUP), so it is sub-linear in event
    /// count (no `FINAL` scan, no read-time JSON parsing). The sign-weighted rollup equals the live
    /// (`status = 'recorded'`) aggregate: amended events count once at their corrected version and
    /// voided runs drop out. Events without a `model` (arbitrary custom meters) are excluded here.
    pub async fn usage_by_model(&self, org_id: Uuid) -> Result<Vec<ModelUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT model, toUInt64(events) AS events, \
                 toUInt64(input_tokens) AS input_tokens, \
                 toUInt64(output_tokens) AS output_tokens, credits FROM ( \
                 SELECT model, \
                 sum(events) AS events, \
                 sum(input_tokens) AS input_tokens, \
                 sum(output_tokens) AS output_tokens, \
                 sum(credits) AS credits \
                 FROM usage_rollup \
                 WHERE org_id = ? AND model != '' \
                 GROUP BY model) \
                 WHERE events > 0 ORDER BY credits DESC",
            )
            .bind(org_id)
            .fetch_all::<ModelUsage>()
            .await?;
        Ok(rows)
    }

    /// Daily credit + event totals for an organization, oldest day first.
    ///
    /// Like [`Self::usage_by_model`], reads the pre-aggregated rollup (amends/voids reflected via the
    /// sign-weighting). `events` counts every live event that day across all meters; `credits` sums
    /// their recorded credit cost.
    pub async fn usage_by_day(&self, org_id: Uuid) -> Result<Vec<DayUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT day, toUInt64(events) AS events, credits FROM ( \
                 SELECT toString(day) AS day, sum(events) AS events, \
                 sum(credits) AS credits \
                 FROM usage_rollup \
                 WHERE org_id = ? \
                 GROUP BY day) \
                 WHERE events > 0 ORDER BY day",
            )
            .bind(org_id)
            .fetch_all::<DayUsage>()
            .await?;
        Ok(rows)
    }

    /// Count of an organization's live events, from the pre-aggregated rollup (sign-weighted, so
    /// amended/voided events net to zero) — sub-linear in event count.
    pub async fn event_count(&self, org_id: Uuid) -> Result<u64, ChError> {
        let row = self
            .client
            .query("SELECT toUInt64(sum(events)) AS n FROM usage_rollup WHERE org_id = ?")
            .bind(org_id)
            .fetch_one::<CountRow>()
            .await?;
        Ok(row.n)
    }

    /// Record events that failed validation/ingest into the dead-letter table.
    pub async fn record_dead_letter(&self, rows: &[DeadLetter]) -> Result<(), ChError> {
        let mut insert = self.client.insert("events_dead_letter")?;
        for row in rows {
            insert.write(row).await?;
        }
        insert.end().await?;
        Ok(())
    }

    /// List an organization's dead-lettered events, newest first.
    pub async fn list_dead_letter(&self, org_id: Uuid) -> Result<Vec<DeadLetter>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT id, org_id, source, payload, error, received_at \
                 FROM events_dead_letter WHERE org_id = ? ORDER BY received_at DESC",
            )
            .bind(org_id)
            .fetch_all::<DeadLetter>()
            .await?;
        Ok(rows)
    }

    /// Count an organization's dead-lettered events.
    pub async fn dead_letter_count(&self, org_id: Uuid) -> Result<u64, ChError> {
        let row = self
            .client
            .query("SELECT count() AS n FROM events_dead_letter WHERE org_id = ?")
            .bind(org_id)
            .fetch_one::<CountRow>()
            .await?;
        Ok(row.n)
    }
}
