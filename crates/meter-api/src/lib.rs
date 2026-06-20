//! The meter engine's HTTP API surface.
//!
//! A thin axum layer over the ledger: it deserializes requests into the domain types (which already
//! carry serde), calls the [`meter_ledger::LedgerBackend`], and serializes the domain results back.
//! Pricing/enforcement endpoints (usage-based) layer on once rate-card config exists.

#![forbid(unsafe_code)]

mod dto;
mod error;
mod routes;

use meter_store_pg::PgLedger;

pub use routes::router;

/// Shared handler state.
#[derive(Clone)]
pub struct AppState {
    pub ledger: PgLedger,
}

impl AppState {
    /// Build state over a Postgres ledger.
    #[must_use]
    pub fn new(ledger: PgLedger) -> Self {
        Self { ledger }
    }
}
