//! Usage metering: price token usage via the catalog, record the event, and charge credits — the
//! product's core loop in one call.

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_event::{EventStore, RecordEvent};
use meter_ledger::{ChargeRequest, LedgerBackend, ReservationId, ReserveRequest, SettleRequest};
use meter_pricing::price_usage;
use meter_ratecards::rate_card_for;

use crate::dto::{MeterUsageBody, ReserveUsageBody, SettleUsageBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/usage`
pub async fn meter_usage(
    State(state): State<AppState>,
    Json(body): Json<MeterUsageBody>,
) -> Result<Json<Value>, ApiError> {
    let card = rate_card_for(&body.model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.model)))?;

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

    Ok(Json(json!({
        "credits": priced.credits.value().normalize().to_string(),
        "cogs_usd": priced.cogs.amount().normalize().to_string(),
        "customer_price_usd": priced.customer_price.amount().normalize().to_string(),
        "event_id": event.id.to_string(),
        "charged": charged,
        "settled": balance.settled.value().normalize().to_string(),
        "available": balance.available().value().normalize().to_string(),
    })))
}

/// `POST /v1/usage/reserve` — price a worst-case estimate against a catalog model and place a hold.
/// The engine computes the credits (ADR 0001). Returns the reserve outcome plus the reserved credits.
pub async fn reserve_usage(
    State(state): State<AppState>,
    Json(body): Json<ReserveUsageBody>,
) -> Result<Json<Value>, ApiError> {
    let card = rate_card_for(&body.model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.model)))?;
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
        })
        .await?;
    let mut value = serde_json::to_value(outcome)
        .map_err(|error| ApiError::unprocessable(format!("serialize: {error}")))?;
    if let Value::Object(map) = &mut value {
        map.insert(
            "reserved_credits".to_owned(),
            json!(priced.credits.value().normalize().to_string()),
        );
    }
    Ok(Json(value))
}

/// `POST /v1/usage/reservations/{id}/settle` — price the actual usage against a catalog model and
/// settle the reservation. Idempotent on the reservation id.
pub async fn settle_usage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SettleUsageBody>,
) -> Result<Json<Value>, ApiError> {
    let card = rate_card_for(&body.model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.model)))?;
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
    Ok(Json(json!({
        "credits_charged": priced.credits.value().normalize().to_string(),
        "balance_after": entry.balance_after.value().normalize().to_string(),
    })))
}
