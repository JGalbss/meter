//! PostgreSQL implementation of [`meter_event::EventStore`].

use async_trait::async_trait;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

use meter_core::{AccountId, EventId, OrgId, RunId};
use meter_event::{AmendEvent, Event, EventError, EventStatus, EventStore, RecordEvent};

use crate::mapping::now_micros;

/// An event store backed by PostgreSQL.
#[derive(Debug, Clone)]
pub struct PgEventStore {
    pool: PgPool,
}

impl PgEventStore {
    /// Wrap a connection pool (shares the engine database with the ledger).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn ebe(error: sqlx::Error) -> EventError {
    EventError::Backend(error.to_string())
}

fn col<'r, T>(row: &'r PgRow, name: &str) -> Result<T, EventError>
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get::<T, _>(name)
        .map_err(|error| EventError::Backend(format!("column {name}: {error}")))
}

fn status_to_str(status: EventStatus) -> &'static str {
    match status {
        EventStatus::Recorded => "recorded",
        EventStatus::Amended => "amended",
        EventStatus::Voided => "voided",
    }
}

fn status_from_str(value: &str) -> Result<EventStatus, EventError> {
    let status = match value {
        "recorded" => EventStatus::Recorded,
        "amended" => EventStatus::Amended,
        "voided" => EventStatus::Voided,
        other => return Err(EventError::Backend(format!("unknown event status {other}"))),
    };
    Ok(status)
}

fn event_from_row(row: &PgRow) -> Result<Event, EventError> {
    let run_id: Option<Uuid> = col(row, "run_id")?;
    let supersedes: Option<Uuid> = col(row, "supersedes_event_id")?;
    Ok(Event {
        id: EventId::from_uuid(col::<Uuid>(row, "id")?),
        org_id: OrgId::from_uuid(col::<Uuid>(row, "org_id")?),
        idempotency_key: col(row, "idempotency_key")?,
        event_time: col(row, "event_time")?,
        meter: col(row, "meter")?,
        account_id: AccountId::from_uuid(col::<Uuid>(row, "account_id")?),
        run_id: run_id.map(RunId::from_uuid),
        properties: col::<Value>(row, "properties")?,
        status: status_from_str(&col::<String>(row, "status")?)?,
        supersedes: supersedes.map(EventId::from_uuid),
        created_at: col(row, "created_at")?,
    })
}

async fn insert_event(conn: &mut sqlx::PgConnection, event: &Event) -> Result<u64, EventError> {
    let result = sqlx::query(
        "INSERT INTO events \
         (id, org_id, idempotency_key, event_time, meter, account_id, run_id, properties, status, \
          supersedes_event_id, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         ON CONFLICT (org_id, idempotency_key) DO NOTHING",
    )
    .bind(event.id.as_uuid())
    .bind(event.org_id.as_uuid())
    .bind(&event.idempotency_key)
    .bind(event.event_time)
    .bind(&event.meter)
    .bind(event.account_id.as_uuid())
    .bind(event.run_id.map(|id| id.as_uuid()))
    .bind(&event.properties)
    .bind(status_to_str(event.status))
    .bind(event.supersedes.map(|id| id.as_uuid()))
    .bind(event.created_at)
    .execute(conn)
    .await
    .map_err(ebe)?;
    Ok(result.rows_affected())
}

#[async_trait]
impl EventStore for PgEventStore {
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError> {
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
            created_at: now_micros(),
        };
        let mut conn = self.pool.acquire().await.map_err(ebe)?;
        let inserted = insert_event(&mut conn, &event).await?;
        if inserted == 1 {
            return Ok(event);
        }
        // A row with this (org, key) already existed: return the canonical stored event.
        let row = sqlx::query("SELECT * FROM events WHERE org_id = $1 AND idempotency_key = $2")
            .bind(event.org_id.as_uuid())
            .bind(&event.idempotency_key)
            .fetch_one(&mut *conn)
            .await
            .map_err(ebe)?;
        event_from_row(&row)
    }

    async fn get(&self, id: EventId) -> Result<Event, EventError> {
        let row = sqlx::query("SELECT * FROM events WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(ebe)?
            .ok_or(EventError::NotFound(id))?;
        event_from_row(&row)
    }

    async fn list_for_account(&self, account: AccountId) -> Result<Vec<Event>, EventError> {
        let rows = sqlx::query(
            "SELECT * FROM events WHERE account_id = $1 AND status = 'recorded' \
             ORDER BY event_time, id",
        )
        .bind(account.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(ebe)?;
        rows.iter().map(event_from_row).collect()
    }

    async fn amend(&self, req: AmendEvent) -> Result<Event, EventError> {
        let mut tx = self.pool.begin().await.map_err(ebe)?;
        let original = sqlx::query("SELECT * FROM events WHERE id = $1 FOR UPDATE")
            .bind(req.event_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(ebe)?
            .ok_or(EventError::NotFound(req.event_id))?;
        let original = event_from_row(&original)?;
        if original.status == EventStatus::Voided {
            return Err(EventError::Voided(req.event_id));
        }

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
            created_at: now_micros(),
        };
        sqlx::query("UPDATE events SET status = 'amended' WHERE id = $1")
            .bind(req.event_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(ebe)?;
        insert_event(&mut tx, &amended).await?;
        tx.commit().await.map_err(ebe)?;
        Ok(amended)
    }

    async fn void_run(&self, run: RunId) -> Result<u64, EventError> {
        let result = sqlx::query(
            "UPDATE events SET status = 'voided' WHERE run_id = $1 AND status = 'recorded'",
        )
        .bind(run.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(ebe)?;
        Ok(result.rows_affected())
    }
}
