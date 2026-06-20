//! Route assembly.

mod accounts;
mod analytics;
mod audit;
mod budgets;
mod catalog;
mod events;
mod health;
mod invoices;
mod leases;
mod metrics;
pub(crate) mod openapi;
mod rate_cards;
mod request_id;
mod reservations;
mod simulate;
mod usage;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::AppState;

/// The engine's HTTP router.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/accounts", post(accounts::open_account))
        .route("/accounts/:id/balance", get(accounts::balance))
        .route("/accounts/:id/grants", post(accounts::grant))
        .route("/accounts/:id/credit-notes", post(accounts::credit_note))
        .route("/accounts/:id/entries", get(accounts::entries))
        .route("/accounts/:id/events", get(events::list_for_account))
        .route("/accounts/:id/invoice", get(invoices::invoice))
        .route("/accounts/:id/budget", get(budgets::budget_status))
        .route("/accounts/:id/usage-by-day", get(analytics::usage_by_day))
        .route("/orgs/:id/usage-by-model", get(analytics::usage_by_model))
        .route("/orgs/:id/usage-by-field", get(analytics::usage_by_field))
        .route("/orgs/:id/usage-by-day", get(analytics::org_usage_by_day))
        .route("/orgs/:id/event-count", get(analytics::event_count))
        .route("/reservations", post(reservations::reserve))
        .route("/reservations/:id/settle", post(reservations::settle))
        .route("/reservations/:id/void", post(reservations::void))
        .route("/reservations/:id/extend", post(reservations::extend))
        .route("/leases", post(leases::open_lease))
        .route("/leases/:id/close", post(leases::close_lease))
        .route("/events", post(events::record))
        .route("/events/batch", post(events::record_batch))
        .route("/events/:id", get(events::get))
        .route("/events/:id/amend", post(events::amend))
        .route("/runs/:id/void", post(events::void_run))
        .route("/usage", post(usage::meter_usage))
        .route("/usage/reserve", post(usage::reserve_usage))
        .route("/usage/reservations/:id/settle", post(usage::settle_usage))
        .route("/simulate", post(simulate::simulate))
        .route("/catalog", get(catalog::list))
        .route("/catalog/:model_id", get(catalog::get_card))
        .route("/rate-cards", get(rate_cards::list))
        .route("/rate-cards/:id", get(rate_cards::get))
        .route("/audit", get(audit::list));

    Router::new()
        .route("/health", get(health::health))
        .route("/health/ready", get(health::ready))
        .route("/openapi.json", get(openapi::openapi_json))
        .route("/metrics", get(metrics::metrics))
        .nest("/v1", v1)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            audit::audit_middleware,
        ))
        // Count every request/response by status for the /metrics endpoint.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics::record_metrics,
        ))
        // Every request/response carries a correlation id.
        .layer(middleware::from_fn(request_id::propagate))
        // Outermost: an info-level span per request with method, path, status, and latency, so the
        // engine emits structured request traces at the default log level (EPIC 16 observability).
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .with_state(state)
}
