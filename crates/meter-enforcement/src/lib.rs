//! Real-time enforcement for meter.
//!
//! Composes [`meter_pricing`] and [`meter_ledger`]: price an estimated usage into credits and place a
//! durable hold *before* the call ([`EnforcementService::reserve_usage`]); price the actual usage and
//! post it on completion ([`EnforcementService::settle_usage`]); release a failed run with
//! [`EnforcementService::void`]. HARD limits are gated by the durable hold and can never overdraft.

#![forbid(unsafe_code)]

pub mod error;
pub mod policy;
pub mod service;

pub use error::EnforcementError;
pub use policy::ReservationPolicy;
pub use service::{EnforcementService, Settlement};
