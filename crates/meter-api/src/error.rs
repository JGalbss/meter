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

fn ledger_status(error: &LedgerError) -> (StatusCode, &'static str) {
    match error {
        LedgerError::AccountNotFound(_) | LedgerError::ReservationNotFound(_) => {
            (StatusCode::NOT_FOUND, "not_found")
        }
        LedgerError::ReservationClosed(_) => (StatusCode::CONFLICT, "conflict"),
        LedgerError::NonPositiveAmount => (StatusCode::BAD_REQUEST, "bad_request"),
        LedgerError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

fn event_status(error: &EventError) -> (StatusCode, &'static str) {
    match error {
        EventError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
        EventError::Voided(_) => (StatusCode::CONFLICT, "conflict"),
        EventError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let ((status, code), message) = match self {
            ApiError::Ledger(error) => (ledger_status(&error), error.to_string()),
            ApiError::Event(error) => (event_status(&error), error.to_string()),
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}
