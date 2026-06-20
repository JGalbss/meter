//! Usage: the metered quantities of one event, to be priced against a rate card.

use rust_decimal::Decimal;

use crate::dimension::{ContextTier, Modality, PricingDimension};

/// The metered quantities of a single usage event. The modality and context tier select which
/// price components apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub modality: Modality,
    pub context_tier: ContextTier,
    pub quantities: Vec<(PricingDimension, Decimal)>,
}

impl Usage {
    /// An empty usage at the given modality and context tier.
    #[must_use]
    pub const fn new(modality: Modality, context_tier: ContextTier) -> Self {
        Self {
            modality,
            context_tier,
            quantities: Vec::new(),
        }
    }

    /// Builder: add a metered quantity for a dimension.
    #[must_use]
    pub fn with(mut self, dimension: PricingDimension, quantity: Decimal) -> Self {
        self.quantities.push((dimension, quantity));
        self
    }
}
