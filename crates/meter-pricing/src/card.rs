//! Rate cards: versioned, dimensional, priced mappings from usage to money.

use meter_core::{Currency, RateCardId};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::component::PriceComponent;
use crate::dimension::{ContextTier, Modality, PricingDimension};

/// Whether a card records provider cost (COGS) or what a customer is charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateCardKind {
    ProviderCost,
    Customer,
}

/// A markup multiplier applied to provider cost to get the customer price. `1.30` is a 30% markup;
/// [`Margin::NONE`] (`1.00`) charges cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Margin(#[serde(with = "rust_decimal::serde::str")] Decimal);

impl Margin {
    /// No markup (multiplier 1.00).
    pub const NONE: Margin = Margin(Decimal::ONE);

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateCard {
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
}
