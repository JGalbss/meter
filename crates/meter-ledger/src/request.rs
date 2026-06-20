//! Operation inputs and outcomes for the ledger API.
//!
//! These are the verbs (`grant`, `reserve`, `settle`, …) as data, kept separate from the model nouns.

use meter_core::{AccountId, Credit, EntryId, OrgId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::model::{AccountScope, CreditSource, LimitClass, ReservationId};

/// Open a new ledger account.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub org_id: OrgId,
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

/// Credit an account back (a credit-note / refund for a correction). Adds credits, referencing the
/// entry it reverses when known. Idempotent on `idempotency_key`.
#[derive(Debug, Clone)]
pub struct RefundRequest {
    pub account: AccountId,
    pub amount: Credit,
    /// The original entry being reversed, if known (for audit / rev-rec).
    pub reverses_entry_id: Option<EntryId>,
    pub idempotency_key: Option<String>,
}

/// Place a durable hold before a spend. Idempotent on `reservation_id`.
#[derive(Debug, Clone)]
pub struct ReserveRequest {
    pub account: AccountId,
    pub reservation_id: ReservationId,
    pub amount: Credit,
    pub limit: LimitClass,
    /// Optional expiry; an open hold past this instant is released by
    /// [`void_expired_holds`](crate::LedgerBackend::void_expired_holds). `None` never expires.
    pub expires_at: Option<OffsetDateTime>,
}

/// The result of a [`ReserveRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
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

/// Charge usage directly, without a prior reservation (post-hoc metering). Always posts — the usage
/// already happened — so it can drive a balance negative (overage). Idempotent on `idempotency_key`.
#[derive(Debug, Clone)]
pub struct ChargeRequest {
    pub account: AccountId,
    pub amount: Credit,
    pub idempotency_key: Option<String>,
}

/// Lease `amount` credits from a parent pool into a fresh per-session sub-balance, to keep hot-account
/// contention off the parent. The session then reserves/settles against the lease; `close_lease`
/// returns the unused remainder to the parent.
#[derive(Debug, Clone)]
pub struct LeaseRequest {
    pub parent: AccountId,
    pub amount: Credit,
}
