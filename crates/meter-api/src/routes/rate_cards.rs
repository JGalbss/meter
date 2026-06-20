//! Read synced (control-plane-configured) rate cards — the counterpart to the static `/v1/catalog`.

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use meter_pricing::RateCard;

use crate::cards::{list_stored_cards, load_stored_card};
use crate::error::ApiError;
use crate::AppState;

/// `GET /v1/rate-cards` — every synced rate card (live version each).
#[utoipa::path(
    get,
    path = "/v1/rate-cards",
    responses((status = 200, description = "Every synced rate card (live version each)", body = Vec<RateCard>)),
    tag = "rate-cards"
)]
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<RateCard>>, ApiError> {
    Ok(Json(list_stored_cards(&state).await?))
}

/// `GET /v1/rate-cards/{id}` — the live (latest-version) synced rate card; `404` if none was synced.
#[utoipa::path(
    get,
    path = "/v1/rate-cards/{id}",
    params(("id" = String, Path, description = "Rate-card id (UUID)")),
    responses(
        (status = 200, description = "The live synced rate card", body = RateCard),
        (status = 404, description = "No card synced for this id")
    ),
    tag = "rate-cards"
)]
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RateCard>, ApiError> {
    Ok(Json(load_stored_card(&state, id).await?))
}
