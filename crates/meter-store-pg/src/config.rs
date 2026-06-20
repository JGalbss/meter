//! Engine-side storage for control-plane-synced config (rate cards, budgets).
//!
//! Per ADR 0001 the control plane authors config; this is the engine's local copy so it can price and
//! enforce without a round-trip. Shares the engine's Postgres pool ("money + config").

use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::postgres::PgPool;
use sqlx::Row;
use uuid::Uuid;

use meter_ledger::LedgerError;

use crate::mapping::be;

/// One stored rate-card version. `components` is the priced dimensional matrix as JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateCardRecord {
    pub id: Uuid,
    pub version: i32,
    pub kind: String,
    pub currency: String,
    pub margin: Decimal,
    pub components: Value,
}

/// A stored spend limit for an account over a recurring period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetRecord {
    pub account_id: Uuid,
    pub limit_credits: Decimal,
    pub period: String,
}

/// Engine-side config store over the engine's Postgres.
#[derive(Debug, Clone)]
pub struct PgConfig {
    pool: PgPool,
}

impl PgConfig {
    /// Wrap a connection pool (typically the same one as the ledger).
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert a rate-card version (idempotent on `(id, version)`).
    pub async fn put_rate_card(&self, card: &RateCardRecord) -> Result<(), LedgerError> {
        sqlx::query(
            "INSERT INTO rate_cards (id, version, kind, currency, margin, components) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (id, version) DO UPDATE SET \
               kind = $3, currency = $4, margin = $5, components = $6",
        )
        .bind(card.id)
        .bind(card.version)
        .bind(&card.kind)
        .bind(&card.currency)
        .bind(card.margin)
        .bind(&card.components)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    /// The live (highest-version) rate card for a logical id, if any.
    pub async fn latest_rate_card(&self, id: Uuid) -> Result<Option<RateCardRecord>, LedgerError> {
        let row = sqlx::query(
            "SELECT id, version, kind, currency, margin, components \
             FROM rate_cards WHERE id = $1 ORDER BY version DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(RateCardRecord {
                id: row.try_get("id").map_err(be)?,
                version: row.try_get("version").map_err(be)?,
                kind: row.try_get("kind").map_err(be)?,
                currency: row.try_get("currency").map_err(be)?,
                margin: row.try_get("margin").map_err(be)?,
                components: row.try_get("components").map_err(be)?,
            })),
        }
    }

    /// Upsert the current budget for an account (idempotent on `account_id`).
    pub async fn set_budget(&self, budget: &BudgetRecord) -> Result<(), LedgerError> {
        sqlx::query(
            "INSERT INTO budgets (account_id, limit_credits, period) VALUES ($1, $2, $3) \
             ON CONFLICT (account_id) DO UPDATE SET \
               limit_credits = $2, period = $3, updated_at = now()",
        )
        .bind(budget.account_id)
        .bind(budget.limit_credits)
        .bind(&budget.period)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    /// The current budget for an account, if one is set.
    pub async fn budget(&self, account_id: Uuid) -> Result<Option<BudgetRecord>, LedgerError> {
        let row = sqlx::query(
            "SELECT account_id, limit_credits, period FROM budgets WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(BudgetRecord {
                account_id: row.try_get("account_id").map_err(be)?,
                limit_credits: row.try_get("limit_credits").map_err(be)?,
                period: row.try_get("period").map_err(be)?,
            })),
        }
    }
}
