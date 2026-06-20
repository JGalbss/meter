//! Rate cards: versioned, dimensional, priced mappings from usage to money.

use meter_core::{Currency, RateCardId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::component::PriceComponent;
use crate::dimension::{ContextTier, Modality, PricingDimension};
use crate::error::PricingError;

/// Whether a card records provider cost (COGS) or what a customer is charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateCardKind {
    ProviderCost,
    Customer,
}

/// A markup multiplier applied to provider cost to get the customer price. `1.30` is a 30% markup;
/// [`Margin::NONE`] (`1.00`) charges cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(value_type = String)]
pub struct Margin(#[serde(with = "rust_decimal::serde::str")] Decimal);

impl Margin {
    /// No markup (multiplier 1.00).
    pub const NONE: Self = Self(Decimal::ONE);

    /// Build a margin from a multiplier (e.g. `1.30` for +30%).
    #[must_use]
    pub const fn from_multiplier(multiplier: Decimal) -> Self {
        Self(multiplier)
    }

    /// The multiplier.
    #[must_use]
    pub const fn multiplier(self) -> Decimal {
        self.0
    }
}

/// A versioned rate card: one card, many priced dimensions, one margin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RateCard {
    #[schema(value_type = String, format = "uuid")]
    pub id: RateCardId,
    pub kind: RateCardKind,
    pub currency: Currency,
    pub version: u32,
    pub margin: Margin,
    pub components: Vec<PriceComponent>,
}

impl RateCard {
    /// The component pricing a given dimension at the given modality and context tier, if any.
    #[must_use]
    pub fn component(
        &self,
        dimension: PricingDimension,
        modality: Modality,
        context_tier: ContextTier,
    ) -> Option<&PriceComponent> {
        self.components
            .iter()
            .find(|component| component.matches(dimension, modality, context_tier))
    }

    /// From the versions of one logical rate card, the live one: the highest `version`. `None` if the
    /// set is empty. Callers pass the versions of a single card; pricing always rates against `latest`
    /// unless a specific version is pinned (see [`resolve`](RateCard::resolve)).
    #[must_use]
    pub fn latest(versions: &[RateCard]) -> Option<&RateCard> {
        versions.iter().max_by_key(|card| card.version)
    }

    /// Resolve a pinned `version` from a card's versions, or the [`latest`](RateCard::latest) when
    /// `version` is `None`. Returns `None` if the pinned version is absent (so an event recorded
    /// against version N re-rates deterministically against exactly that card).
    #[must_use]
    pub fn resolve(versions: &[RateCard], version: Option<u32>) -> Option<&RateCard> {
        match version {
            None => Self::latest(versions),
            Some(wanted) => versions.iter().find(|card| card.version == wanted),
        }
    }

    /// Validate the card's structure so malformed pricing config is rejected at sync time, not at
    /// price time: every component must price in the card's currency, no unit price may be negative,
    /// and no two components may target the same (dimension, modality, context-tier) cell.
    pub fn validate(&self) -> Result<(), PricingError> {
        let mut seen = std::collections::HashSet::new();
        for component in &self.components {
            if component.unit_price.currency() != &self.currency {
                return Err(PricingError::CurrencyMismatch);
            }
            if component.unit_price.amount().is_sign_negative() {
                return Err(PricingError::NegativePrice(component.dimension));
            }
            component.validate()?;
            let cell = (
                component.dimension,
                component.modality,
                component.context_tier,
            );
            if !seen.insert(cell) {
                return Err(PricingError::DuplicateComponent(component.dimension));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::RateCardId;

    fn card(version: u32) -> RateCard {
        RateCard {
            id: RateCardId::new(),
            kind: RateCardKind::ProviderCost,
            currency: Currency::new("USD").expect("valid currency"),
            version,
            margin: Margin::NONE,
            components: vec![],
        }
    }

    #[test]
    fn latest_picks_the_highest_version_regardless_of_order() {
        let versions = vec![card(2), card(5), card(3)];
        assert_eq!(RateCard::latest(&versions).map(|c| c.version), Some(5));
    }

    #[test]
    fn latest_of_empty_is_none() {
        assert!(RateCard::latest(&[]).is_none());
    }

    #[test]
    fn resolve_pins_a_version_or_falls_back_to_latest() {
        let versions = vec![card(1), card(2), card(3)];
        assert_eq!(
            RateCard::resolve(&versions, Some(2)).map(|c| c.version),
            Some(2)
        );
        assert_eq!(
            RateCard::resolve(&versions, None).map(|c| c.version),
            Some(3)
        );
        assert!(RateCard::resolve(&versions, Some(99)).is_none());
    }

    fn component(dimension: PricingDimension, price: rust_decimal::Decimal) -> PriceComponent {
        use crate::component::ChargeModel;
        use crate::dimension::Unit;
        use meter_core::Money;
        PriceComponent {
            dimension,
            modality: Modality::Text,
            context_tier: ContextTier::Standard,
            unit: Unit::Token,
            charge_model: ChargeModel::Standard,
            unit_price: Money::new(price, Currency::new("USD").expect("usd")),
        }
    }

    #[test]
    fn validate_accepts_a_well_formed_card() {
        use rust_decimal_macros::dec;
        let mut card = card(1);
        card.components = vec![
            component(PricingDimension::InputUncached, dec!(0.000003)),
            component(PricingDimension::Output, dec!(0.000015)),
        ];
        assert_eq!(card.validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_currency_mismatch() {
        use meter_core::Money;
        use rust_decimal_macros::dec;
        let mut card = card(1);
        card.components = vec![PriceComponent {
            unit_price: Money::new(dec!(0.01), Currency::new("EUR").expect("eur")),
            ..component(PricingDimension::Output, dec!(0.01))
        }];
        assert_eq!(card.validate(), Err(PricingError::CurrencyMismatch));
    }

    #[test]
    fn validate_rejects_negative_price() {
        use rust_decimal_macros::dec;
        let mut card = card(1);
        card.components = vec![component(PricingDimension::Output, dec!(-0.01))];
        assert_eq!(
            card.validate(),
            Err(PricingError::NegativePrice(PricingDimension::Output))
        );
    }

    #[test]
    fn validate_rejects_duplicate_cells() {
        use rust_decimal_macros::dec;
        let mut card = card(1);
        card.components = vec![
            component(PricingDimension::Output, dec!(0.01)),
            component(PricingDimension::Output, dec!(0.02)),
        ];
        assert_eq!(
            card.validate(),
            Err(PricingError::DuplicateComponent(PricingDimension::Output))
        );
    }
}
