//! Ledger accounts.

use meter_core::AccountId;
use serde::{Deserialize, Serialize};

/// What a ledger account represents within the org hierarchy and the credit machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountScope {
    Org,
    Team,
    User,
    Product,
    /// A per-session/per-agent-run sub-balance leased from a parent pool (hot-account mitigation).
    Session,
    Promo,
    Paid,
    Budget,
    /// Bounded sink for spend that exceeded its reservation (recorded, alerted, never silent).
    Overage,
    FxClearing,
    /// The backstop account every transfer pairs against (mint + usage sink).
    System,
}

/// A node in the ledger that carries a credit balance, optionally leased from a parent account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerAccount {
    pub id: AccountId,
    pub scope: AccountScope,
    /// When true, a HARD reservation can never drive available credits negative.
    pub no_overdraft: bool,
    /// The parent pool this account leases from, if it is a session sub-balance.
    pub parent_id: Option<AccountId>,
}
