//! Operation inputs for the event store.

use meter_core::{AccountId, EventId, OrgId, RunId};
use serde_json::Value;
use time::OffsetDateTime;

/// Record a usage event. Idempotent on `(org_id, idempotency_key)`.
#[derive(Debug, Clone)]
pub struct RecordEvent {
    pub org_id: OrgId,
    pub idempotency_key: String,
    pub event_time: OffsetDateTime,
    pub meter: String,
    pub account_id: AccountId,
    pub run_id: Option<RunId>,
    pub properties: Value,
}

/// Amend an event: record a new version superseding `event_id` with the given properties.
#[derive(Debug, Clone)]
pub struct AmendEvent {
    pub event_id: EventId,
    pub properties: Value,
}
