//! Reservation endpoints: reserve, settle, void.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use meter_ledger::{
    LedgerBackend, LedgerEntry, ReservationId, ReserveOutcome, ReserveRequest, SettleRequest,
};

use crate::dto::{ExtendBody, ReserveBody, SettleBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/reservations`
#[utoipa::path(
    post,
    path = "/v1/reservations",
    request_body = ReserveBody,
    responses((status = 200, description = "Reservation outcome (allowed or denied)", body = ReserveOutcome)),
    tag = "reservations"
)]
pub async fn reserve(
    State(state): State<AppState>,
    Json(body): Json<ReserveBody>,
) -> Result<Json<ReserveOutcome>, ApiError> {
    let outcome = state
        .ledger
        .reserve(ReserveRequest {
            account: body.account,
            reservation_id: body.reservation_id,
            amount: body.amount,
            limit: body.limit,
            expires_at: body.expires_at,
            run_id: body.run_id,
        })
        .await?;
    Ok(Json(outcome))
}

/// `POST /v1/reservations/{id}/settle`
#[utoipa::path(
    post,
    path = "/v1/reservations/{id}/settle",
    params(("id" = String, Path, description = "Reservation id (UUID)")),
    request_body = SettleBody,
    responses((status = 200, description = "Settle posted; returns the ledger entry", body = LedgerEntry)),
    tag = "reservations"
)]
pub async fn settle(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SettleBody>,
) -> Result<Json<LedgerEntry>, ApiError> {
    let entry = state
        .ledger
        .settle(SettleRequest {
            reservation_id: ReservationId::from_uuid(id),
            actual: body.actual,
        })
        .await?;
    Ok(Json(entry))
}

/// `POST /v1/reservations/{id}/void`
#[utoipa::path(
    post,
    path = "/v1/reservations/{id}/void",
    params(("id" = String, Path, description = "Reservation id (UUID)")),
    responses((status = 204, description = "Hold released (idempotent)")),
    tag = "reservations"
)]
pub async fn void(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.ledger.void(ReservationId::from_uuid(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/reservations/{id}/extend` — push the hold's expiry forward (heartbeat keep-alive).
#[utoipa::path(
    post,
    path = "/v1/reservations/{id}/extend",
    params(("id" = String, Path, description = "Reservation id (UUID)")),
    request_body = ExtendBody,
    responses(
        (status = 204, description = "Expiry extended"),
        (status = 404, description = "Unknown reservation"),
        (status = 409, description = "Reservation already closed")
    ),
    tag = "reservations"
)]
pub async fn extend(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ExtendBody>,
) -> Result<StatusCode, ApiError> {
    state
        .ledger
        .extend_hold(ReservationId::from_uuid(id), body.expires_at)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
