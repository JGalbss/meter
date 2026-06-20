//! Metrics endpoint + recording middleware (see [`crate::metrics`]).

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// Records every completed request by its response status into [`AppState::metrics`].
pub async fn record_metrics(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let response = next.run(request).await;
    state.metrics.record(response.status().as_u16());
    response
}

/// `GET /metrics` — Prometheus text exposition of the engine's request counters.
pub async fn metrics(State(state): State<AppState>) -> String {
    state.metrics.render()
}
