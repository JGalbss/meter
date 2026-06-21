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
use crate::dto::{AmendUsageBody, MeterUsageBody, ReserveUsageBody, SettleUsageBody};
use crate::error::ApiError;
use crate::AppState;
use meter_core::{Credit, EntryId, EventId};
use meter_event::{AmendEvent, Event, EventError, EventStatus, EventStore, RecordEvent};
use meter_ledger::{
    ChargeRequest, LedgerBackend, ReservationId, ReserveOutcome, ReserveRequest,
    ReverseChargeRequest, SettleRequest,
};
use meter_pricing::price_usage;
use rust_decimal::Decimal;
use std::str::FromStr;

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

    // Charge first so we can record the charge's ledger entry id on the event — the link a usage
    // amendment needs to reverse exactly this charge. (Both stores are idempotent on their keys, so a
    // retry after a mid-call failure re-derives the same charge + event.)
    let charged = burned.is_positive();
    let charge_entry_id = match charged {
        true => Some(
            state
                .ledger
                .charge(ChargeRequest {
                    account: body.account,
                    amount: burned,
                    // Tag the charge with its run so POST /v1/runs/{id}/void can reverse it if the run fails.
                    run_id: body.run_id,
                    idempotency_key: Some(format!("{}::charge", body.idempotency_key)),
                })
                .await?
                .id
                .to_string(),
        ),
        false => None,
    };

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
                    // The ledger entry this usage charged, so an amendment can reverse exactly it.
                    "charge_entry_id": charge_entry_id,
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

/// `POST /v1/usage/{event_id}/amend` result: the re-priced amounts, the signed credit delta posted to
/// the ledger, the new event version id, and the resulting balance.
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendUsageResult {
    #[schema(value_type = String, format = "uuid")]
    pub event_id: String,
    /// Credits the corrected usage burns (debited), as an exact decimal string.
    pub credits: String,
    /// What the corrected usage priced to, whether or not it burned.
    pub priced_credits: String,
    /// Signed change applied to the ledger: new burned − old burned.
    pub delta: String,
    pub settled: String,
    pub available: String,
}

/// Only an engine-priced usage event (one that carries pricing provenance) can be re-priced/amended.
fn is_engine_priced(event: &Event) -> bool {
    event
        .properties
        .get("priced_via")
        .is_some_and(|value| !value.is_null())
}

