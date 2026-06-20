//! In-memory reference implementation of [`EventStore`] — the conformance oracle.

use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use meter_core::{AccountId, EventId, RunId};
use time::OffsetDateTime;

use crate::error::EventError;
use crate::event::{Event, EventStatus};
use crate::request::{AmendEvent, RecordEvent};
use crate::store::EventStore;

/// An entirely in-memory event store.
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: Mutex<Vec<Event>>,
}

impl InMemoryEventStore {
    /// Create an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Event>> {
        self.events.lock().expect("event store mutex poisoned")
    }

    /// Record one event under an already-held lock, deduplicating on the content-addressed id so the
    /// oracle matches a `ReplacingMergeTree`'s `(org_id, id)` dedup (same key → same id → counted once).
    fn record_locked(events: &mut Vec<Event>, req: RecordEvent) -> Event {
        let event = req.into_event();
        if let Some(existing) = events.iter().find(|stored| stored.id == event.id) {
            return existing.clone();
        }
        events.push(event.clone());
        event
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn record(&self, req: RecordEvent) -> Result<Event, EventError> {
        Ok(Self::record_locked(&mut self.lock(), req))
    }

    async fn record_batch(&self, reqs: Vec<RecordEvent>) -> Result<Vec<Event>, EventError> {
        let mut events = self.lock();
        Ok(reqs
            .into_iter()
            .map(|req| Self::record_locked(&mut events, req))
            .collect())
    }

    async fn get(&self, id: EventId) -> Result<Event, EventError> {
        self.lock()
            .iter()
            .find(|event| event.id == id)
            .cloned()
            .ok_or(EventError::NotFound(id))
    }

    async fn list_for_account(&self, account: AccountId) -> Result<Vec<Event>, EventError> {
        Ok(self
            .lock()
            .iter()
            .filter(|event| event.account_id == account && event.status == EventStatus::Recorded)
            .cloned()
            .collect())
    }

    async fn amend(&self, req: AmendEvent) -> Result<Event, EventError> {
        let mut events = self.lock();
        let original = events
            .iter()
            .find(|event| event.id == req.event_id)
            .cloned()
            .ok_or(EventError::NotFound(req.event_id))?;
        if original.status == EventStatus::Voided {
            return Err(EventError::Voided(req.event_id));
        }
        let new_id = EventId::new();
        let amended = Event {
            id: new_id,
            org_id: original.org_id,
            idempotency_key: format!("{}::amend::{new_id}", original.idempotency_key),
            event_time: original.event_time,
            meter: original.meter,
            account_id: original.account_id,
            run_id: original.run_id,
            properties: req.properties,
            status: EventStatus::Recorded,
            supersedes: Some(original.id),
            created_at: OffsetDateTime::now_utc(),
        };
        if let Some(existing) = events.iter_mut().find(|event| event.id == req.event_id) {
            existing.status = EventStatus::Amended;
        }
        events.push(amended.clone());
        Ok(amended)
    }

    async fn void_run(&self, run: RunId) -> Result<u64, EventError> {
        let mut events = self.lock();
        let mut voided = 0;
        for event in events.iter_mut() {
            if event.run_id == Some(run) && event.status == EventStatus::Recorded {
                event.status = EventStatus::Voided;
                voided += 1;
            }
        }
        Ok(voided)
    }
}

#[cfg(test)]
mod tests {
    use super::InMemoryEventStore;
    use crate::conformance;

    #[tokio::test]
    async fn passes_the_conformance_suite() {
        let store = InMemoryEventStore::new();
        conformance::run_all_scenarios(&store).await;
    }
}
