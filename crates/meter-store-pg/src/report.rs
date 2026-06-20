//! Reporting queries over the ledger (read-only).

use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;
use time::OffsetDateTime;

use meter_core::{AccountId, Credit};
use meter_ledger::LedgerError;

use crate::mapping::{be, credit_from_db};
use crate::PgLedger;

/// Usage summed from the ledger over a period — the basis for an invoice. It sums every spend posting
/// (settle from reserve/settle, and usage from direct charges), so "enforced == billed" holds by
/// construction however usage was recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeriodUsage {
    pub total_credits: Credit,
    pub entry_count: i64,
}

impl PgLedger {
    /// Total credits consumed (sum of `settle` + `usage` postings) by an account in `[start, end)`.
    pub async fn period_usage(
        &self,
        account: AccountId,
        start: OffsetDateTime,
        end: OffsetDateTime,
    ) -> Result<PeriodUsage, LedgerError> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(-delta_credits), 0) AS total, COUNT(*) AS entry_count \
             FROM ledger_entries \
             WHERE account_id = $1 AND entry_type IN ('settle', 'usage') \
               AND created_at >= $2 AND created_at < $3",
        )
        .bind(account.as_uuid())
        .bind(start)
        .bind(end)
        .fetch_one(self.pool())
        .await
        .map_err(be)?;
        let total: Decimal = row.try_get("total").map_err(be)?;
        let entry_count: i64 = row.try_get("entry_count").map_err(be)?;
        Ok(PeriodUsage {
            total_credits: credit_from_db(total),
            entry_count,
        })
    }
}
