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

use std::collections::BTreeMap;
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
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq, utoipa::ToSchema)]
pub struct ModelUsage {
    pub model: String,
    pub events: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

/// Credit burndown for one value of an arbitrary grouping factor — a custom event field (e.g.
/// `team`, `feature`, `customer`) or a built-in like `model`. `dimension` is the field's value;
/// `credits` is the burnable spend attributed to it (events that recorded no credits contribute 0,
/// so non-burnable usage shows up as `events > 0` with `credits = 0`).
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq, utoipa::ToSchema)]
pub struct FieldUsage {
    pub dimension: String,
    pub events: u64,
    pub credits: f64,
}

/// One model's drift between the pre-aggregated `usage_rollup` and the live `events` system of record.
/// Returned only when the two disagree, so an empty result means the rollup faithfully tracks the SoR.
/// `*_events`/`*_credits` are the two sides; a non-empty list flags a rollup that needs rebuilding
/// (e.g. a materialized view added after data already existed, or a merge/ingest anomaly).
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq, utoipa::ToSchema)]
pub struct RollupDrift {
    pub model: String,
    pub rollup_events: i64,
    pub scan_events: i64,
    pub rollup_credits: f64,
    pub scan_credits: f64,
}

/// One model's aggregate, used internally to reconcile the rollup against the scan.
#[derive(clickhouse::Row, Deserialize)]
struct ModelAgg {
    model: String,
    events: i64,
    credits: f64,
}

/// Daily usage totals for an organization (a time series for charts). `day` is `YYYY-MM-DD`.
#[derive(clickhouse::Row, Serialize, Deserialize, Debug, Clone, PartialEq, utoipa::ToSchema)]
// Distinct OpenAPI name: the Postgres per-account view is also `DayUsage`.
#[schema(as = EventDayUsage)]
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

/// Whether a burndown field is pre-aggregated into `field_usage_rollup` (the fast read path) versus
/// served by the flexible `events`-scan path.
fn is_promoted_field(field: &str) -> bool {
    schema::PROMOTED_FIELDS.contains(&field)
}

/// Per-model drift between the rollup aggregate and the scan aggregate. A model on only one side
/// (missing from the other) is itself drift — its absent side reads as zero. Credits compare with a
/// small epsilon (`Float64` accumulation); event counts compare exactly.
fn diff_model_aggregates(rollup: Vec<ModelAgg>, scan: Vec<ModelAgg>) -> Vec<RollupDrift> {
    let mut sides: BTreeMap<String, (i64, f64, i64, f64)> = BTreeMap::new();
    for row in rollup {
        let entry = sides.entry(row.model).or_default();
        entry.0 = row.events;
        entry.1 = row.credits;
    }
    for row in scan {
        let entry = sides.entry(row.model).or_default();
        entry.2 = row.events;
        entry.3 = row.credits;
    }
    sides
        .into_iter()
        .filter(|(_, (re, rc, se, sc))| re != se || (rc - sc).abs() > 1e-6)
        .map(
            |(model, (rollup_events, rollup_credits, scan_events, scan_credits))| RollupDrift {
                model,
                rollup_events,
                scan_events,
                rollup_credits,
                scan_credits,
            },
        )
        .collect()
}

/// Optional filters for [`ChStore::list_audit`]. `None` fields are unconstrained.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub actor: Option<String>,
    pub method: Option<String>,
    pub since: Option<OffsetDateTime>,
    pub until: Option<OffsetDateTime>,
}

/// One recorded mutating action (ADR 0004). Returned by reads; serialized for the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub actor: String,
    pub method: String,
    pub path: String,
    pub status: i32,
    pub request_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// The `audit` row as stored in ClickHouse (RowBinary codecs); converted to [`AuditEntry`] on read.
