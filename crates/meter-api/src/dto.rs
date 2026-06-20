//! Request bodies. Responses reuse the domain types directly (they already derive serde).

use meter_core::{AccountId, Credit, OrgId};
use meter_ledger::{AccountScope, CreditSource, LimitClass, ReservationId};
use serde::Deserialize;

/// `POST /v1/accounts`
#[derive(Debug, Deserialize)]
pub struct OpenAccountBody {
    pub org_id: OrgId,
    pub scope: AccountScope,
    #[serde(default)]
    pub no_overdraft: bool,
    #[serde(default)]
    pub parent_id: Option<AccountId>,
}

/// `POST /v1/accounts/{id}/grants`
#[derive(Debug, Deserialize)]
pub struct GrantBody {
    pub amount: Credit,
    pub source: CreditSource,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// `POST /v1/reservations`
#[derive(Debug, Deserialize)]
pub struct ReserveBody {
    pub account: AccountId,
    pub reservation_id: ReservationId,
    pub amount: Credit,
    pub limit: LimitClass,
}

/// `POST /v1/reservations/{id}/settle`
#[derive(Debug, Deserialize)]
pub struct SettleBody {
    pub actual: Credit,
}
