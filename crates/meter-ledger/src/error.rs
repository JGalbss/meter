//! The ledger error type.

use meter_core::{AccountId, Credit};
use thiserror::Error;

use crate::model::ReservationId;

/// Errors returned by a [`crate::LedgerBackend`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LedgerError {
    /// No account exists with the given id.
    #[error("account not found: {0}")]
    AccountNotFound(AccountId),
    /// No reservation exists with the given id (settle/void of an unknown hold).
    #[error("reservation not found: {0}")]
    ReservationNotFound(ReservationId),
    /// The reservation has already been settled or voided and cannot be reused.
    #[error("reservation is closed (settled or voided): {0}")]
    ReservationClosed(ReservationId),
    /// A grant or reservation amount was not strictly positive.
    #[error("amount must be positive")]
    NonPositiveAmount,
    /// Leasing more than a no-overdraft pool's available balance.
    #[error("insufficient funds: {available} available, {requested} requested")]
    InsufficientFunds { available: Credit, requested: Credit },
    /// `close_lease` was called on an account that is not a lease (it has no parent pool).
    #[error("account is not a lease: {0}")]
    NotALease(AccountId),
    /// A backend-specific failure (storage, network, serialization).
    #[error("backend error: {0}")]
    Backend(String),
}
