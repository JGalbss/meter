//! Mapping domain errors to HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use meter_event::EventError;
use meter_ledger::LedgerError;
use serde_json::json;

/// A handler error, mapped to an HTTP status and a JSON body.
pub enum ApiError {
    Ledger(LedgerError),
    Event(EventError),
    Status(StatusCode, &'static str, String),
}

impl ApiError {
    /// A 404 with a message.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Status(StatusCode::NOT_FOUND, "not_found", message.into())
    }

    /// A 422 with a message (the request was well-formed but could not be processed).
    #[must_use]
    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::Status(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable",
            message.into(),
        )
    }

    /// A 500 with a message.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Status(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            message.into(),
        )
    }
}

impl From<LedgerError> for ApiError {
    fn from(error: LedgerError) -> Self {
        Self::Ledger(error)
    }
}

impl From<EventError> for ApiError {
    fn from(error: EventError) -> Self {
        Self::Event(error)
    }
}

const fn ledger_status(error: &LedgerError) -> (StatusCode, &'static str) {
    match error {
        LedgerError::AccountNotFound(_) | LedgerError::ReservationNotFound(_) => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        LedgerError::ReservationClosed(_) => (StatusCode::CONFLICT, "conflict"),
        LedgerError::NonPositiveAmount | LedgerError::NotALease(_) => {
            (StatusCode::BAD_REQUEST, "bad_request")
        }
        LedgerError::InsufficientFunds { .. } => (StatusCode::CONFLICT, "insufficient_funds"),
        LedgerError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

const fn event_status(error: &EventError) -> (StatusCode, &'static str) {
    match error {
        EventError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        EventError::Voided(_) => (StatusCode::CONFLICT, "conflict"),
        EventError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let ((status, code), message) = match self {
            Self::Ledger(error) => (ledger_status(&error), error.to_string()),
            Self::Event(error) => (event_status(&error), error.to_string()),
            Self::Status(status, code, message) => ((status, code), message),
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}
