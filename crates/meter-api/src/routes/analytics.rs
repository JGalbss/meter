//! Analytics: read-only usage rollups over the ledger (the authoritative Postgres data).

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::AccountId;
use meter_store_ch::{DayUsage as EventDayUsage, ModelUsage};
use meter_store_pg::DayUsage;

use crate::error::ApiError;
use crate::AppState;

/// `?start=<rfc3339>&end=<rfc3339>`
#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
}

/// `GET /v1/accounts/{id}/usage-by-day?start&end` — daily credit usage time series.
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/usage-by-day",
    params(
        ("id" = String, Path, description = "Account id (UUID)"),
        ("start" = String, Query, description = "Period start (RFC3339)"),
        ("end" = String, Query, description = "Period end (RFC3339)")
    ),
    responses((status = 200, description = "Daily credit totals over the period")),
    tag = "analytics"
)]
pub async fn usage_by_day(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<Vec<DayUsage>>, ApiError> {
    let account = AccountId::from_uuid(id);
    let days = state
        .ledger
        .usage_by_day(account, query.start, query.end)
        .await?;
    Ok(Json(days))
}

/// `GET /v1/orgs/{id}/usage-by-model` — usage aggregated by model, derived from the event store
/// (`ClickHouse`). Reflects amends and voids; ordered by spend, highest first.
#[utoipa::path(
    get,
    path = "/v1/orgs/{id}/usage-by-model",
    params(("id" = String, Path, description = "Org id (UUID)")),
    responses((status = 200, description = "Usage aggregated by model, ordered by spend")),
    tag = "analytics"
)]
pub async fn usage_by_model(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ModelUsage>>, ApiError> {
    let usage = state
        .events
        .usage_by_model(id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(usage))
}

/// `GET /v1/orgs/{id}/usage-by-day` — daily event + credit totals from the event store (`ClickHouse`).
#[utoipa::path(
    get,
    path = "/v1/orgs/{id}/usage-by-day",
    params(("id" = String, Path, description = "Org id (UUID)")),
    responses((status = 200, description = "Daily event + credit totals")),
    tag = "analytics"
)]
pub async fn org_usage_by_day(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<EventDayUsage>>, ApiError> {
    let days = state
        .events
        .usage_by_day(id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(days))
}

/// `GET /v1/orgs/{id}/event-count` — count of an organization's live (recorded) events.
#[utoipa::path(
    get,
    path = "/v1/orgs/{id}/event-count",
    params(("id" = String, Path, description = "Org id (UUID)")),
    responses((status = 200, description = "Count of live (recorded) events")),
    tag = "analytics"
)]
pub async fn event_count(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let count = state
        .events
        .event_count(id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(json!({ "count": count })))
}
