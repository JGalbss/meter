//! Health endpoints: liveness (always) and readiness (stores reachable).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// `GET /health` — liveness. Static: the process is up and serving. Deliberately independent of the
/// stores, so a transient database blip never trips a liveness probe into a restart loop.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// `GET /health/ready` — readiness. Pings the money store (Postgres) and the event store (ClickHouse);
/// `200` only when both answer, otherwise `503` reporting which dependency is down. This is the probe a
/// load balancer / k8s should gate traffic on.
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
