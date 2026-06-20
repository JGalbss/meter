//! Rate-card simulation: re-rate a usage stream from one catalog model onto another to preview the
//! credit impact of switching. Pure projection over the pricing layer — it never touches the ledger.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use meter_pricing::{simulate_rerate, Usage};
use meter_ratecards::rate_card_for;

use crate::dto::SimulateBody;
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/simulate`
pub async fn simulate(
    State(state): State<AppState>,
    Json(body): Json<SimulateBody>,
) -> Result<Json<Value>, ApiError> {
    let current = rate_card_for(&body.current_model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.current_model)))?;
    let proposed = rate_card_for(&body.proposed_model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.proposed_model)))?;

    let usages: Vec<Usage> = body.events.iter().map(|d| d.to_usage()).collect();
    let summary = simulate_rerate(&usages, &current, &proposed, &state.credit_value)
        .map_err(|error| ApiError::unprocessable(format!("pricing: {error}")))?;

    Ok(Json(json!({
        "current_model": body.current_model,
        "proposed_model": body.proposed_model,
        "event_count": summary.event_count,
        "credits_current": summary.credits_current.value().normalize().to_string(),
        "credits_proposed": summary.credits_proposed.value().normalize().to_string(),
        "credit_delta": summary.credit_delta.value().normalize().to_string(),
    })))
}
