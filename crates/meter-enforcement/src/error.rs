//! Enforcement errors.

use meter_ledger::LedgerError;
use meter_pricing::PricingError;
use thiserror::Error;

/// An error from the enforcement layer: either pricing the usage or applying it to the ledger failed.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EnforcementError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Pricing(#[from] PricingError),
}