/// Field order matches the SELECT in `list_audit` (RowBinary is positional).
#[derive(clickhouse::Row, Serialize, Deserialize)]
struct AuditRow {
    #[serde(with = "clickhouse::serde::uuid")]
    id: Uuid,
    actor: String,
    method: String,
    path: String,
    status: i32,
    request_id: String,
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
        request_id: row.request_id,
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
            .query(schema::FIELD_USAGE_ROLLUP)
            .execute()
            .await?;
        self.client
            .query(&schema::field_usage_rollup_mv())
            .execute()
            .await?;
        self.client
            .query(schema::EVENTS_DEAD_LETTER)
            .execute()
            .await?;
        self.client.query(schema::AUDIT).execute().await?;
        self.client
            .query(schema::AUDIT_ADD_REQUEST_ID)
            .execute()
            .await?;
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
        request_id: &str,
    ) -> Result<(), ChError> {
        let row = AuditRow {
            id: Uuid::now_v7(),
            actor: actor.to_owned(),
            method: method.to_owned(),
            path: path.to_owned(),
            status,
            request_id: request_id.to_owned(),
            created_at: OffsetDateTime::now_utc(),
        };
        let mut insert = self.client.insert("audit")?;
        insert.write(&row).await?;
        insert.end().await?;
        Ok(())
    }

    /// The most recent audit entries matching `filter`, newest first.
    pub async fn list_audit(
        &self,
        limit: i64,
        filter: &AuditFilter,
    ) -> Result<Vec<AuditEntry>, ChError> {
        // Build the WHERE from whichever filters are set, binding in the same order.
        let mut sql = String::from(
            "SELECT id, actor, method, path, status, request_id, created_at FROM audit WHERE 1 = 1",
        );
        if filter.actor.is_some() {
            sql.push_str(" AND actor = ?");
        }
        if filter.method.is_some() {
            sql.push_str(" AND method = ?");
        }
        // Compare the DateTime64 column via Unix millis to avoid datetime-bind format pitfalls.
        if filter.since.is_some() {
            sql.push_str(" AND toUnixTimestamp64Milli(created_at) >= ?");
        }
        if filter.until.is_some() {
            sql.push_str(" AND toUnixTimestamp64Milli(created_at) < ?");
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ?");

        let to_millis = |time: OffsetDateTime| (time.unix_timestamp_nanos() / 1_000_000) as i64;
        let mut query = self.client.query(&sql);
        if let Some(actor) = &filter.actor {
            query = query.bind(actor);
        }
        if let Some(method) = &filter.method {
            query = query.bind(method);
        }
        if let Some(since) = filter.since {
            query = query.bind(to_millis(since));
        }
        if let Some(until) = filter.until {
            query = query.bind(to_millis(until));
        }
        let rows = query.bind(limit).fetch_all::<AuditRow>().await?;
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

    /// Flexible credit burndown: group an org's live usage by **any** factor and sum the burnable
    /// credits per value. `field` is a custom event-property name (e.g. `team`, `feature`, `customer`)
    /// or a built-in like `model` — the system is agnostic to what you slice by. Reads the events
    /// system of record (`FINAL` + `status = 'recorded'`, so amends/voids are reflected); events that
    /// recorded no `credits` contribute zero, so non-burnable usage surfaces as `credits = 0`.
    ///
    /// A [`promoted`](schema::PROMOTED_FIELDS) field reads the pre-aggregated `field_usage_rollup`
    /// (O(rollup groups), no `FINAL` scan or read-time JSON parsing); any other field uses the flexible
    /// `events`-scan path. Both yield the identical live aggregate.
    pub async fn usage_by_field(
        &self,
        org_id: Uuid,
        field: &str,
    ) -> Result<Vec<FieldUsage>, ChError> {
        if is_promoted_field(field) {
            return self.usage_by_promoted_field(org_id, field).await;
        }
        let rows = self
            .client
            .query(
                "SELECT JSONExtractString(properties, ?) AS dimension, \
                 toUInt64(count()) AS events, \
                 sum(toFloat64OrZero(JSONExtractString(properties, 'credits'))) AS credits \
                 FROM events FINAL \
                 WHERE org_id = ? AND status = 'recorded' \
                 GROUP BY dimension \
                 HAVING dimension != '' \
                 ORDER BY credits DESC, events DESC",
            )
            .bind(field)
            .bind(org_id)
            .fetch_all::<FieldUsage>()
            .await?;
        Ok(rows)
    }

    /// The pre-aggregated burndown read for a promoted field: sum the sign-weighted rollup rows for
    /// `field`, dropping values whose contributions have fully cancelled (`events = 0`, i.e. every
    /// event for that value was amended/voided away).
    async fn usage_by_promoted_field(
        &self,
        org_id: Uuid,
        field: &str,
    ) -> Result<Vec<FieldUsage>, ChError> {
        let rows = self
            .client
            .query(
                "SELECT field_value AS dimension, toUInt64(events) AS events, credits FROM ( \
                 SELECT field_value, sum(events) AS events, sum(credits) AS credits \
                 FROM field_usage_rollup \
                 WHERE org_id = ? AND field_name = ? \
                 GROUP BY field_value) \
                 WHERE events > 0 \
                 ORDER BY credits DESC, events DESC",
            )
            .bind(org_id)
            .bind(field)
            .fetch_all::<FieldUsage>()
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

    /// Reconcile the pre-aggregated `usage_rollup` against the live `events` system of record for an
    /// org, per model. Returns one [`RollupDrift`] for each model where the rollup's sign-weighted
    /// totals disagree with a direct `FINAL` scan; an empty result means the rollup is consistent. This
    /// is an operational safety net — the rollup is the fast read path, the scan is ground truth, and
    /// this surfaces any divergence (a backfilled MV, a merge anomaly) before it reaches a bill.
    pub async fn reconcile_model_usage(&self, org_id: Uuid) -> Result<Vec<RollupDrift>, ChError> {
        let rollup = self
            .client
            .query(
                "SELECT model, toInt64(sum(events)) AS events, sum(credits) AS credits \
                 FROM usage_rollup WHERE org_id = ? AND model != '' GROUP BY model",
            )
            .bind(org_id)
            .fetch_all::<ModelAgg>()
            .await?;
        let scan = self
            .client
            .query(
                "SELECT JSONExtractString(properties, 'model') AS model, \
                 toInt64(count()) AS events, \
                 sum(toFloat64OrZero(JSONExtractString(properties, 'credits'))) AS credits \
                 FROM events FINAL \
                 WHERE org_id = ? AND status = 'recorded' \
                 AND JSONExtractString(properties, 'model') != '' \
                 GROUP BY model",
            )
            .bind(org_id)
            .fetch_all::<ModelAgg>()
            .await?;
        Ok(diff_model_aggregates(rollup, scan))
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

#[cfg(test)]
mod tests {
    use super::{diff_model_aggregates, ModelAgg};

    fn agg(model: &str, events: i64, credits: f64) -> ModelAgg {
        ModelAgg {
            model: model.to_owned(),
            events,
            credits,
        }
    }

    #[test]
    fn consistent_rollup_and_scan_report_no_drift() {
        let rollup = vec![agg("opus", 2, 50.0), agg("gpt-x", 1, 40.0)];
        let scan = vec![agg("gpt-x", 1, 40.0), agg("opus", 2, 50.0)];
        assert!(diff_model_aggregates(rollup, scan).is_empty());
    }

    #[test]
    fn diverging_credits_or_events_are_reported_as_drift() {
        // opus credits disagree; gpt-x event counts disagree; claude matches and must not appear.
        let rollup = vec![
            agg("opus", 2, 50.0),
            agg("gpt-x", 1, 40.0),
            agg("claude", 3, 9.0),
        ];
        let scan = vec![
            agg("opus", 2, 49.0),
            agg("gpt-x", 2, 40.0),
            agg("claude", 3, 9.0),
        ];
        let drift = diff_model_aggregates(rollup, scan);
        assert_eq!(drift.len(), 2);
        // BTreeMap orders by model: gpt-x then opus.
        assert_eq!(drift[0].model, "gpt-x");
        assert_eq!(drift[0].rollup_events, 1);
        assert_eq!(drift[0].scan_events, 2);
        assert_eq!(drift[1].model, "opus");
        assert_eq!(drift[1].rollup_credits, 50.0);
        assert_eq!(drift[1].scan_credits, 49.0);
    }

    #[test]
    fn a_model_present_on_only_one_side_is_drift() {
        // The rollup has a model the scan lacks (e.g. a stale rollup row) — must surface, scan side zero.
        let drift = diff_model_aggregates(vec![agg("ghost", 5, 12.0)], vec![]);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].model, "ghost");
        assert_eq!(drift[0].rollup_events, 5);
        assert_eq!(drift[0].scan_events, 0);
        assert_eq!(drift[0].scan_credits, 0.0);
    }
}
