//! The [`LedgerBackend`] implementation for [`PgLedger`].

use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, Credit, EntryId, OrgId};
use meter_ledger::{
    AccountScope, Balance, ChargeRequest, EntryType, GrantRequest, LeaseRequest, LedgerAccount,
    LedgerBackend, LedgerEntry, LedgerError, LimitClass, NewAccount, ReservationId, ReserveOutcome,
    ReserveRequest, SettleRequest, SYSTEM_ACCOUNT,
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
    .bind(entry.reservation_id.map(meter_ledger::ReservationId::as_uuid))
    .bind(entry.idempotency_key.as_deref())
    .bind(entry.created_at)
    .execute(conn)
    .await
    .map_err(be)?;
    Ok(())
}

/// Post a conserving double-entry transfer between two accounts within an open transaction. Both
/// accounts must already be locked/created by the caller.
async fn post_transfer(
    tx: &mut sqlx::PgConnection,
    org_id: Uuid,
    from: AccountId,
    to: AccountId,
    amount: Credit,
) -> Result<(), LedgerError> {
    let from_after: Decimal = sqlx::query_scalar(
        "UPDATE ledger_accounts SET settled_credits = settled_credits - $2 \
         WHERE id = $1 RETURNING settled_credits",
    )
    .bind(from.as_uuid())
    .bind(amount.value())
    .fetch_one(&mut *tx)
    .await
    .map_err(be)?;
    let to_after: Decimal = sqlx::query_scalar(
        "UPDATE ledger_accounts SET settled_credits = settled_credits + $2 \
         WHERE id = $1 RETURNING settled_credits",
    )
    .bind(to.as_uuid())
    .bind(amount.value())
    .fetch_one(&mut *tx)
    .await
    .map_err(be)?;

    let from_entry = LedgerEntry {
        id: EntryId::new(),
        account_id: from,
        paired_account_id: to,
        entry_type: EntryType::Transfer,
        delta_credits: -amount,
        balance_after: credit_from_db(from_after),
        source: None,
        revenue_recognizable: false,
        reverses_entry_id: None,
        reservation_id: None,
        idempotency_key: None,
        created_at: now_micros(),
    };
    insert_entry(tx, &from_entry, org_id).await?;
    let to_entry = LedgerEntry {
        id: EntryId::new(),
        account_id: to,
        paired_account_id: from,
        entry_type: EntryType::Transfer,
        delta_credits: amount,
        balance_after: credit_from_db(to_after),
        source: None,
        revenue_recognizable: false,
        reverses_entry_id: None,
        reservation_id: None,
        idempotency_key: None,
        created_at: now_micros(),
    };
    insert_entry(tx, &to_entry, org_id).await?;
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
            "INSERT INTO ledger_holds (reservation_id, org_id, account_id, amount, status, expires_at) \
             VALUES ($1, $2, $3, $4, 'open', $5)",
        )
        .bind(req.reservation_id.as_uuid())
        .bind(org_id)
        .bind(req.account.as_uuid())
        .bind(req.amount.value())
        .bind(req.expires_at)
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

    async fn charge(&self, req: ChargeRequest) -> Result<LedgerEntry, LedgerError> {
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
            "UPDATE ledger_accounts SET settled_credits = settled_credits - $2 \
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
            entry_type: EntryType::Usage,
            delta_credits: -req.amount,
            balance_after: credit_from_db(balance_after),
            source: None,
            revenue_recognizable: true,
            reverses_entry_id: None,
            reservation_id: None,
            idempotency_key: req.idempotency_key.clone(),
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
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

    async fn void_expired_holds(&self, now: OffsetDateTime) -> Result<u64, LedgerError> {
        let result = sqlx::query(
            "UPDATE ledger_holds SET status = 'voided' \
             WHERE status = 'open' AND expires_at IS NOT NULL AND expires_at <= $1",
        )
        .bind(now)
        .execute(self.pool())
        .await
        .map_err(be)?;
        Ok(result.rows_affected())
    }

    async fn open_lease(&self, req: LeaseRequest) -> Result<LedgerAccount, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.pool().begin().await.map_err(be)?;
        let parent = sqlx::query(
            "SELECT org_id, settled_credits, no_overdraft FROM ledger_accounts \
             WHERE id = $1 FOR UPDATE",
        )
        .bind(req.parent.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?
        .ok_or(LedgerError::AccountNotFound(req.parent))?;
        let org_id: Uuid = parent.try_get("org_id").map_err(be)?;
        let settled: Decimal = parent.try_get("settled_credits").map_err(be)?;
        let no_overdraft: bool = parent.try_get("no_overdraft").map_err(be)?;

        let held: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM ledger_holds \
             WHERE account_id = $1 AND status = 'open'",
        )
        .bind(req.parent.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;

        let available = settled - held;
        if no_overdraft && available < req.amount.value() {
            tx.commit().await.map_err(be)?;
            return Err(LedgerError::InsufficientFunds {
                available: credit_from_db(available),
                requested: req.amount,
            });
        }

        let child_id = AccountId::new();
        sqlx::query(
            "INSERT INTO ledger_accounts (id, org_id, scope, no_overdraft, parent_id) \
             VALUES ($1, $2, $3, true, $4)",
        )
        .bind(child_id.as_uuid())
        .bind(org_id)
        .bind(scope_to_str(AccountScope::Session))
        .bind(req.parent.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(be)?;

        post_transfer(&mut tx, org_id, req.parent, child_id, req.amount).await?;
        tx.commit().await.map_err(be)?;
        Ok(LedgerAccount {
            id: child_id,
            org_id: OrgId::from_uuid(org_id),
            scope: AccountScope::Session,
            no_overdraft: true,
            parent_id: Some(req.parent),
        })
    }

    async fn close_lease(&self, lease: AccountId) -> Result<Credit, LedgerError> {
        let mut tx = self.pool().begin().await.map_err(be)?;
        let row = sqlx::query(
            "SELECT org_id, settled_credits, parent_id FROM ledger_accounts \
             WHERE id = $1 FOR UPDATE",
        )
        .bind(lease.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?
        .ok_or(LedgerError::AccountNotFound(lease))?;
        let org_id: Uuid = row.try_get("org_id").map_err(be)?;
        let settled: Decimal = row.try_get("settled_credits").map_err(be)?;
        let parent_id: Option<Uuid> = row.try_get("parent_id").map_err(be)?;
        let parent = match parent_id {
            None => {
                tx.commit().await.map_err(be)?;
                return Err(LedgerError::NotALease(lease));
            }
            Some(parent) => parent,
        };

        let held: Decimal = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0) FROM ledger_holds \
             WHERE account_id = $1 AND status = 'open'",
        )
        .bind(lease.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;

        let available = settled - held;
        if available <= Decimal::ZERO {
            tx.commit().await.map_err(be)?;
            return Ok(Credit::ZERO);
        }
        let amount = credit_from_db(available);
        post_transfer(&mut tx, org_id, lease, AccountId::from_uuid(parent), amount).await?;
        tx.commit().await.map_err(be)?;
        Ok(amount)
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
