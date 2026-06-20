//! Resolve which rate card to price against: an explicit control-plane-synced card id, or the
//! built-in catalog by model. Lets customers price with their own configured cards while keeping the
//! batteries-included catalog as the default.

use uuid::Uuid;

use meter_core::{Currency, RateCardId};
use meter_pricing::{Margin, PriceComponent, RateCard, RateCardKind};
use meter_ratecards::rate_card_for;
use meter_store_pg::{PgConfig, RateCardRecord};

use crate::error::ApiError;
use crate::AppState;

fn kind_from_str(kind: &str) -> Result<RateCardKind, ApiError> {
    match kind {
        "provider_cost" => Ok(RateCardKind::ProviderCost),
        "customer" => Ok(RateCardKind::Customer),
        other => Err(ApiError::unprocessable(format!(
            "unknown rate card kind: {other}"
        ))),
    }
}

/// Rebuild a pricing [`RateCard`] from a stored record (components round-trip through serde JSON).
fn rate_card_from_record(record: RateCardRecord) -> Result<RateCard, ApiError> {
    let currency = Currency::new(&record.currency)
        .map_err(|error| ApiError::unprocessable(format!("rate card currency: {error}")))?;
    let components: Vec<PriceComponent> = serde_json::from_value(record.components)
        .map_err(|error| ApiError::unprocessable(format!("rate card components: {error}")))?;
    let version = u32::try_from(record.version)
        .map_err(|_| ApiError::unprocessable("rate card version out of range"))?;
    Ok(RateCard {
        id: RateCardId::from_uuid(record.id),
        kind: kind_from_str(&record.kind)?,
        currency,
        version,
        margin: Margin::from_multiplier(record.margin),
        components,
    })
}

/// The card to price against: the synced card `rate_card_id` (its latest version) when given, else the
/// catalog card for `model`. `404` if the chosen source has no such card.
pub async fn resolve_card(
    state: &AppState,
    model: &str,
    rate_card_id: Option<&str>,
) -> Result<RateCard, ApiError> {
    match rate_card_id {
        None => rate_card_for(model)
            .ok_or_else(|| ApiError::not_found(format!("unknown model: {model}"))),
        Some(id) => {
            let uuid = Uuid::parse_str(id)
                .map_err(|_| ApiError::unprocessable(format!("invalid rate_card_id: {id}")))?;
            let record = PgConfig::new(state.ledger.pool().clone())
                .latest_rate_card(uuid)
                .await?
                .ok_or_else(|| ApiError::not_found(format!("unknown rate card: {id}")))?;
            rate_card_from_record(record)
        }
    }
}