/// Read a string property off an event's JSON.
fn prop_str(event: &Event, key: &str) -> Option<String> {
    event
        .properties
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

/// The burned credits recorded on an event, parsed back to a [`Credit`] (zero if absent/unparseable).
fn recorded_credits(event: &Event) -> Credit {
    prop_str(event, "credits")
        .and_then(|raw| Decimal::from_str(&raw).ok())
        .map(Credit::from_decimal)
        .unwrap_or(Credit::ZERO)
}

/// The ledger entry an event's charge produced, if it recorded one.
fn recorded_charge_entry(event: &Event) -> Option<EntryId> {
    prop_str(event, "charge_entry_id")
        .and_then(|raw| Uuid::parse_str(&raw).ok())
        .map(EntryId::from_uuid)
}

/// `POST /v1/usage/{event_id}/amend` — correct a priced usage event and post the ledger delta.
///
/// The engine re-prices the corrected token counts (the caller never supplies credits, ADR 0001),
/// honouring the original event's immutable `burnable` flag, then adjusts the ledger to the new amount
/// by reversing the original charge's remainder and posting the re-priced charge (net = the delta —
/// ADR 0009). It records a new event version. Idempotent on `idempotency_key`.
#[utoipa::path(
    post,
    path = "/v1/usage/{event_id}/amend",
    params(("event_id" = String, Path, description = "The priced usage event id (UUID) to amend")),
    request_body = AmendUsageBody,
    responses(
        (status = 200, description = "Re-priced; the ledger delta posted and a new event version recorded", body = AmendUsageResult),
        (status = 422, description = "Not an engine-priced usage event, or it is voided/superseded")
    ),
    tag = "usage"
)]
pub async fn amend_usage(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AmendUsageBody>,
) -> Result<Json<AmendUsageResult>, ApiError> {
    let event_id = EventId::from_uuid(id);
    let original = state.events.get(event_id).await?;
    if matches!(original.status, EventStatus::Voided) {
        return Err(ApiError::unprocessable(
            "event is voided; a voided run's events cannot be amended (would re-charge a dead run)",
        ));
    }
    if !is_engine_priced(&original) {
        return Err(ApiError::unprocessable(
            "not an engine-priced usage event; only POST /v1/usage events can be amended",
        ));
    }

    // Idempotent: the version THIS amend produces is content-addressed from the original key + the amend
    // key (matching `AmendEvent::into_amended_event`). If it already exists, the amendment was applied —
    // return its current result without re-posting (the ledger postings are keyed-idempotent too).
    let amended_id = meter_event::idempotent_event_id(
        original.org_id,
        &format!(
            "{}::amend::{}",
            original.idempotency_key, body.idempotency_key
        ),
    );
    if let Ok(existing) = state.events.get(amended_id).await {
        let balance = state.ledger.balance(original.account_id).await?;
        let new_burned = recorded_credits(&existing);
        let delta = new_burned.value() - recorded_credits(&original).value();
        return Ok(Json(AmendUsageResult {
            event_id: existing.id.to_string(),
            credits: new_burned.value().normalize().to_string(),
            priced_credits: prop_str(&existing, "priced_credits").unwrap_or_default(),
            delta: delta.normalize().to_string(),
            settled: balance.settled.value().normalize().to_string(),
            available: balance.available().value().normalize().to_string(),
        }));
    }

    // A fresh amend may only target the current recorded version — never a superseded one (which would
    // fork the event into two live versions).
    if !matches!(original.status, EventStatus::Recorded) {
        return Err(ApiError::unprocessable(
            "event is superseded; amend its current version instead",
        ));
    }

    // Re-price the corrected usage; model + card default to the original event's.
    let model = body
        .model
        .clone()
        .or_else(|| prop_str(&original, "model"))
        .ok_or_else(|| {
            ApiError::unprocessable("original event has no model to re-price against")
        })?;
    let rate_card_id = body
        .rate_card_id
        .clone()
        .or_else(|| prop_str(&original, "rate_card_id"));
    let card = resolve_card(&state, &model, rate_card_id.as_deref()).await?;
    let usage = body.usage.to_usage();
    let priced = price_usage(&usage, &card, &state.credit_value)
        .map_err(|error| ApiError::unprocessable(format!("pricing: {error}")))?;

    // Honour the original's immutable burnable flag — an amend can never flip free <-> paid.
    let burnable = original
        .properties
        .get("burnable")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let new_burned = burned_credits(burnable, priced.credits);
    let old_burned = recorded_credits(&original);
    let account = original.account_id;
    let run_id = original.run_id;

    // Reverse the current charge's remainder, then post the re-priced charge: net = new − old. The
    // ledger postings are keyed by the *amended event id* (unique per amendment), NOT the caller's amend
    // key — otherwise two amends of different events under the same key would collide and the second
    // would reuse the first's postings. The amended id is content-addressed, so a true retry still keys
    // identically (idempotent).
    let amend_scope = amended_id.to_string();
    if let Some(charge_id) = recorded_charge_entry(&original) {
        state
            .ledger
            .reverse_charge(ReverseChargeRequest {
                account,
                charge_entry_id: charge_id,
                run_id,
                idempotency_key: format!("{amend_scope}::amend-reverse"),
            })
            .await?;
    }
    let new_charge_entry_id = match new_burned.is_positive() {
        true => Some(
            state
                .ledger
                .charge(ChargeRequest {
                    account,
                    amount: new_burned,
                    run_id,
                    idempotency_key: Some(format!("{amend_scope}::amend-charge")),
                })
                .await?
                .id
                .to_string(),
        ),
        false => None,
    };

    // Record the amended event version with the re-priced amounts + its new charge link.
    let amend = state
        .events
        .amend(AmendEvent {
            event_id,
            properties: json!({
                "model": model,
                "input_uncached": body.usage.input_uncached,
                "cache_read": body.usage.cache_read,
                "cache_write": body.usage.cache_write,
                "output": body.usage.output,
                "reasoning": body.usage.reasoning,
                "cogs_usd": priced.cogs.amount().normalize().to_string(),
                "credits": new_burned.value().normalize().to_string(),
                "priced_credits": priced.credits.value().normalize().to_string(),
                "burnable": burnable,
                "charge_entry_id": new_charge_entry_id,
                "rate_card_id": rate_card_id,
                "rate_card_version": card.version,
                "priced_via": pricing_source(rate_card_id.as_deref()),
                "amends": original.id.to_string(),
            }),
            idempotency_key: Some(body.idempotency_key.clone()),
        })
        .await;
    let amended = match amend {
        Ok(event) => event,
        // The event was voided out from under us (a concurrent void of the run) between our checks and
        // this write. Compensate: reverse the charge we just posted so it can't strand on a dead run.
        // The reversal is remainder-based, so if the void already swept it this is a no-op.
        Err(EventError::Voided(_)) => {
            if let Some(charge_id) = new_charge_entry_id
                .as_deref()
                .and_then(|raw| Uuid::parse_str(raw).ok())
            {
                state
                    .ledger
                    .reverse_charge(ReverseChargeRequest {
                        account,
                        charge_entry_id: EntryId::from_uuid(charge_id),
                        run_id,
                        idempotency_key: format!("{amend_scope}::amend-compensate"),
                    })
                    .await?;
            }
            return Err(ApiError::unprocessable(
                "event was voided concurrently; the amendment was aborted and its charge reversed",
            ));
        }
        Err(other) => return Err(other.into()),
    };

    let balance = state.ledger.balance(account).await?;
    let delta = new_burned.value() - old_burned.value();
    Ok(Json(AmendUsageResult {
        event_id: amended.id.to_string(),
        credits: new_burned.value().normalize().to_string(),
        priced_credits: priced.credits.value().normalize().to_string(),
        delta: delta.normalize().to_string(),
        settled: balance.settled.value().normalize().to_string(),
        available: balance.available().value().normalize().to_string(),
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
