//! ClickHouse-backed [`EventStore`] (ADR 0003) — the system of record for usage events.
//!
//! The editable event model maps onto a `ReplacingMergeTree(version)`: a status change (amend → the
//! original becomes `amended`; `void_run` → `voided`) is a new row with the same `id` and a higher
//! version, and reads use `FINAL` to resolve the latest version of each event id.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, EventId, OrgId, RunId};
use meter_event::{AmendEvent, Event, EventError, EventStatus, EventStore, RecordEvent};

use crate::{ChStore, IngestMode};

/// The `events` columns in struct order — `RowBinary` reads positionally, so SELECTs must match.
/// A macro (not a `const`) so the list can be `concat!`-ed into compile-time query strings, keeping
/// every read a static `&str` (no runtime `format!`, single source of truth for the column order).
macro_rules! event_columns {
    () => {
        "id, org_id, idempotency_key, event_time, meter, account_id, run_id, \
         properties, status, supersedes, created_at, version"
    };
}

const SELECT_BY_ID: &str = concat!(
    "SELECT ",
    event_columns!(),
    " FROM events FINAL WHERE id = ? LIMIT 1"
);
const SELECT_FOR_ACCOUNT: &str = concat!(
    "SELECT ",
    event_columns!(),
    " FROM events FINAL WHERE account_id = ? AND status = 'recorded' ORDER BY event_time, id"
);
const SELECT_FOR_RUN: &str = concat!(
    "SELECT ",
    event_columns!(),
    " FROM events FINAL WHERE run_id = ? AND status = 'recorded'"
);

#[derive(clickhouse::Row, Serialize, Deserialize)]
struct EventRow {
    #[serde(with = "clickhouse::serde::uuid")]
    id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    org_id: Uuid,
    idempotency_key: String,
    #[serde(with = "clickhouse::serde::time::datetime64::millis")]
    event_time: OffsetDateTime,
    meter: String,
    #[serde(with = "clickhouse::serde::uuid")]
    account_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid::option")]
    run_id: Option<Uuid>,
    properties: String,
    status: String,
    #[serde(with = "clickhouse::serde::uuid::option")]
    supersedes: Option<Uuid>,
    #[serde(with = "clickhouse::serde::time::datetime64::millis")]
    created_at: OffsetDateTime,
    version: u64,
}

#[derive(clickhouse::Row, Deserialize)]
struct IdRow {
    #[serde(with = "clickhouse::serde::uuid")]
    id: Uuid,
}

fn backend<E: std::fmt::Display>(error: E) -> EventError {
    EventError::Backend(error.to_string())
}

const fn status_to_str(status: EventStatus) -> &'static str {
    match status {
        EventStatus::Recorded => "recorded",
        EventStatus::Amended => "amended",
        EventStatus::Voided => "voided",
    }
}

fn status_from_str(value: &str) -> EventStatus {
    match value {
        "amended" => EventStatus::Amended,
        "voided" => EventStatus::Voided,
        _ => EventStatus::Recorded,
    }
}

fn row_to_event(row: EventRow) -> Result<Event, EventError> {
    Ok(Event {
        id: EventId::from_uuid(row.id),
        org_id: OrgId::from_uuid(row.org_id),
        idempotency_key: row.idempotency_key,
        event_time: row.event_time,
        meter: row.meter,
        account_id: AccountId::from_uuid(row.account_id),
        run_id: row.run_id.map(RunId::from_uuid),
        properties: serde_json::from_str(&row.properties).map_err(backend)?,
        status: status_from_str(&row.status),
        supersedes: row.supersedes.map(EventId::from_uuid),
        created_at: row.created_at,
    })
}

impl ChStore {
    /// Build the `RowBinary` row for one event version, assigning the next `ReplacingMergeTree` version
    /// (so a later status change supersedes the prior row).
    fn event_to_row(&self, event: &Event) -> Result<EventRow, EventError> {
        Ok(EventRow {
            id: event.id.as_uuid(),
            org_id: event.org_id.as_uuid(),
            idempotency_key: event.idempotency_key.clone(),
            event_time: event.event_time,
            meter: event.meter.clone(),
            account_id: event.account_id.as_uuid(),
            run_id: event.run_id.map(|run| run.as_uuid()),
            properties: serde_json::to_string(&event.properties).map_err(backend)?,
            status: status_to_str(event.status).to_owned(),
            supersedes: event.supersedes.map(|id| id.as_uuid()),
            created_at: event.created_at,
            version: self.next_version(),
        })
    }

    /// Insert one event version. Used by the single-row amend/void paths; the firehose ingest path
    /// uses [`record_batch`](EventStore::record_batch), which writes a whole batch in one insert.
    async fn insert_event(&self, event: &Event) -> Result<(), EventError> {
        let row = self.event_to_row(event)?;
        let mut insert = self.client.insert("events").map_err(backend)?;
        insert.write(&row).await.map_err(backend)?;
        insert.end().await.map_err(backend)?;
        Ok(())
    }

