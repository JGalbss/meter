//! `PostgreSQL` implementation of meter's [`meter_ledger::LedgerBackend`].
//!
//! The append-only double-entry ledger over Postgres. Per-account settled balances are kept on
//! `ledger_accounts` and updated transactionally; reserve/settle serialize on the account row with
//! `SELECT … FOR UPDATE`, which is what makes a HARD limit unable to overdraft under concurrency.
//! This backend is verified against the shared `meter_ledger::conformance` suite (the in-memory
//! reference is the oracle).

#![forbid(unsafe_code)]

mod config;
mod mapping;
mod ops;
mod report;

pub use config::{BudgetRecord, PgConfig, RateCardRecord};
pub use report::{DayUsage, PeriodUsage};

use meter_ledger::LedgerError;
use sqlx::postgres::PgPool;

/// A ledger backed by `PostgreSQL`.
#[derive(Debug, Clone)]
pub struct PgLedger {
    pool: PgPool,
}

impl PgLedger {
    /// Wrap a connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Apply the engine ledger migrations.
    ///
    /// Refuses on version skew: if the database has an applied migration this binary does not ship (a
    /// newer build already migrated ahead), the run fails rather than operating against an unknown
    /// schema. This is sqlx's default (we never `set_ignore_missing`); proven by the `migrate`
    /// integration test.
    pub async fn migrate(&self) -> Result<(), LedgerError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|error| LedgerError::Backend(format!("migrate: {error}")))
    }

    /// The underlying pool (for composing higher-level engine services on the same database).
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Readiness check: confirm the database is reachable and answering queries.
    pub async fn ping(&self) -> Result<(), LedgerError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|error| LedgerError::Backend(format!("ping: {error}")))
    }
}
