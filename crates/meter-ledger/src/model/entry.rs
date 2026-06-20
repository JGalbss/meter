//! Ledger entries — the immutable, append-only postings.

use meter_core::{AccountId, Credit, EntryId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::reservation::ReservationId;

/// The kind of a ledger posting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    /// Credits added to an account (a grant / top-up / prepaid block).
    Grant,
    /// Credits consumed by usage (the priced result of an event).
    Usage,
    /// A pending hold placed by `reserve`.
    ReservationHold,
    /// The priced posting that closes a reservation with the actual amount.
    Settle,
    /// The unused remainder returned when actual < reserved.
    PartialReturn,
    /// A released (cancelled) reservation.
    Void,
    /// A reversal that returns credits (references the entry it reverses).
    Refund,
    /// A reversal driven by an external dispute.
    Chargeback,
    /// Credits removed because a block expired.
    Expiration,
    /// A correction to a prior entry (the clean "edit").
    Amendment,
    /// A foreign-exchange leg between two currency ledgers.
    Fx,
    /// A posting that seals an invoice to the ledger at finalization.
    Sealing,
}

/// Provenance of credits, carried onto every entry so revenue recognition can split real product
/// margin from promotional spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreditSource {
    Paid,
    Promo,
    Grant,
}

/// One immutable transfer between two accounts. Never updated after creation; corrections are new
/// entries that point back via [`LedgerEntry::reverses_entry_id`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: EntryId,
    /// The account whose balance moves by `delta_credits`.
    pub account_id: AccountId,
    /// The counter-account the equal-and-opposite delta posts to (double-entry).
    pub paired_account_id: AccountId,
    pub entry_type: EntryType,
    /// Signed change to `account_id`'s settled balance.
    pub delta_credits: Credit,
    /// `account_id`'s settled balance immediately after this entry (stored so audits never replay).
    pub balance_after: Credit,
    /// Provenance, when the entry concerns a credit pool.
    pub source: Option<CreditSource>,
    /// Whether the credits on this entry are recognizable revenue.
    pub revenue_recognizable: bool,
    /// For reversals/amendments: the entry this one corrects.
    pub reverses_entry_id: Option<EntryId>,
    /// The reservation this entry belongs to, when part of a reserve→settle flow.
    pub reservation_id: Option<ReservationId>,
    /// The client idempotency key that produced this entry, when applicable.
    pub idempotency_key: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
