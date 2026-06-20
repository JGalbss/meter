//! ClickHouse-backed [`EventStore`] (ADR 0003) — the system of record for usage events.
//!
//! The editable event model maps onto a `ReplacingMergeTree(version)`: a status change (amend → the
//! original becomes `amended`; `void_run` → `voided`) is a new row with the same `id` and a higher
//! version, and reads use `FINAL` to resolve the latest version of each event id.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, EventId, OrgId, RunId};
use meter_event::{AmendEvent, Event, EventError, EventStatus, EventStore, RecordEvent};

use crate::ChStore;

/// The `events` columns in struct order — `RowBinary` reads positionally, so SELECTs must match.
/// A macro (not a `const`) so the list can be `concat!`-ed into compile-time query strings, keeping
/// every read a static `&str` (no runtime `format!`, single source of truth for the column order).
macro_rules! event_columns {
    () => {
        "id, org_id, idempotency_key, event_time, meter, account_id, run_id, \
         properties, status, supersedes, created_at, version"
    };
}

const SELECT_BY_KEY: &str = concat!(
    "SELECT ",
    event_columns!(),
    " FROM events FINAL WHERE org_id = ? AND idempotency_key = ? LIMIT 1"
);
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
    /// Insert one event version (assigns the next version for the `ReplacingMergeTree`).
    async fn insert_event(&self, event: &Event) -> Result<(), EventError> {
        let row = EventRow {
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
        };
        let mut insert = self.client.insert("events").map_err(backend)?;
        insert.write(&row).await.map_err(backend)?;
        insert.end().await.map_err(backend)?;
        Ok(())
    }

    async fn find_by_key(&self, org_id: OrgId, key: &str) -> Result<Option<Event>, EventError> {
        let rows = self
            .client
            .query(SELECT_BY_KEY)
            .bind(org_id.as_uuid())
            .bind(key)
            .fetch_all::<EventRow>()
            .await
            .map_err(backend)?;
        match rows.into_iter().next() {
            None => Ok(None),
            Some(row) => Ok(Some(row_to_event(row)?)),
        }
    }
}

#[async_trait]
impl EventStore for ChStore {
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError> {
        if let Some(existing) = self.find_by_key(req.org_id, &req.idempotency_key).await? {
            return Ok(existing);
        }
        let event = Event {
            id: EventId::new(),
            org_id: req.org_id,
            idempotency_key: req.idempotency_key,
            event_time: req.event_time,
            meter: req.meter,
            account_id: req.account_id,
            run_id: req.run_id,
            properties: req.properties,
            status: EventStatus::Recorded,
            supersedes: None,
            created_at: OffsetDateTime::now_utc(),
        };
        self.insert_event(&event).await?;
        Ok(event)
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
        // Supersede the original: same id, status `amended`, a higher version.
        let mut superseded = original.clone();
        superseded.status = EventStatus::Amended;
        self.insert_event(&superseded).await?;

        let new_id = EventId::new();
        let amended = Event {
            id: new_id,
            org_id: original.org_id,
            idempotency_key: format!("{}::amend::{new_id}", original.idempotency_key),
            event_time: original.event_time,
            meter: original.meter,
            account_id: original.account_id,
            run_id: original.run_id,
            properties: req.properties,
            status: EventStatus::Recorded,
            supersedes: Some(original.id),
            created_at: OffsetDateTime::now_utc(),
        };
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
