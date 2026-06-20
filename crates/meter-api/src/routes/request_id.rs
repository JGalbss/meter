//! Request-id propagation for log/trace correlation: every request gets an `x-request-id` (reusing a
//! caller-supplied one, else a fresh UUIDv7), logged with the request line and echoed on the response.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

const HEADER: &str = "x-request-id";

/// The request's correlation id, stored in request extensions so inner layers (e.g. the audit log)
/// and handlers can read it.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Middleware: tag the request/response with a correlation id and log the request line.
pub async fn propagate(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    tracing::info!(
        request_id = %id,
        method = %request.method(),
        path = %request.uri().path(),
        "request"
    );
    // Make the id available to inner middleware/handlers.
    request.extensions_mut().insert(RequestId(id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(HEADER, value);
    }
    response
}