    /// Which of these (content-addressed) event ids already exist. One index-friendly lookup per org
    /// (the `events` primary key is `(org_id, id)`), so re-recording a key is idempotent without a
    /// per-event read — and, crucially, the sign-weighted `usage_rollup` never sees a duplicate `+1`.
    async fn existing_ids(&self, events: &[Event]) -> Result<HashSet<Uuid>, EventError> {
        // Chunk so the inlined `id IN (...)` list never exceeds ClickHouse's `max_query_size` (256 KB
        // default; a UUID literal is ~39 bytes, so 4 000 ids ≈ 156 KB leaves comfortable headroom).
        const CHUNK: usize = 4_000;
        let mut by_org: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for event in events {
            by_org
                .entry(event.org_id.as_uuid())
                .or_default()
                .push(event.id.as_uuid());
        }
        let mut found = HashSet::new();
        for (org, ids) in by_org {
            for chunk in ids.chunks(CHUNK) {
                // ids are UUIDs we just generated — safe to inline (no user input in the SQL).
                let list = chunk
                    .iter()
                    .map(|id| format!("'{id}'"))
                    .collect::<Vec<_>>()
                    .join(",");
                let query =
                    format!("SELECT id FROM events WHERE org_id = '{org}' AND id IN ({list})");
                let rows = self
                    .client
                    .query(&query)
                    .fetch_all::<IdRow>()
                    .await
                    .map_err(backend)?;
                found.extend(rows.into_iter().map(|row| row.id));
            }
        }
        Ok(found)
    }
}

#[async_trait]
impl EventStore for ChStore {
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError> {
        let mut events = self.record_batch(vec![req]).await?;
        events
            .pop()
            .ok_or_else(|| backend("record produced no event"))
    }

    async fn record_batch(&self, reqs: Vec<RecordEvent>) -> Result<Vec<Event>, EventError> {
        if reqs.is_empty() {
            return Ok(Vec::new());
        }
        // The firehose path. Build every event (its id is content-addressed from the idempotency key),
        // drop the ones already recorded (one index lookup per org, amortized across the whole batch),
        // and write the survivors in a single ClickHouse insert. Exactly-once ingest keeps both the
        // `events` system of record and the sign-weighted `usage_rollup` free of duplicate counts.
        let events: Vec<Event> = reqs.into_iter().map(RecordEvent::into_event).collect();
        let existing = match self.ingest_mode() {
            IngestMode::Append => HashSet::new(),
            IngestMode::ExactlyOnce => self.existing_ids(&events).await?,
        };
        let mut seen = HashSet::with_capacity(events.len());
        let mut insert = self.client.insert("events").map_err(backend)?;
        let mut wrote = false;
        for event in &events {
            let id = event.id.as_uuid();
            if existing.contains(&id) || !seen.insert(id) {
                continue;
            }
            insert
                .write(&self.event_to_row(event)?)
                .await
                .map_err(backend)?;
            wrote = true;
        }
        match wrote {
            true => insert.end().await.map_err(backend)?,
            // No new rows — abort the (empty) insert rather than committing a zero-row part.
            false => drop(insert),
        }
        Ok(events)
    }

    async fn get(&self, id: EventId) -> Result<Event, EventError> {
        let rows = self
            .client
            .query(SELECT_BY_ID)
            .bind(id.as_uuid())
            .fetch_all::<EventRow>()
            .await
            .map_err(backend)?;
        match rows.into_iter().next() {
            None => Err(EventError::NotFound(id)),
            Some(row) => row_to_event(row),
        }
    }

    async fn list_for_account(&self, account: AccountId) -> Result<Vec<Event>, EventError> {
        let rows = self
            .client
            .query(SELECT_FOR_ACCOUNT)
            .bind(account.as_uuid())
            .fetch_all::<EventRow>()
            .await
            .map_err(backend)?;
        rows.into_iter().map(row_to_event).collect()
    }

    async fn amend(&self, req: AmendEvent) -> Result<Event, EventError> {
        let original = self.get(req.event_id).await?;
        if original.status == EventStatus::Voided {
            return Err(EventError::Voided(req.event_id));
        }
        let amended = req.into_amended_event(&original);
        // Idempotent (keyed amends): if this exact amendment was already applied, don't re-run the two
        // writes — replaying the supersede + new-version inserts would post a second -1/+1 pair into the
        // sign-weighted rollups and corrupt the totals. A retry returns the existing version.
        let already_applied = self
            .existing_ids(std::slice::from_ref(&amended))
            .await?
            .contains(&amended.id.as_uuid());
        if already_applied {
            return self.get(amended.id).await;
        }
        // Supersede the original: same id, status `amended`, a higher version.
        let mut superseded = original.clone();
        superseded.status = EventStatus::Amended;
        self.insert_event(&superseded).await?;
        self.insert_event(&amended).await?;
        Ok(amended)
    }

    async fn void_run(&self, run: RunId) -> Result<u64, EventError> {
        let rows = self
            .client
            .query(SELECT_FOR_RUN)
            .bind(run.as_uuid())
            .fetch_all::<EventRow>()
            .await
            .map_err(backend)?;
        let mut voided = 0_u64;
        for row in rows {
            let mut event = row_to_event(row)?;
            event.status = EventStatus::Voided;
            self.insert_event(&event).await?;
            voided += 1;
        }
        Ok(voided)
    }
}
