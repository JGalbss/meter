//! Operation inputs and outcomes for the ledger API.
//!
//! These are the verbs (`grant`, `reserve`, `settle`, …) as data, kept separate from the model nouns.

use meter_core::{AccountId, Credit};

use crate::model::{AccountScope, CreditSource, LimitClass, ReservationId};

/// Open a new ledger account.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub scope: AccountScope,
    pub no_overdraft: bool,
    pub parent_id: Option<AccountId>,
}

/// Grant credits into an account. Idempotent on `idempotency_key` when supplied.
#[derive(Debug, Clone)]
pub struct GrantRequest {
    pub account: AccountId,
    pub amount: Credit,
    pub source: CreditSource,
    pub idempotency_key: Option<String>,
}

/// Place a durable hold before a spend. Idempotent on `reservation_id`.
#[derive(Debug, Clone)]
pub struct ReserveRequest {
    pub account: AccountId,
    pub reservation_id: ReservationId,
    pub amount: Credit,
    pub limit: LimitClass,
}

/// The result of a [`ReserveRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveOutcome {
    /// The hold was placed (or already existed); the caller may proceed to spend.
    Allowed { reservation: ReservationId },
    /// A HARD limit refused the spend; the call must not proceed.
    Denied {
        available: Credit,
        requested: Credit,
    },
}

/// Settle a prior reservation with the actual spend. Idempotent on `reservation_id`.
#[derive(Debug, Clone)]
pub struct SettleRequest {
    pub reservation_id: ReservationId,
    pub actual: Credit,
}
