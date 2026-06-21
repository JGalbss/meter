//! Operation inputs and outcomes for the ledger API.
//!
//! These are the verbs (`grant`, `reserve`, `settle`, …) as data, kept separate from the model nouns.

use meter_core::{AccountId, Credit, EntryId, OrgId, RunId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

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
    /// The agent run this hold belongs to, if any. Tagging the hold lets
    /// [`void_run`](crate::LedgerBackend::void_run) reverse a whole run's financial impact:
    /// release its open holds and refund its settled charges.
    pub run_id: Option<RunId>,
}

/// The result of a [`ReserveRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ReserveOutcome {
    /// The hold was placed (or already existed); the caller may proceed to spend.
    Allowed {
        #[schema(value_type = String, format = "uuid")]
        reservation: ReservationId,
    },
    /// A HARD limit refused the spend; the call must not proceed.
    Denied {
        #[schema(value_type = String)]
        available: Credit,
        #[schema(value_type = String)]
        requested: Credit,
    },
}

/// The result of voiding a whole run via [`void_run`](crate::LedgerBackend::void_run): how many open
/// holds were released, how many settled charges were refunded, and the total credits returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct RunVoidSummary {
    /// Open holds released back to available balance.
    pub holds_released: u64,
    /// Settled charges reversed with a refund posting.
    pub charges_refunded: u64,
    /// Total credits returned to the account (the sum of the refunded charges).
    #[schema(value_type = String)]
    pub credits_refunded: Credit,
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
    /// The agent run this charge belongs to, if any. Tagging it lets
    /// [`void_run`](crate::LedgerBackend::void_run) reverse a failed run's direct charges.
    pub run_id: Option<RunId>,
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
