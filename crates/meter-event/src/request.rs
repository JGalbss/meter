//! Operation inputs for the event store.

use meter_core::{AccountId, EventId, OrgId, RunId};
use serde_json::Value;
use time::OffsetDateTime;

use crate::event::{Event, EventStatus};
use crate::store::idempotent_event_id;

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

impl RecordEvent {
    /// Build the freshly-recorded [`Event`] for this request.
    ///
    /// The id is content-addressed from `(org_id, idempotency_key)` (see [`idempotent_event_id`]), so
    /// every backend assigns the same id to the same key — the foundation of read-free idempotency.
    /// `created_at` is stamped now; status is `Recorded`; it supersedes nothing.
    #[must_use]
    pub fn into_event(self) -> Event {
        Event {
            id: idempotent_event_id(self.org_id, &self.idempotency_key),
            org_id: self.org_id,
            idempotency_key: self.idempotency_key,
            event_time: self.event_time,
            meter: self.meter,
            account_id: self.account_id,
            run_id: self.run_id,
            properties: self.properties,
            status: EventStatus::Recorded,
            supersedes: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Amend an event: record a new version superseding `event_id` with the given properties.
#[derive(Debug, Clone)]
pub struct AmendEvent {
    pub event_id: EventId,
    pub properties: Value,
}
