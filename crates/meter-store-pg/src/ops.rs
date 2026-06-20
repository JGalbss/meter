//! The [`LedgerBackend`] implementation for [`PgLedger`].

use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::Row;
use uuid::Uuid;

use meter_core::{AccountId, EntryId};
use meter_ledger::{
    Balance, EntryType, GrantRequest, LedgerAccount, LedgerBackend, LedgerEntry, LedgerError,
    LimitClass, NewAccount, ReservationId, ReserveOutcome, ReserveRequest, SettleRequest,
    SYSTEM_ACCOUNT,
};

use crate::mapping::{
    be, credit_from_db, entry_from_row, entry_type_to_str, now_micros, scope_to_str, source_to_str,
};
use crate::PgLedger;

async fn insert_entry(
    conn: &mut sqlx::PgConnection,
    entry: &LedgerEntry,
    org_id: Uuid,
) -> Result<(), LedgerError> {
    sqlx::query(
        "INSERT INTO ledger_entries \
         (id, org_id, account_id, paired_account_id, entry_type, delta_credits, balance_after, \
          source, revenue_recognizable, reverses_entry_id, reservation_id, idempotency_key, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(entry.id.as_uuid())
    .bind(org_id)
    .bind(entry.account_id.as_uuid())
    .bind(entry.paired_account_id.as_uuid())
    .bind(entry_type_to_str(entry.entry_type))
    .bind(entry.delta_credits.value())
    .bind(entry.balance_after.value())
    .bind(entry.source.map(source_to_str))
    .bind(entry.revenue_recognizable)
    .bind(entry.reverses_entry_id.map(|id| id.as_uuid()))
    .bind(entry.reservation_id.map(|id| id.as_uuid()))
    .bind(entry.idempotency_key.as_deref())
    .bind(entry.created_at)
    .execute(conn)
    .await
    .map_err(be)?;
    Ok(())
}

#[async_trait]
impl LedgerBackend for PgLedger {
    async fn open_account(&self, req: NewAccount) -> Result<LedgerAccount, LedgerError> {
        let id = AccountId::new();
        sqlx::query(
            "INSERT INTO ledger_accounts (id, org_id, scope, no_overdraft, parent_id) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.as_uuid())
        .bind(req.org_id.as_uuid())
        .bind(scope_to_str(req.scope))
        .bind(req.no_overdraft)
        .bind(req.parent_id.map(|parent| parent.as_uuid()))
        .execute(self.pool())
        .await
        .map_err(be)?;
        Ok(LedgerAccount {
            id,
            org_id: req.org_id,
            scope: req.scope,
            no_overdraft: req.no_overdraft,
            parent_id: req.parent_id,
        })
    }

    async fn balance(&self, account: AccountId) -> Result<Balance, LedgerError> {
        let row = sqlx::query("SELECT settled_credits FROM ledger_accounts WHERE id = $1")
            .bind(account.as_uuid())
            .fetch_optional(self.pool())
            .await
            .map_err(be)?
            .ok_or(LedgerError::AccountNotFound(account))?;
        let settled = credit_from_db(row.try_get::<Decimal, _>("settled_credits").map_err(be)?);
        let held: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM ledger_holds \
             WHERE account_id = $1 AND status = 'open'",
        )
        .bind(account.as_uuid())
        .fetch_one(self.pool())
        .await
        .map_err(be)?;
        Ok(Balance {
            settled,
            held: credit_from_db(held),
        })
    }

    async fn grant(&self, req: GrantRequest) -> Result<LedgerEntry, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.pool().begin().await.map_err(be)?;
        let account_row =
            sqlx::query("SELECT org_id FROM ledger_accounts WHERE id = $1 FOR UPDATE")
                .bind(req.account.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(be)?
                .ok_or(LedgerError::AccountNotFound(req.account))?;
        let org_id: Uuid = account_row.try_get("org_id").map_err(be)?;

        if let Some(key) = req.idempotency_key.as_deref() {
            let existing = sqlx::query(
                "SELECT * FROM ledger_entries WHERE account_id = $1 AND idempotency_key = $2",
            )
            .bind(req.account.as_uuid())
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(be)?;
            if let Some(row) = existing {
                let entry = entry_from_row(&row)?;
                tx.commit().await.map_err(be)?;
                return Ok(entry);
            }
        }

        let balance_after: Decimal = sqlx::query_scalar(
            "UPDATE ledger_accounts SET settled_credits = settled_credits + $2 \
             WHERE id = $1 RETURNING settled_credits",
        )
        .bind(req.account.as_uuid())
        .bind(req.amount.value())
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;

        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: req.account,
            paired_account_id: SYSTEM_ACCOUNT,
            entry_type: EntryType::Grant,
            delta_credits: req.amount,
            balance_after: credit_from_db(balance_after),
            source: Some(req.source),
            revenue_recognizable: false,
            reverses_entry_id: None,
            reservation_id: None,
            idempotency_key: req.idempotency_key.clone(),
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
        tx.commit().await.map_err(be)?;
        Ok(entry)
    }

    async fn reserve(&self, req: ReserveRequest) -> Result<ReserveOutcome, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.pool().begin().await.map_err(be)?;
        let account_row = sqlx::query(
            "SELECT org_id, settled_credits, no_overdraft FROM ledger_accounts \
             WHERE id = $1 FOR UPDATE",
        )
        .bind(req.account.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?
        .ok_or(LedgerError::AccountNotFound(req.account))?;
        let org_id: Uuid = account_row.try_get("org_id").map_err(be)?;
        let settled: Decimal = account_row.try_get("settled_credits").map_err(be)?;
        let no_overdraft: bool = account_row.try_get("no_overdraft").map_err(be)?;

        let existing: Option<String> =
            sqlx::query_scalar("SELECT status FROM ledger_holds WHERE reservation_id = $1")
                .bind(req.reservation_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(be)?;
        if let Some(status) = existing {
            tx.commit().await.map_err(be)?;
            if status == "open" {
                return Ok(ReserveOutcome::Allowed {
                    reservation: req.reservation_id,
                });
            }
            return Err(LedgerError::ReservationClosed(req.reservation_id));
        }

        let held: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM ledger_holds \
             WHERE account_id = $1 AND status = 'open'",
        )
        .bind(req.account.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;

        let available = settled - held;
        let hard = matches!(req.limit, LimitClass::Hard) || no_overdraft;
        if hard && available < req.amount.value() {
            tx.commit().await.map_err(be)?;
            return Ok(ReserveOutcome::Denied {
                available: credit_from_db(available),
                requested: req.amount,
            });
        }

        sqlx::query(
            "INSERT INTO ledger_holds (reservation_id, org_id, account_id, amount, status) \
             VALUES ($1, $2, $3, $4, 'open')",
        )
        .bind(req.reservation_id.as_uuid())
        .bind(org_id)
        .bind(req.account.as_uuid())
        .bind(req.amount.value())
        .execute(&mut *tx)
        .await
        .map_err(be)?;
        tx.commit().await.map_err(be)?;
        Ok(ReserveOutcome::Allowed {
            reservation: req.reservation_id,
        })
    }

    async fn settle(&self, req: SettleRequest) -> Result<LedgerEntry, LedgerError> {
        if req.actual.is_negative() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.pool().begin().await.map_err(be)?;
        let hold = sqlx::query(
            "SELECT account_id, status, settle_entry_id FROM ledger_holds \
             WHERE reservation_id = $1 FOR UPDATE",
        )
        .bind(req.reservation_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?
        .ok_or(LedgerError::ReservationNotFound(req.reservation_id))?;
        let status: String = hold.try_get("status").map_err(be)?;
        let account_id: Uuid = hold.try_get("account_id").map_err(be)?;

        if status == "settled" {
            let settle_entry_id: Uuid = hold.try_get("settle_entry_id").map_err(be)?;
            let row = sqlx::query("SELECT * FROM ledger_entries WHERE id = $1")
                .bind(settle_entry_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(be)?;
            let entry = entry_from_row(&row)?;
            tx.commit().await.map_err(be)?;
            return Ok(entry);
        }
        if status == "voided" {
            tx.commit().await.map_err(be)?;
            return Err(LedgerError::ReservationClosed(req.reservation_id));
        }

        let updated = sqlx::query(
            "UPDATE ledger_accounts SET settled_credits = settled_credits - $2 \
             WHERE id = $1 RETURNING settled_credits, org_id",
        )
        .bind(account_id)
        .bind(req.actual.value())
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;
        let balance_after: Decimal = updated.try_get("settled_credits").map_err(be)?;
        let org_id: Uuid = updated.try_get("org_id").map_err(be)?;

        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: AccountId::from_uuid(account_id),
            paired_account_id: SYSTEM_ACCOUNT,
            entry_type: EntryType::Settle,
            delta_credits: -req.actual,
            balance_after: credit_from_db(balance_after),
            source: None,
            revenue_recognizable: true,
            reverses_entry_id: None,
            reservation_id: Some(req.reservation_id),
            idempotency_key: None,
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
        sqlx::query(
            "UPDATE ledger_holds SET status = 'settled', settle_entry_id = $2 \
             WHERE reservation_id = $1",
        )
        .bind(req.reservation_id.as_uuid())
        .bind(entry.id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(be)?;
        tx.commit().await.map_err(be)?;
        Ok(entry)
    }

    async fn void(&self, reservation: ReservationId) -> Result<(), LedgerError> {
        let mut tx = self.pool().begin().await.map_err(be)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM ledger_holds WHERE reservation_id = $1 FOR UPDATE",
        )
        .bind(reservation.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?;
        let result = match status.as_deref() {
            None | Some("voided") => Ok(()),
            Some("open") => {
                sqlx::query("UPDATE ledger_holds SET status = 'voided' WHERE reservation_id = $1")
                    .bind(reservation.as_uuid())
                    .execute(&mut *tx)
                    .await
                    .map_err(be)?;
                Ok(())
            }
            Some(_) => Err(LedgerError::ReservationClosed(reservation)),
        };
        tx.commit().await.map_err(be)?;
        result
    }

    async fn entries(&self, account: AccountId) -> Result<Vec<LedgerEntry>, LedgerError> {
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM ledger_accounts WHERE id = $1")
                .bind(account.as_uuid())
                .fetch_optional(self.pool())
                .await
                .map_err(be)?;
        if exists.is_none() {
            return Err(LedgerError::AccountNotFound(account));
        }
        let rows = sqlx::query(
            "SELECT * FROM ledger_entries WHERE account_id = $1 OR paired_account_id = $1 \
             ORDER BY created_at, id",
        )
        .bind(account.as_uuid())
        .fetch_all(self.pool())
        .await
        .map_err(be)?;
        rows.iter().map(entry_from_row).collect()
    }
}
