//! Pricing errors.

use thiserror::Error;

use crate::dimension::PricingDimension;

/// Errors raised while pricing usage.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PricingError {
    /// The rate card has no component for a dimension at the usage's modality and context tier.
    #[error("no price component for dimension {0:?} at the given modality and context tier")]
    NoComponent(PricingDimension),
    /// A component or the credit value used a different currency than expected.
    #[error("currency mismatch in pricing")]
    CurrencyMismatch,
    /// The cash value of a credit was zero or negative.
    #[error("credit value must be positive")]
    NonPositiveCreditValue,
}
