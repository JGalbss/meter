//! Request bodies. Responses reuse the domain types directly (they already derive serde).

use meter_core::{AccountId, Credit, OrgId, RunId};
use meter_event::RecordEvent;
use meter_ledger::{AccountScope, CreditSource, LimitClass, ReservationId};
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;

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

/// `POST /v1/leases`
#[derive(Debug, Deserialize)]
pub struct OpenLeaseBody {
    pub parent: AccountId,
    pub amount: Credit,
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

/// `POST /v1/events`
#[derive(Debug, Deserialize)]
pub struct RecordEventBody {
    pub org_id: OrgId,
    pub idempotency_key: String,
    #[serde(with = "time::serde::rfc3339::option", default)]
    pub event_time: Option<OffsetDateTime>,
    pub meter: String,
    pub account: AccountId,
    #[serde(default)]
    pub run_id: Option<RunId>,
    #[serde(default)]
    pub properties: Value,
}

impl RecordEventBody {
    /// Convert an API body into a store request, defaulting `event_time` to now.
    #[must_use]
    pub fn into_request(self) -> RecordEvent {
        RecordEvent {
            org_id: self.org_id,
            idempotency_key: self.idempotency_key,
            event_time: self.event_time.unwrap_or_else(OffsetDateTime::now_utc),
            meter: self.meter,
            account_id: self.account,
            run_id: self.run_id,
            properties: self.properties,
        }
    }
}

/// `POST /v1/events/batch` — the firehose ingest path: many events in one round-trip, written in a
/// single bulk insert. Idempotent per event on `(org_id, idempotency_key)`, exactly like `record`.
#[derive(Debug, Deserialize)]
pub struct RecordBatchBody {
    pub events: Vec<RecordEventBody>,
}

/// `POST /v1/events/{id}/amend`
#[derive(Debug, Deserialize)]
pub struct AmendBody {
    pub properties: Value,
}

/// Token counts for a metered usage event.
#[derive(Debug, Default, Deserialize)]
pub struct UsageDimensions {
    #[serde(default)]
    pub input_uncached: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
}

/// `POST /v1/usage` — price token usage via the catalog, record the event, and charge credits.
#[derive(Debug, Deserialize)]
pub struct MeterUsageBody {
    pub org_id: OrgId,
    pub account: AccountId,
    pub model: String,
    pub idempotency_key: String,
    #[serde(default)]
    pub run_id: Option<RunId>,
    #[serde(default)]
    pub usage: UsageDimensions,
}

/// `POST /v1/simulate` — re-rate a stream of usage events from one catalog model onto another to
/// preview the credit impact of switching, without touching the ledger.
#[derive(Debug, Deserialize)]
pub struct SimulateBody {
    pub current_model: String,
    pub proposed_model: String,
    #[serde(default)]
    pub events: Vec<UsageDimensions>,
}
