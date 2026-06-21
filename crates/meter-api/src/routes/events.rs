//! Event endpoints: record, get, list, amend, void a run.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use meter_core::{AccountId, EventId, RunId};
use meter_event::{AmendEvent, Event, EventStore};
use meter_ledger::LedgerBackend;

use crate::dto::{AmendBody, RecordBatchBody, RecordEventBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/events`
#[utoipa::path(
    post,
    path = "/v1/events",
    request_body = RecordEventBody,
    responses((status = 200, description = "Event recorded (idempotent on org + key)", body = Event)),
    tag = "events"
)]
pub async fn record(
    State(state): State<AppState>,
    Json(body): Json<RecordEventBody>,
) -> Result<Json<Event>, ApiError> {
    let event = state.events.record(body.into_request()).await?;
    Ok(Json(event))
}

/// `POST /v1/events/batch` — bulk ingest; returns `202 Accepted` with the count recorded.
///
/// Ids are content-addressed from `(org_id, idempotency_key)`, so callers can derive them without the
/// payload.
#[utoipa::path(
    post,
    path = "/v1/events/batch",
    request_body = RecordBatchBody,
    responses((status = 202, description = "Batch accepted; returns the count recorded")),
    tag = "events"
)]
pub async fn record_batch(
    State(state): State<AppState>,
    Json(body): Json<RecordBatchBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let reqs = body
        .events
        .into_iter()
        .map(RecordEventBody::into_request)
        .collect();
    let recorded = state.events.record_batch(reqs).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "accepted": recorded.len() })),
    ))
}

/// `GET /v1/events/{id}`
#[utoipa::path(
    get,
    path = "/v1/events/{id}",
    params(("id" = String, Path, description = "Event id (UUID)")),
    responses(
        (status = 200, description = "The event", body = Event),
        (status = 404, description = "Unknown event")
    ),
    tag = "events"
)]
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Event>, ApiError> {
    let event = state.events.get(EventId::from_uuid(id)).await?;
    Ok(Json(event))
}

/// `GET /v1/accounts/{id}/events`
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/events",
    params(("id" = String, Path, description = "Account id (UUID)")),
    responses((status = 200, description = "The account's latest (non-voided) events", body = Vec<Event>)),
    tag = "events"
)]
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
#[utoipa::path(
    post,
    path = "/v1/events/{id}/amend",
    params(("id" = String, Path, description = "Event id (UUID) to amend")),
    request_body = AmendBody,
    responses((status = 200, description = "New event version superseding the prior", body = Event)),
    tag = "events"
)]
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
            idempotency_key: body.idempotency_key,
        })
        .await?;
    Ok(Json(event))
}

/// `POST /v1/runs/{id}/void` — kill a whole run: void its events and reverse its ledger impact.
///
/// Voids the run's events (append-only: each is marked voided) and reverses its ledger impact (release
/// open holds, refund settled charges). Both halves are idempotent, so retrying a void is safe.
#[utoipa::path(
    post,
    path = "/v1/runs/{id}/void",
    params(("id" = String, Path, description = "Run id (UUID) to kill")),
    responses((status = 200, description = "Counts of events voided + ledger holds/charges reversed")),
    tag = "events"
)]
pub async fn void_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let run = RunId::from_uuid(id);
    let events_voided = state.events.void_run(run).await?;
    let ledger = state.ledger.void_run(run).await?;
    Ok(Json(json!({
        "events_voided": events_voided,
        "holds_released": ledger.holds_released,
        "charges_refunded": ledger.charges_refunded,
        "credits_refunded": ledger.credits_refunded,
    })))
}
