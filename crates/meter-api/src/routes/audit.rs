//! Audit log: middleware that records every mutating request, and a listing endpoint.

use axum::extract::{Query, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use meter_store_ch::{AuditEntry, AuditFilter};

use crate::error::ApiError;
use crate::AppState;

const fn is_mutating(method: &Method) -> bool {
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
    let request_id = request
        .extensions()
        .get::<super::request_id::RequestId>()
        .map(|id| id.0.clone())
        .unwrap_or_default();

    let response = next.run(request).await;

    if is_mutating(&method) {
        let status = i32::from(response.status().as_u16());
        let _ = state
            .audit
            .record_audit(&actor, method.as_str(), &path, status, &request_id)
            .await;
    }
    response
}

/// `?limit=<n>&actor=&method=&since=<rfc3339>&until=<rfc3339>` (limit default 100, capped at 1000)
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    /// RFC3339 lower/upper bounds (parsed in the handler).
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
}

const fn default_limit() -> i64 {
    100
}

/// Parse an optional RFC3339 query bound.
fn parse_bound(value: Option<String>, field: &str) -> Result<Option<OffsetDateTime>, ApiError> {
    value
        .map(|raw| {
            OffsetDateTime::parse(&raw, &Rfc3339)
                .map_err(|_| ApiError::unprocessable(format!("invalid {field}: {raw}")))
        })
        .transpose()
}

/// `GET /v1/audit` — newest-first, optionally filtered by actor, method, and time window.
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    let limit = query.limit.clamp(1, 1000);
    let filter = AuditFilter {
        actor: query.actor,
        method: query.method,
        since: parse_bound(query.since, "since")?,
        until: parse_bound(query.until, "until")?,
    };
    let entries = state
        .audit
        .list_audit(limit, &filter)
        .await
        .map_err(|error| ApiError::internal(format!("audit: {error}")))?;
    Ok(Json(entries))
}
