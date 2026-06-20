//! Pricing for meter: rate cards, pricing dimensions, and token-to-credit translation.
//!
//! The model is deliberately small. A [`RateCard`] holds versioned, dimensional [`PriceComponent`]s
//! and a [`Margin`]. [`Usage`] is the metered quantities of one event. The functions in [`price`]
//! turn usage into a cost (COGS), a customer price (margin applied), and the credits to charge —
//! rounding exactly once at the credit layer.

#![forbid(unsafe_code)]

pub mod card;
pub mod component;
pub mod dimension;
pub mod error;
pub mod price;
pub mod simulate;
pub mod usage;

pub use card::{Margin, RateCard, RateCardKind};
pub use component::{ChargeModel, PriceComponent};
pub use dimension::{ContextTier, Modality, PricingDimension, Unit};
pub use error::PricingError;
pub use price::{apply_margin, cost, price_usage, to_credits, PricedUsage};
pub use simulate::{rerate_event, simulate_rerate, RerateLine, RerateSummary};
pub use usage::Usage;
