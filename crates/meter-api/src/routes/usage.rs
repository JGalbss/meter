//! Usage metering: price token usage via the catalog, record the event, and charge credits — the
//! product's core loop in one call.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use serde_json::json;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::cards::resolve_card;
use crate::dto::{MeterUsageBody, ReserveUsageBody, SettleUsageBody};
use crate::error::ApiError;
use crate::AppState;
use meter_event::{EventStore, RecordEvent};
use meter_ledger::{
    ChargeRequest, LedgerBackend, ReservationId, ReserveOutcome, ReserveRequest, SettleRequest,
};
use meter_pricing::price_usage;

/// `POST /v1/usage` result: the priced amounts, the recorded event id, and the resulting balance.
/// Credit/USD amounts are exact decimal strings.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeterUsageResult {
    pub credits: String,
    pub cogs_usd: String,
    pub customer_price_usd: String,
    #[schema(value_type = String, format = "uuid")]
    pub event_id: String,
    pub charged: bool,
    pub settled: String,
    pub available: String,
}

/// `POST /v1/usage/reservations/{id}/settle` result.
#[derive(Debug, Serialize, ToSchema)]
pub struct SettleUsageResult {
    pub credits_charged: String,
    pub balance_after: String,
}

/// `POST /v1/usage/reserve` result: the reserve outcome (flattened) plus the engine-computed credits
/// that were held.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReserveUsageResult {
    #[serde(flatten)]
    pub outcome: ReserveOutcome,
    /// Credits the engine priced and reserved, as an exact decimal string.
    pub reserved_credits: String,
}

/// `POST /v1/usage`
#[utoipa::path(
    post,
    path = "/v1/usage",
    request_body = MeterUsageBody,
    responses((status = 200, description = "Priced, recorded, and charged; returns credits + balance", body = MeterUsageResult)),
    tag = "usage"
)]
pub async fn meter_usage(
    State(state): State<AppState>,
    Json(body): Json<MeterUsageBody>,
) -> Result<Json<MeterUsageResult>, ApiError> {
    let card = resolve_card(&state, &body.model, body.rate_card_id.as_deref()).await?;

    let usage = body.usage.to_usage();
    let priced = price_usage(&usage, &card, &state.credit_value)
        .map_err(|error| ApiError::unprocessable(format!("pricing: {error}")))?;

    let event = state
        .events
        .record(RecordEvent {
            org_id: body.org_id,
            idempotency_key: format!("{}::event", body.idempotency_key),
            event_time: OffsetDateTime::now_utc(),
            meter: "tokens".to_owned(),
            account_id: body.account,
            run_id: body.run_id,
            properties: json!({
                "model": body.model,
                "input_uncached": body.usage.input_uncached,
                "cache_read": body.usage.cache_read,
                "cache_write": body.usage.cache_write,
                "output": body.usage.output,
                "reasoning": body.usage.reasoning,
                "cogs_usd": priced.cogs.amount().normalize().to_string(),
                "credits": priced.credits.value().normalize().to_string(),
            }),
        })
        .await?;

    let charged = priced.credits.is_positive();
    if charged {
        state
            .ledger
            .charge(ChargeRequest {
                account: body.account,
                amount: priced.credits,
                idempotency_key: Some(format!("{}::charge", body.idempotency_key)),
            })
            .await?;
    }

    let balance = state.ledger.balance(body.account).await?;

    Ok(Json(MeterUsageResult {
        credits: priced.credits.value().normalize().to_string(),
        cogs_usd: priced.cogs.amount().normalize().to_string(),
        customer_price_usd: priced.customer_price.amount().normalize().to_string(),
        event_id: event.id.to_string(),
        charged,
        settled: balance.settled.value().normalize().to_string(),
        available: balance.available().value().normalize().to_string(),
    }))
}

/// `POST /v1/usage/reserve` — price a worst-case estimate against a catalog model and place a hold.
/// The engine computes the credits (ADR 0001). Returns the reserve outcome plus the reserved credits.
#[utoipa::path(
    post,
    path = "/v1/usage/reserve",
    request_body = ReserveUsageBody,
    responses((status = 200, description = "Reserve outcome plus the reserved credits", body = ReserveUsageResult)),
    tag = "usage"
)]
pub async fn reserve_usage(
    State(state): State<AppState>,
    Json(body): Json<ReserveUsageBody>,
) -> Result<Json<ReserveUsageResult>, ApiError> {
    let card = resolve_card(&state, &body.model, body.rate_card_id.as_deref()).await?;
    let usage = body.estimate.to_usage();
    let priced = price_usage(&usage, &card, &state.credit_value)
        .map_err(|error| ApiError::unprocessable(format!("pricing: {error}")))?;
    let outcome = state
        .ledger
        .reserve(ReserveRequest {
            account: body.account,
            reservation_id: body.reservation_id,
            amount: priced.credits,
            limit: body.limit,
            expires_at: None,
            run_id: body.run_id,
        })
        .await?;
    Ok(Json(ReserveUsageResult {
        outcome,
        reserved_credits: priced.credits.value().normalize().to_string(),
    }))
}

/// `POST /v1/usage/reservations/{id}/settle` — price the actual usage against a catalog model and
/// settle the reservation. Idempotent on the reservation id.
#[utoipa::path(
    post,
    path = "/v1/usage/reservations/{id}/settle",
    params(("id" = String, Path, description = "Reservation id (UUID)")),
    request_body = SettleUsageBody,
    responses((status = 200, description = "Credits charged + balance after", body = SettleUsageResult)),
    tag = "usage"
)]
pub async fn settle_usage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SettleUsageBody>,
) -> Result<Json<SettleUsageResult>, ApiError> {
    let card = resolve_card(&state, &body.model, body.rate_card_id.as_deref()).await?;
    let usage = body.actual.to_usage();
    let priced = price_usage(&usage, &card, &state.credit_value)
        .map_err(|error| ApiError::unprocessable(format!("pricing: {error}")))?;
    let entry = state
        .ledger
        .settle(SettleRequest {
            reservation_id: ReservationId::from_uuid(id),
            actual: priced.credits,
        })
        .await?;
    Ok(Json(SettleUsageResult {
        credits_charged: priced.credits.value().normalize().to_string(),
        balance_after: entry.balance_after.value().normalize().to_string(),
    }))
}
