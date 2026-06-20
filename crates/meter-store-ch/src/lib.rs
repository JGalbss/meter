//! ClickHouse analytics store for meter usage events (optional add-on).
//!
//! High-volume usage firehose + rollup queries for dashboards. This store is **analytics only** —
//! money-truth lives in the engine's Postgres ledger (ADR 0001). Ingest is idempotent: `events_raw`
//! is a `ReplacingMergeTree` keyed by `(org_id, event_id)`, so re-sending an event id dedups.

#![forbid(unsafe_code)]

mod schema;

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

#[derive(clickhouse::Row, Deserialize)]
struct CountRow {
    n: u64,
}

/// The analytics store over a ClickHouse server.
#[derive(Clone)]
pub struct ChStore {
    client: Client,
}

impl ChStore {
    /// Connect to ClickHouse over its HTTP interface (e.g. `http://127.0.0.1:8123`).
    pub fn new(url: &str) -> Self {
        Self {
            client: Client::default().with_url(url),
        }
    }

    /// Apply the analytics schema (idempotent).
    pub async fn migrate(&self) -> Result<(), ChError> {
        self.client.query(schema::EVENTS_RAW).execute().await?;
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
}
