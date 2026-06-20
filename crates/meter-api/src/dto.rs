//! Request bodies. Responses reuse the domain types directly (they already derive serde).

use meter_core::{AccountId, Credit, EntryId, OrgId, RunId};
use meter_event::RecordEvent;
use meter_ledger::{AccountScope, CreditSource, LimitClass, ReservationId};
use meter_pricing::{ContextTier, Modality, PricingDimension, Usage};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

/// `POST /v1/accounts`
#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenAccountBody {
    #[schema(value_type = String, format = "uuid")]
    pub org_id: OrgId,
    /// Account scope: `org` | `team` | `user` | `product` | `session` | `promo` | `paid`.
    #[schema(value_type = String)]
    pub scope: AccountScope,
    #[serde(default)]
    pub no_overdraft: bool,
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_id: Option<AccountId>,
}

/// `POST /v1/accounts/{id}/grants`
#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantBody {
    /// Credit amount as an exact decimal string.
    #[schema(value_type = String)]
    pub amount: Credit,
    /// Credit source: `paid` | `promo` | `grant`.
    #[schema(value_type = String)]
    pub source: CreditSource,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// `POST /v1/accounts/{id}/credit-notes` — credit an account back (a refund / correction).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RefundBody {
    /// Credit amount as an exact decimal string.
    #[schema(value_type = String)]
    pub amount: Credit,
    /// The original ledger entry being reversed (UUID), if known.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub reverses_entry_id: Option<EntryId>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// `POST /v1/leases`
#[derive(Debug, Deserialize, ToSchema)]
pub struct OpenLeaseBody {
    #[schema(value_type = String, format = "uuid")]
    pub parent: AccountId,
    #[schema(value_type = String)]
    pub amount: Credit,
}

/// `POST /v1/reservations`
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReserveBody {
    #[schema(value_type = String, format = "uuid")]
    pub account: AccountId,
    #[schema(value_type = String, format = "uuid")]
    pub reservation_id: ReservationId,
    #[schema(value_type = String)]
    pub amount: Credit,
    /// Limit class: `hard` (fail-closed) or `soft` (fail-open).
    #[schema(value_type = String)]
    pub limit: LimitClass,
    /// Optional hold expiry (RFC3339); an open hold past it is released by the sweep.
    #[serde(with = "time::serde::rfc3339::option", default)]
    #[schema(value_type = Option<String>, format = "date-time")]
    pub expires_at: Option<OffsetDateTime>,
    /// Optional agent run this hold belongs to; lets `POST /v1/runs/{id}/void` reverse it.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = "uuid")]
    pub run_id: Option<RunId>,
}

/// `POST /v1/reservations/{id}/settle`
#[derive(Debug, Deserialize, ToSchema)]
pub struct SettleBody {
    /// Actual spend to charge, as an exact decimal string.
    #[schema(value_type = String)]
    pub actual: Credit,
}

/// `POST /v1/reservations/{id}/extend` — push the hold's expiry forward (heartbeat).
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExtendBody {
    /// New hold expiry (RFC3339).
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub expires_at: OffsetDateTime,
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

impl UsageDimensions {
    /// Build a priceable [`Usage`] (text, standard context) from these token counts, including only
    /// positive quantities — a catalog card need not have a component for every dimension.
    #[must_use]
    pub fn to_usage(&self) -> Usage {
        let mut usage = Usage::new(Modality::Text, ContextTier::Standard);
        let dimensions = [
            (PricingDimension::InputUncached, self.input_uncached),
            (PricingDimension::CacheRead, self.cache_read),
            (PricingDimension::CacheWrite, self.cache_write),
            (PricingDimension::Output, self.output),
        ];
        for (dimension, quantity) in dimensions {
            if quantity > 0 {
                usage = usage.with(dimension, Decimal::from(quantity));
            }
        }
        usage
    }
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
    /// Optional synced rate-card id to price against; defaults to the catalog card for `model`.
    #[serde(default)]
    pub rate_card_id: Option<String>,
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

/// `POST /v1/usage/reserve` — price a worst-case usage estimate against a catalog model and place a
/// hold before the call. The engine computes the credits (ADR 0001), not the caller.
#[derive(Debug, Deserialize)]
pub struct ReserveUsageBody {
    pub account: AccountId,
    pub reservation_id: ReservationId,
    pub model: String,
    #[serde(default)]
    pub estimate: UsageDimensions,
    pub limit: LimitClass,
    /// Optional synced rate-card id to price against; defaults to the catalog card for `model`.
    #[serde(default)]
    pub rate_card_id: Option<String>,
    /// Optional agent run this hold belongs to; lets `POST /v1/runs/{id}/void` reverse it.
    #[serde(default)]
    pub run_id: Option<RunId>,
}

/// `POST /v1/usage/reservations/{id}/settle` — price the actual usage against a catalog model and
/// settle the reservation. Idempotent on the reservation id.
#[derive(Debug, Deserialize)]
pub struct SettleUsageBody {
    pub model: String,
    #[serde(default)]
    pub actual: UsageDimensions,
    /// Optional synced rate-card id to price against; defaults to the catalog card for `model`.
    #[serde(default)]
    pub rate_card_id: Option<String>,
}
