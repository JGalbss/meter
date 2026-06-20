//! The meter engine's HTTP + gRPC API surface.
//!
//! An axum layer over the engine's stores ([`meter_ledger::LedgerBackend`] for money, the ClickHouse
//! event store for usage): it deserializes requests into the domain types (which already carry serde),
//! calls the stores, and serializes the results back. It also covers the metering loop (price → record
//! → charge), model-priced reservation governance, the rate-card catalog, and re-rate simulation.
//!
//! The same operations are exposed over gRPC from the [`grpc`] module (the `meter.v1` Ledger, Ingest,
//! and Query services), which `meter-engine` serves alongside HTTP.

#![forbid(unsafe_code)]

mod cards;
mod dto;
mod error;
pub mod grpc;
mod routes;

use meter_core::Money;
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;

pub use routes::router;

/// Shared handler state. Money-truth (the ledger) is Postgres; events and the append-only audit log
/// live in `ClickHouse` (ADR 0003/0004) — both high-velocity firehoses kept off the money database.
#[derive(Clone)]
pub struct AppState {
    pub ledger: PgLedger,
    pub events: ChStore,
    pub audit: ChStore,
    /// The cash value of one credit (used to price usage into credits).
    pub credit_value: Money,
}

impl AppState {
    /// Build state over the engine stores, with the credit's cash value.
    #[must_use]
    pub const fn new(
        ledger: PgLedger,
        events: ChStore,
        audit: ChStore,
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
