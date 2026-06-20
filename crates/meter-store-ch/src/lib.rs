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

/// The `ClickHouse` store over a `ClickHouse` server.
#[derive(Clone)]
pub struct ChStore {
    client: Client,
    /// Strictly-increasing version source for the `events` `ReplacingMergeTree` (so a status change
    /// always supersedes the prior row). Seeded from the wall clock; monotonic within a process.
    version_seq: Arc<AtomicU64>,
}

impl ChStore {
    /// Connect to `ClickHouse` over its HTTP interface (e.g. `http://127.0.0.1:8123`).
    pub fn new(url: &str) -> Self {
        let seed =
            u64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000).unwrap_or(0);
        Self {
            client: Client::default().with_url(url),
            version_seq: Arc::new(AtomicU64::new(seed)),
        }
    }

    /// The next strictly-increasing version for an `events` row.
    fn next_version(&self) -> u64 {
        self.version_seq.fetch_add(1, Ordering::SeqCst)
    }

    /// Apply the schema (idempotent): the events system of record, dead-letter queue, and audit log.
    pub async fn migrate(&self) -> Result<(), ChError> {
        self.client.query(schema::EVENTS).execute().await?;
        self.client
            .query(schema::EVENTS_DEAD_LETTER)
            .execute()
            .await?;
        self.client.query(schema::AUDIT).execute().await?;
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

    /// Ingest a batch of usage events. Idempotent on `(org_id, event_id)`.
    /// Usage aggregated by model for an organization, highest spend first.
    ///
    /// Derived from the live event set (`FINAL` + `status = 'recorded'`): amended events count once
    /// (as their corrected version) and voided runs are excluded. Token and credit figures are read
    /// from the usage event's JSON `properties` (the shape the metering path records); events without
    /// a `model` (arbitrary custom meters) are not part of model usage.
    pub async fn usage_by_model(&self, org_id: Uuid) -> Result<Vec<ModelUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT JSONExtractString(properties, 'model') AS model, \
                 count() AS events, \
                 sum(JSONExtractUInt(properties, 'input_uncached') \
                     + JSONExtractUInt(properties, 'cache_read') \
                     + JSONExtractUInt(properties, 'cache_write')) AS input_tokens, \
                 sum(JSONExtractUInt(properties, 'output') \
                     + JSONExtractUInt(properties, 'reasoning')) AS output_tokens, \
                 sum(toFloat64OrZero(JSONExtractString(properties, 'credits'))) AS credits \
                 FROM events FINAL \
                 WHERE org_id = ? AND status = 'recorded' AND JSONHas(properties, 'model') \
                 GROUP BY model ORDER BY credits DESC",
            )
            .bind(org_id)
            .fetch_all::<ModelUsage>()
            .await?;
        Ok(rows)
    }

    /// Daily credit + event totals for an organization, oldest day first.
    ///
    /// Like [`Self::usage_by_model`], derived from the live event set so amends and voids are
    /// reflected. `events` counts every recorded event that day; `credits` sums the usage events'
    /// recorded credit cost.
    pub async fn usage_by_day(&self, org_id: Uuid) -> Result<Vec<DayUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT toString(toDate(event_time)) AS day, count() AS events, \
                 sum(toFloat64OrZero(JSONExtractString(properties, 'credits'))) AS credits \
                 FROM events FINAL \
                 WHERE org_id = ? AND status = 'recorded' \
                 GROUP BY day ORDER BY day",
            )
            .bind(org_id)
            .fetch_all::<DayUsage>()
            .await?;
        Ok(rows)
    }

    /// Count of an organization's live events (`FINAL` + `status = 'recorded'`).
    pub async fn event_count(&self, org_id: Uuid) -> Result<u64, ChError> {
        let row = self
            .client
            .query("SELECT count() AS n FROM events FINAL WHERE org_id = ? AND status = 'recorded'")
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
