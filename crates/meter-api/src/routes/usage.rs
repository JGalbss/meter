//! Usage metering: price token usage via the catalog, record the event, and charge credits — the
//! product's core loop in one call.

use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use time::OffsetDateTime;

use meter_event::{EventStore, RecordEvent};
use meter_ledger::{ChargeRequest, LedgerBackend};
use meter_pricing::{price_usage, ContextTier, Modality, PricingDimension, Usage};
use meter_ratecards::rate_card_for;

use crate::dto::MeterUsageBody;
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/usage`
pub async fn meter_usage(
    State(state): State<AppState>,
    Json(body): Json<MeterUsageBody>,
) -> Result<Json<Value>, ApiError> {
    let card = rate_card_for(&body.model)
        .ok_or_else(|| ApiError::not_found(format!("unknown model: {}", body.model)))?;

    // Only price dimensions with positive quantity (the catalog card need not cover every dimension).
    let mut usage = Usage::new(Modality::Text, ContextTier::Standard);
    let dimensions = [
        (PricingDimension::InputUncached, body.usage.input_uncached),
        (PricingDimension::CacheRead, body.usage.cache_read),
        (PricingDimension::CacheWrite, body.usage.cache_write),
        (PricingDimension::Output, body.usage.output),
    ];
    for (dimension, quantity) in dimensions {
        if quantity > 0 {
            usage = usage.with(dimension, Decimal::from(quantity));
        }
    }

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
