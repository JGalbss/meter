//! The event model.

use meter_core::{AccountId, EventId, OrgId, RunId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

/// The lifecycle status of an event version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub org_id: OrgId,
    pub idempotency_key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub event_time: OffsetDateTime,
    pub meter: String,
    pub account_id: AccountId,
    pub run_id: Option<RunId>,
    pub properties: Value,
    pub status: EventStatus,
    pub supersedes: Option<EventId>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
