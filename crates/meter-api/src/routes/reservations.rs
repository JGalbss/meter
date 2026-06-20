//! Reservation endpoints: reserve, settle, void.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use meter_ledger::{
    LedgerBackend, LedgerEntry, ReservationId, ReserveOutcome, ReserveRequest, SettleRequest,
};

use crate::dto::{ReserveBody, SettleBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/reservations`
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
        })
        .await?;
    Ok(Json(outcome))
}

/// `POST /v1/reservations/{id}/settle`
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
pub async fn void(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.ledger.void(ReservationId::from_uuid(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
