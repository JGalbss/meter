//! Ledger accounts.

use meter_core::{AccountId, OrgId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The well-known org that owns the system (mint + usage sink) account. Not a real tenant; it is the
/// counter-party every grant and settle pairs against so the global ledger always sums to zero.
pub const SYSTEM_ORG: OrgId = OrgId::from_uuid(Uuid::nil());

/// The well-known system account (mint + usage sink) that every transfer pairs against. Fixed so all
/// backends record the same counter-party.
pub const SYSTEM_ACCOUNT: AccountId = AccountId::from_uuid(Uuid::nil());

/// What a ledger account represents within the org hierarchy and the credit machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LedgerAccount {
    #[schema(value_type = String, format = "uuid")]
    pub id: AccountId,
    /// The tenant that owns this account ([`SYSTEM_ORG`] for the system account).
    #[schema(value_type = String, format = "uuid")]
    pub org_id: OrgId,
    pub scope: AccountScope,
    /// When true, a HARD reservation can never drive available credits negative.
    pub no_overdraft: bool,
    /// The parent pool this account leases from, if it is a session sub-balance.
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_id: Option<AccountId>,
}
