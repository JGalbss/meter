//! Event endpoints: record, get, list, amend, void a run.

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, EventId, RunId};
use meter_event::{AmendEvent, Event, EventStore, RecordEvent};

use crate::dto::{AmendBody, RecordEventBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/events`
pub async fn record(
    State(state): State<AppState>,
    Json(body): Json<RecordEventBody>,
) -> Result<Json<Event>, ApiError> {
    let event = state
        .events
        .record(RecordEvent {
            org_id: body.org_id,
            idempotency_key: body.idempotency_key,
            event_time: body.event_time.unwrap_or_else(OffsetDateTime::now_utc),
            meter: body.meter,
            account_id: body.account,
            run_id: body.run_id,
            properties: body.properties,
        })
        .await?;
    Ok(Json(event))
}

/// `GET /v1/events/{id}`
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    let event = state.events.get(EventId::from_uuid(id)).await?;
    Ok(Json(event))
}

/// `GET /v1/accounts/{id}/events`
pub async fn list_for_account(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Event>>, ApiError> {
    let events = state
        .events
        .list_for_account(AccountId::from_uuid(id))
        .await?;
    Ok(Json(events))
}

/// `POST /v1/events/{id}/amend`
pub async fn amend(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AmendBody>,
) -> Result<Json<Event>, ApiError> {
    let event = state
        .events
        .amend(AmendEvent {
            event_id: EventId::from_uuid(id),
            properties: body.properties,
        })
        .await?;
    Ok(Json(event))
}

/// `POST /v1/runs/{id}/void`
pub async fn void_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let voided = state.events.void_run(RunId::from_uuid(id)).await?;
    Ok(Json(json!({ "voided": voided })))
}
