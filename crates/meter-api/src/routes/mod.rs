//! Route assembly.

mod accounts;
mod analytics;
mod audit;
mod budgets;
mod events;
mod health;
mod invoices;
mod reservations;
mod usage;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

/// The engine's HTTP router.
pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/accounts", post(accounts::open_account))
        .route("/accounts/:id/balance", get(accounts::balance))
        .route("/accounts/:id/grants", post(accounts::grant))
        .route("/accounts/:id/entries", get(accounts::entries))
        .route("/accounts/:id/events", get(events::list_for_account))
        .route("/accounts/:id/invoice", get(invoices::invoice))
        .route("/accounts/:id/budget", get(budgets::budget_status))
        .route("/accounts/:id/usage-by-day", get(analytics::usage_by_day))
        .route("/reservations", post(reservations::reserve))
        .route("/reservations/:id/settle", post(reservations::settle))
        .route("/reservations/:id/void", post(reservations::void))
        .route("/events", post(events::record))
        .route("/events/:id", get(events::get))
        .route("/events/:id/amend", post(events::amend))
        .route("/runs/:id/void", post(events::void_run))
        .route("/usage", post(usage::meter_usage))
        .route("/audit", get(audit::list));

    Router::new()
        .route("/health", get(health::health))
        .nest("/v1", v1)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            audit::audit_middleware,
        ))
        .with_state(state)
}
