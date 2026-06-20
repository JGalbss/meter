//! Audit log: middleware that records every mutating request, and a listing endpoint.

use axum::extract::{Query, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;
use serde::Deserialize;

use meter_store_pg::AuditEntry;

use crate::error::ApiError;
use crate::AppState;

fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Records mutating requests (actor from the `x-meter-actor` header, default `system`). Best-effort:
/// an audit failure never fails the request.
pub async fn audit_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let actor = request
        .headers()
        .get("x-meter-actor")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("system")
        .to_owned();

    let response = next.run(request).await;

    if is_mutating(&method) {
        let status = i32::from(response.status().as_u16());
        let _ = state
            .audit
            .record(&actor, method.as_str(), &path, status)
            .await;
    }
    response
}

/// `?limit=<n>` (default 100, capped at 1000)
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// `GET /v1/audit`
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    let limit = query.limit.clamp(1, 1000);
    let entries = state
        .audit
        .list(limit)
        .await
        .map_err(|error| ApiError::internal(format!("audit: {error}")))?;
    Ok(Json(entries))
}
