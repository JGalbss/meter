//! The event store trait — the seam every event backend implements.

use async_trait::async_trait;
use meter_core::{AccountId, EventId, RunId};

use crate::error::EventError;
use crate::event::Event;
use crate::request::{AmendEvent, RecordEvent};

/// Stores immutable usage events with append-only amendments and run-level voiding.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Record an event. Idempotent on `(org_id, idempotency_key)`.
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError>;

    /// Fetch a specific event version by id.
    async fn get(&self, id: EventId) -> Result<Event, EventError>;

    /// The current (recorded, non-superseded, non-voided) events for an account.
    async fn list_for_account(&self, account: AccountId) -> Result<Vec<Event>, EventError>;

    /// Record a new version superseding an event; the original becomes `Amended`.
    async fn amend(&self, req: AmendEvent) -> Result<Event, EventError>;

    /// Void every current event of a run. Returns the number voided.
    async fn void_run(&self, run: RunId) -> Result<u64, EventError>;
}
