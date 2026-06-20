//! Route assembly.

mod accounts;
mod health;
mod reservations;

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
        .route("/reservations", post(reservations::reserve))
        .route("/reservations/:id/settle", post(reservations::settle))
        .route("/reservations/:id/void", post(reservations::void));

    Router::new()
        .route("/health", get(health::health))
        .nest("/v1", v1)
        .with_state(state)
}
