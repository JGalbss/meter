//! The meter engine's HTTP API surface.
//!
//! A thin axum layer over the ledger: it deserializes requests into the domain types (which already
//! carry serde), calls the [`meter_ledger::LedgerBackend`], and serializes the domain results back.
//! Pricing/enforcement endpoints (usage-based) layer on once rate-card config exists.

#![forbid(unsafe_code)]

mod dto;
mod error;
mod routes;

use meter_core::Money;
use meter_store_ch::ChStore;
use meter_store_pg::{PgAuditLog, PgLedger};

pub use routes::router;

/// Shared handler state. Money-truth (ledger, audit) is Postgres; events live in `ClickHouse` (ADR 0003).
#[derive(Clone)]
pub struct AppState {
    pub ledger: PgLedger,
    pub events: ChStore,
    pub audit: PgAuditLog,
    /// The cash value of one credit (used to price usage into credits).
    pub credit_value: Money,
}

impl AppState {
    /// Build state over the engine stores, with the credit's cash value.
    #[must_use]
    pub const fn new(
        ledger: PgLedger,
        events: ChStore,
        audit: PgAuditLog,
        credit_value: Money,
    ) -> Self {
        Self {
            ledger,
            events,
            audit,
            credit_value,
        }
    }
}
