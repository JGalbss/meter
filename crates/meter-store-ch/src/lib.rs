//! ClickHouse store for meter events + usage analytics.
//!
//! Per ADR 0003 this is the **system of record for events** (the firehose), not just analytics:
//! the editable event model (`EventStore`) lives here in `events`, alongside the `events_raw` usage
//! rollup source and the `events_dead_letter` queue. Money-truth stays in the Postgres ledger
//! (ADR 0001). Status changes are versioned rows in a `ReplacingMergeTree`; reads use `FINAL`.

#![forbid(unsafe_code)]

mod event_store;
mod schema;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use clickhouse::Client;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// A failure talking to ClickHouse.
#[derive(Debug, thiserror::Error)]
pub enum ChError {
    #[error("clickhouse: {0}")]
    Client(#[from] clickhouse::error::Error),
}

/// One usage event in the firehose. `version` orders duplicates for the `ReplacingMergeTree`
/// (highest wins); use the ingest time in milliseconds.
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EventRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub org_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub event_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub account_id: Uuid,
    pub meter: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
    pub credits: f64,
    #[serde(with = "clickhouse::serde::time::datetime64::millis")]
    pub ts: OffsetDateTime,
    pub version: u64,
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
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq)]
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

/// The ClickHouse store over a ClickHouse server.
#[derive(Clone)]
pub struct ChStore {
    client: Client,
    /// Strictly-increasing version source for the `events` ReplacingMergeTree (so a status change
    /// always supersedes the prior row). Seeded from the wall clock; monotonic within a process.
    version_seq: Arc<AtomicU64>,
}

impl ChStore {
    /// Connect to ClickHouse over its HTTP interface (e.g. `http://127.0.0.1:8123`).
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

    /// Apply the schema (idempotent): events, the usage rollup source, and the dead-letter queue.
    pub async fn migrate(&self) -> Result<(), ChError> {
        self.client.query(schema::EVENTS).execute().await?;
        self.client.query(schema::EVENTS_RAW).execute().await?;
        self.client
            .query(schema::EVENTS_DEAD_LETTER)
            .execute()
            .await?;
        Ok(())
    }

    /// Ingest a batch of usage events. Idempotent on `(org_id, event_id)`.
    pub async fn insert_events(&self, rows: &[EventRow]) -> Result<(), ChError> {
        let mut insert = self.client.insert("events_raw")?;
        for row in rows {
            insert.write(row).await?;
        }
        insert.end().await?;
        Ok(())
    }

    /// Usage aggregated by model for an organization (deduped via `FINAL`), highest spend first.
    pub async fn usage_by_model(&self, org_id: Uuid) -> Result<Vec<ModelUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT model, count() AS events, sum(input_tokens) AS input_tokens, \
                 sum(output_tokens) AS output_tokens, sum(credits) AS credits \
                 FROM events_raw FINAL WHERE org_id = ? GROUP BY model ORDER BY credits DESC",
            )
            .bind(org_id)
            .fetch_all::<ModelUsage>()
            .await?;
        Ok(rows)
    }

    /// Daily credit + event totals for an organization (deduped via `FINAL`), oldest day first.
    pub async fn usage_by_day(&self, org_id: Uuid) -> Result<Vec<DayUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT toString(toDate(ts)) AS day, count() AS events, sum(credits) AS credits \
                 FROM events_raw FINAL WHERE org_id = ? GROUP BY day ORDER BY day",
            )
            .bind(org_id)
            .fetch_all::<DayUsage>()
            .await?;
        Ok(rows)
    }

    /// Distinct event count for an organization (deduped via `FINAL`).
    pub async fn event_count(&self, org_id: Uuid) -> Result<u64, ChError> {
        let row = self
            .client
            .query("SELECT count() AS n FROM events_raw FINAL WHERE org_id = ?")
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
