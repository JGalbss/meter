//! Health endpoints: liveness (always) and readiness (stores reachable).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// `GET /health` — liveness; static "the process is up and serving".
///
/// Deliberately independent of the stores, so a transient database blip never trips a liveness probe
/// into a restart loop.
#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "The process is up")),
    tag = "health"
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// `GET /health/ready` — readiness; pings both stores and reports which is down.
///
/// `200` only when the money store (Postgres) and the event store (ClickHouse) both answer, otherwise
/// `503`. This is the probe a load balancer / k8s should gate traffic on.
#[utoipa::path(
    get,
    path = "/health/ready",
    responses(
        (status = 200, description = "Both stores reachable"),
        (status = 503, description = "A dependency is down")
    ),
    tag = "health"
)]
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let ledger_ok = state.ledger.ping().await.is_ok();
    let events_ok = state.events.ping().await.is_ok();
    let status = match ledger_ok && events_ok {
        true => StatusCode::OK,
        false => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(json!({ "ledger": ledger_ok, "events": events_ok })),
    )
}
