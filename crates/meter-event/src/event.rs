//! The event model.

use meter_core::{AccountId, EventId, OrgId, RunId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;

/// The lifecycle status of an event version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    /// The current, in-effect version.
    Recorded,
    /// Superseded by a later amended version.
    Amended,
    /// Reversed (e.g. its run was voided).
    Voided,
}

/// One usage event. `properties` is arbitrary customer JSON; `run_id` groups the events of one agent
/// run. Amendments create a new event whose `supersedes` points back to the version it replaces.
///
/// `Event` is not `Eq` because `properties` (a `serde_json::Value`) is only `PartialEq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Event {
    #[schema(value_type = String, format = "uuid")]
    pub id: EventId,
    #[schema(value_type = String, format = "uuid")]
    pub org_id: OrgId,
    pub idempotency_key: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub event_time: OffsetDateTime,
    pub meter: String,
    #[schema(value_type = String, format = "uuid")]
    pub account_id: AccountId,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub run_id: Option<RunId>,
    /// Arbitrary customer JSON.
    #[schema(value_type = Object)]
    pub properties: Value,
    pub status: EventStatus,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub supersedes: Option<EventId>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = "date-time")]
    pub created_at: OffsetDateTime,
}
