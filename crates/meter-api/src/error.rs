//! Mapping domain errors to HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use meter_ledger::LedgerError;
use serde_json::json;

/// Wraps a [`LedgerError`] so it can be returned from a handler as a typed HTTP error.
pub struct ApiError(pub LedgerError);

impl From<LedgerError> for ApiError {
    fn from(error: LedgerError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            LedgerError::AccountNotFound(_) | LedgerError::ReservationNotFound(_) => {
                (StatusCode::NOT_FOUND, "not_found")
            }
            LedgerError::ReservationClosed(_) => (StatusCode::CONFLICT, "conflict"),
            LedgerError::NonPositiveAmount => (StatusCode::BAD_REQUEST, "bad_request"),
            LedgerError::Backend(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        let body = Json(json!({ "error": code, "message": self.0.to_string() }));
        (status, body).into_response()
    }
}
