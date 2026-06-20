//! Event store errors.

use meter_core::EventId;
use thiserror::Error;

/// Errors returned by an [`crate::EventStore`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EventError {
    /// No event exists with the given id.
    #[error("event not found: {0}")]
    NotFound(EventId),
    /// The event is voided and cannot be amended.
    #[error("event {0} is voided and cannot be amended")]
    Voided(EventId),
    /// A backend-specific failure.
    #[error("backend error: {0}")]
    Backend(String),
}
