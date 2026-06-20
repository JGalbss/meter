//! The event store trait — the seam every event backend implements.

use async_trait::async_trait;
use meter_core::{AccountId, EventId, OrgId, RunId};

use crate::error::EventError;
use crate::event::Event;
use crate::request::{AmendEvent, RecordEvent};

/// The stable, content-addressed id for an event with idempotency `key` in org `org`.
///
/// Idempotency is keyed on `(org_id, idempotency_key)`, so deriving the event id from exactly those
/// two inputs makes re-recording the same key produce the same id. Backends keyed on the id (e.g. a
/// `ReplacingMergeTree`) then deduplicate without a read-before-write, which is what lets ingest scale.
#[must_use]
pub fn idempotent_event_id(org: OrgId, key: &str) -> EventId {
    EventId::deterministic(org.as_uuid(), key.as_bytes())
}

/// Stores immutable usage events with append-only amendments and run-level voiding.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Record an event. Idempotent on `(org_id, idempotency_key)`.
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError>;

    /// Record many events in one round-trip. Idempotent on `(org_id, idempotency_key)` per event,
    /// exactly like [`record`](Self::record). This is the firehose ingest path — backends override it
    /// with a single bulk write; the default loops [`record`](Self::record) for correctness parity.
    /// Returns the recorded events in request order.
    async fn record_batch(&self, reqs: Vec<RecordEvent>) -> Result<Vec<Event>, EventError> {
        let mut events = Vec::with_capacity(reqs.len());
        for req in reqs {
            events.push(self.record(req).await?);
        }
        Ok(events)
    }

    /// Fetch a specific event version by id.
    async fn get(&self, id: EventId) -> Result<Event, EventError>;

    /// The current (recorded, non-superseded, non-voided) events for an account.
    async fn list_for_account(&self, account: AccountId) -> Result<Vec<Event>, EventError>;

    /// Record a new version superseding an event; the original becomes `Amended`.
    async fn amend(&self, req: AmendEvent) -> Result<Event, EventError>;

    /// Void every current event of a run. Returns the number voided.
    async fn void_run(&self, run: RunId) -> Result<u64, EventError>;
}
