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
use meter_core::Credit;
use meter_event::{EventStore, RecordEvent};
use meter_ledger::{
    ChargeRequest, LedgerBackend, ReservationId, ReserveOutcome, ReserveRequest, SettleRequest,
};
use meter_pricing::price_usage;

/// Where a usage charge was priced: a control-plane-`synced` rate card, or the hosted `catalog`.
fn pricing_source(rate_card_id: Option<&str>) -> &'static str {
    match rate_card_id {
        Some(_) => "synced",
        None => "catalog",
    }
}

/// Merge caller-supplied custom fields (`tags`) into an event's reserved properties. Reserved keys
/// (model, token dimensions, credits, pricing provenance) win, so a caller can never spoof the priced
/// amount by passing a `credits` tag. Non-object `tags` are ignored.
fn merge_tags(mut base: serde_json::Value, tags: serde_json::Value) -> serde_json::Value {
    if let (serde_json::Value::Object(base_map), serde_json::Value::Object(tag_map)) =
        (&mut base, tags)
    {
        for (key, value) in tag_map {
            base_map.entry(key).or_insert(value);
        }
    }
    base
}

/// Credits actually burned for a usage call: the priced amount when the usage is burnable, zero
/// otherwise. Non-burnable usage is still recorded for cost visibility but never debits the ledger,
/// so its burndown contribution is zero and the ledger stays the sole source of money-truth.
fn burned_credits(burnable: bool, priced: Credit) -> Credit {
    match burnable {
        true => priced,
        false => Credit::ZERO,
    }
}

/// `POST /v1/usage` result: the priced amounts, the recorded event id, and the resulting balance.
/// Credit/USD amounts are exact decimal strings.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeterUsageResult {
    /// Credits actually burned (debited from the ledger). Zero for non-burnable usage.
    pub credits: String,
    /// What the usage priced to, whether or not it burned. Equals `credits` for burnable usage.
    pub priced_credits: String,
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

    // Credits that actually burn: zero for non-burnable usage. We record this (not the priced amount)
    // as `credits` so analytics burndown reflects ledger truth; `priced_credits` keeps the would-be
    // charge for visibility.
    let burned = burned_credits(body.burnable, priced.credits);

    let event = state
        .events
        .record(RecordEvent {
            org_id: body.org_id,
            idempotency_key: format!("{}::event", body.idempotency_key),
            event_time: OffsetDateTime::now_utc(),
            meter: "tokens".to_owned(),
            account_id: body.account,
            run_id: body.run_id,
            properties: merge_tags(
                json!({
                    "model": body.model,
                    "input_uncached": body.usage.input_uncached,
                    "cache_read": body.usage.cache_read,
                    "cache_write": body.usage.cache_write,
                    "output": body.usage.output,
                    "reasoning": body.usage.reasoning,
                    "cogs_usd": priced.cogs.amount().normalize().to_string(),
                    "credits": burned.value().normalize().to_string(),
                    "priced_credits": priced.credits.value().normalize().to_string(),
                    "burnable": body.burnable,
                    // Pricing provenance, so a charge can always be re-derived/reconciled: which synced
                    // rate card (null = the hosted catalog) and which version priced this event.
                    "rate_card_id": body.rate_card_id,
                    "rate_card_version": card.version,
                    "priced_via": pricing_source(body.rate_card_id.as_deref()),
                }),
                body.tags,
            ),
        })
        .await?;

    let charged = burned.is_positive();
    if charged {
        state
            .ledger
            .charge(ChargeRequest {
                account: body.account,
                amount: burned,
                idempotency_key: Some(format!("{}::charge", body.idempotency_key)),
            })
            .await?;
    }

    let balance = state.ledger.balance(body.account).await?;

    Ok(Json(MeterUsageResult {
        credits: burned.value().normalize().to_string(),
        priced_credits: priced.credits.value().normalize().to_string(),
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

/// `POST /v1/usage/reservations/{id}/settle` — price the actual usage and settle the reservation.
///
/// Prices the actuals against a catalog model and closes the reservation. Idempotent on the
/// reservation id.
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

#[cfg(test)]
mod tests {
    use super::{burned_credits, merge_tags};
    use meter_core::Credit;
    use rust_decimal::Decimal;
    use serde_json::json;

    #[test]
    fn burnable_usage_burns_the_priced_amount() {
        let priced = Credit::from_decimal(Decimal::new(200_000, 0));
        assert_eq!(burned_credits(true, priced), priced);
    }

    #[test]
    fn non_burnable_usage_burns_nothing() {
        let priced = Credit::from_decimal(Decimal::new(200_000, 0));
        assert_eq!(burned_credits(false, priced), Credit::ZERO);
        assert!(!burned_credits(false, priced).is_positive());
    }

    #[test]
    fn merge_tags_adds_custom_fields_for_burndown() {
        let merged = merge_tags(
            json!({ "model": "claude-opus-4-8", "credits": "5" }),
            json!({ "team": "alpha", "feature": "chat" }),
        );
        assert_eq!(merged["team"], "alpha");
        assert_eq!(merged["feature"], "chat");
        assert_eq!(merged["model"], "claude-opus-4-8");
        assert_eq!(merged["credits"], "5");
    }

    #[test]
    fn merge_tags_cannot_override_reserved_keys() {
        // A caller must not be able to spoof the priced credits or model via tags.
        let merged = merge_tags(
            json!({ "model": "real", "credits": "5" }),
            json!({ "model": "spoofed", "credits": "0", "team": "alpha" }),
        );
        assert_eq!(merged["model"], "real");
        assert_eq!(merged["credits"], "5");
        assert_eq!(merged["team"], "alpha");
    }

    #[test]
    fn merge_tags_ignores_non_object_tags() {
        let base = json!({ "model": "x" });
        assert_eq!(merge_tags(base.clone(), json!(null)), base);
        assert_eq!(merge_tags(base.clone(), json!("not-an-object")), base);
        assert_eq!(merge_tags(base.clone(), json!(42)), base);
    }
}
