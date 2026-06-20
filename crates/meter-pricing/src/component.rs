//! Price components: one priced cell of a rate card's dimensional matrix.

use meter_core::Money;
use serde::{Deserialize, Serialize};

use crate::dimension::{ContextTier, Modality, PricingDimension, Unit};

/// How a component computes its charge. v1 is flat per-unit pricing; graduated/volume/package models
/// are added later behind this enum without changing call sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeModel {
    Standard,
}

/// One priced cell: a (dimension, modality, context-tier) charged at `unit_price` per `unit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceComponent {
    pub dimension: PricingDimension,
    pub modality: Modality,
    pub context_tier: ContextTier,
    pub unit: Unit,
    pub charge_model: ChargeModel,
    pub unit_price: Money,
}

impl PriceComponent {
    /// Whether this component prices the given dimension at the given modality and context tier.
    #[must_use]
    pub fn matches(
        &self,
        dimension: PricingDimension,
        modality: Modality,
        context_tier: ContextTier,
    ) -> bool {
        self.dimension == dimension
            && self.modality == modality
            && self.context_tier == context_tier
    }
}
