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
///
/// Idempotent on `idempotency_key` when supplied (like [`crate::RecordEvent`], and like the ledger's
/// grant/charge/refund): a retried amend with the same key resolves to the same new event id, so the
/// store dedups it instead of stacking a second version. Without a key, each call mints a distinct
/// version (back-compatible behaviour).
#[derive(Debug, Clone)]
pub struct AmendEvent {
    pub event_id: EventId,
    pub properties: Value,
    pub idempotency_key: Option<String>,
}

impl AmendEvent {
    /// Build the new (`Recorded`) event that supersedes `original`. The new event's id is
    /// content-addressed from a per-original amend key (see [`idempotent_event_id`]), so every backend
    /// assigns the same id to the same amendment — making amend idempotent under retries. A supplied
    /// [`idempotency_key`](Self::idempotency_key) makes that key stable; its absence seeds a fresh one,
    /// preserving the prior "new version each call" behaviour. `event_time`/`meter`/`account`/`run` are
    /// carried over from the original; `created_at` is stamped now.
    #[must_use]
    pub fn into_amended_event(self, original: &Event) -> Event {
        let amend_key = match self.idempotency_key {
            Some(key) => format!("{}::amend::{key}", original.idempotency_key),
            None => format!("{}::amend::{}", original.idempotency_key, EventId::new()),
        };
        Event {
            id: idempotent_event_id(original.org_id, &amend_key),
            org_id: original.org_id,
            idempotency_key: amend_key,
            event_time: original.event_time,
            meter: original.meter.clone(),
            account_id: original.account_id,
            run_id: original.run_id,
            properties: self.properties,
            status: EventStatus::Recorded,
            supersedes: Some(original.id),
            created_at: OffsetDateTime::now_utc(),
        }
    }
}
