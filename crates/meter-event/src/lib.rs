//! Usage events for meter.
//!
//! Events are immutable facts carrying arbitrary custom fields. "Editing" is append-only: [`amend`]
//! records a new version that supersedes the original (the original becomes `Amended`), and
//! [`void_run`] marks every event of a failed run `Voided`. Reads return the latest non-voided
//! version. The [`EventStore`] trait is the seam; the in-memory store is the conformance oracle.
//!
//! [`amend`]: store::EventStore::amend
//! [`void_run`]: store::EventStore::void_run

#![forbid(unsafe_code)]

#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub mod error;
pub mod event;
pub mod memory;
pub mod request;
pub mod store;

pub use error::EventError;
pub use event::{Event, EventStatus};
pub use memory::InMemoryEventStore;
pub use request::{AmendEvent, RecordEvent};
pub use store::EventStore;
