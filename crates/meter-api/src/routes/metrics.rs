//! Metrics endpoint + recording middleware (see [`crate::metrics`]).

use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// Records every completed request — its response status and its latency — into [`AppState::metrics`].
pub async fn record_metrics(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let response = next.run(request).await;
    state.metrics.record(response.status().as_u16());
    state.metrics.record_latency(started.elapsed());
    response
}

/// `GET /metrics` — Prometheus text exposition of the engine's request counters.
pub async fn metrics(State(state): State<AppState>) -> String {
    state.metrics.render()
}
