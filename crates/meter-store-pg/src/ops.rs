//! The [`LedgerBackend`] implementation for [`PgLedger`].

use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, Credit, EntryId, OrgId, RunId};
use meter_ledger::{
    run_void_charge_refund_key, run_void_refund_key, AccountScope, Balance, ChargeRequest,
    EntryType, GrantRequest, LeaseRequest, LedgerAccount, LedgerBackend, LedgerEntry, LedgerError,
    LimitClass, NewAccount, RefundRequest, ReservationId, ReserveOutcome, ReserveRequest,
    ReverseChargeRequest, RunVoidSummary, SettleRequest, SYSTEM_ACCOUNT,
};

use crate::mapping::{
    be, credit_from_db, entry_from_row, entry_type_to_str, now_micros, scope_to_str, source_to_str,
};
use crate::PgLedger;

/// Statement timeout applied to every hot-path money transaction (milliseconds). A hung query holding
/// account-row locks under `FOR UPDATE` must never stall the ledger indefinitely — the OpenAI-Postgres
/// discipline for hot tables. Conservative enough that legitimate reserve/settle never trips it.
const HOT_PATH_STATEMENT_TIMEOUT_MS: u32 = 5_000;

impl PgLedger {
    /// Begin a money-path transaction with [`HOT_PATH_STATEMENT_TIMEOUT_MS`] already applied via
    /// `SET LOCAL`, so no single statement can hold locks past the timeout.
    async fn begin_hot(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, LedgerError> {
        let mut tx = self.pool().begin().await.map_err(be)?;
        sqlx::query(&format!(
            "SET LOCAL statement_timeout = {HOT_PATH_STATEMENT_TIMEOUT_MS}"
        ))
        .execute(&mut *tx)
        .await
        .map_err(be)?;
        Ok(tx)
    }
}

async fn insert_entry(
    conn: &mut sqlx::PgConnection,
    entry: &LedgerEntry,
    org_id: Uuid,
) -> Result<(), LedgerError> {
    sqlx::query(
        "INSERT INTO ledger_entries \
         (id, org_id, account_id, paired_account_id, entry_type, delta_credits, balance_after, \
          source, revenue_recognizable, reverses_entry_id, reservation_id, run_id, idempotency_key, \
          created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
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
    .bind(entry.run_id.map(|run| run.as_uuid()))
    .bind(entry.idempotency_key.as_deref())
    .bind(entry.created_at)
    .execute(conn)
    .await
    .map_err(be)?;
    Ok(())
}

/// The still-unreversed magnitude of a charge/settle entry: its original spend (`-delta_credits`,
/// positive) minus every refund that **references it** via `reverses_entry_id`. Zero or negative means
/// it is fully reversed, so `void_run` posts nothing — making a re-void idempotent and never
/// double-refunding a charge already corrected by a *linked* credit-note or (later) an amendment delta.
/// An *unlinked* credit-note (`reverses_entry_id` = NULL — a free-standing goodwill credit) is an
/// independent posting and intentionally does not offset the charge: to cancel a specific charge, the
/// credit-note must reference it.
async fn unreversed_remainder(
    conn: &mut sqlx::PgConnection,
    entry_id: Uuid,
) -> Result<Decimal, LedgerError> {
    let charged: Decimal =
        sqlx::query_scalar("SELECT delta_credits FROM ledger_entries WHERE id = $1")
            .bind(entry_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(be)?;
    let reversed: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta_credits), 0) FROM ledger_entries WHERE reverses_entry_id = $1",
    )
    .bind(entry_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(be)?;
    // `charged` is negative (a spend); `-charged` is its positive magnitude; `reversed` sums the
    // positive refunds already applied to it.
    Ok(-charged - reversed)
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
        run_id: None,
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
        run_id: None,
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
        let mut tx = self.begin_hot().await?;
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
            run_id: None,
            idempotency_key: req.idempotency_key.clone(),
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
        tx.commit().await.map_err(be)?;
        Ok(entry)
    }

    async fn refund(&self, req: RefundRequest) -> Result<LedgerEntry, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.begin_hot().await?;
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
            entry_type: EntryType::Refund,
            delta_credits: req.amount,
            balance_after: credit_from_db(balance_after),
            source: None,
            revenue_recognizable: false,
            reverses_entry_id: req.reverses_entry_id,
            reservation_id: None,
            run_id: None,
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
        let mut tx = self.begin_hot().await?;
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
            "INSERT INTO ledger_holds \
             (reservation_id, org_id, account_id, amount, status, expires_at, run_id) \
             VALUES ($1, $2, $3, $4, 'open', $5, $6)",
        )
        .bind(req.reservation_id.as_uuid())
        .bind(org_id)
        .bind(req.account.as_uuid())
        .bind(req.amount.value())
        .bind(req.expires_at)
        .bind(req.run_id.map(|run| run.as_uuid()))
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
        let mut tx = self.begin_hot().await?;
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
            run_id: None,
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
        let mut tx = self.begin_hot().await?;
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
            run_id: req.run_id,
            idempotency_key: req.idempotency_key.clone(),
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
        tx.commit().await.map_err(be)?;
        Ok(entry)
    }

    async fn reverse_charge(
        &self,
        req: ReverseChargeRequest,
    ) -> Result<Option<LedgerEntry>, LedgerError> {
        let mut tx = self.begin_hot().await?;
        let account_row =
            sqlx::query("SELECT org_id FROM ledger_accounts WHERE id = $1 FOR UPDATE")
                .bind(req.account.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(be)?
                .ok_or(LedgerError::AccountNotFound(req.account))?;
        let org_id: Uuid = account_row.try_get("org_id").map_err(be)?;

        // Idempotent: a refund already posted under this key means this reversal happened.
        let existing = sqlx::query(
            "SELECT * FROM ledger_entries WHERE account_id = $1 AND idempotency_key = $2",
        )
        .bind(req.account.as_uuid())
        .bind(&req.idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?;
        if let Some(row) = existing {
            let entry = entry_from_row(&row)?;
            tx.commit().await.map_err(be)?;
            return Ok(Some(entry));
        }

        // Lock the charge so a concurrent reversal (another amend, or void_run) serializes with us, and
        // refund only its unreversed remainder.
        sqlx::query("SELECT id FROM ledger_entries WHERE id = $1 FOR UPDATE")
            .bind(req.charge_entry_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(be)?;
        let amount = unreversed_remainder(&mut tx, req.charge_entry_id.as_uuid()).await?;
        if amount <= Decimal::ZERO {
            tx.commit().await.map_err(be)?;
            return Ok(None);
        }
        let updated = sqlx::query(
            "UPDATE ledger_accounts SET settled_credits = settled_credits + $2 \
             WHERE id = $1 RETURNING settled_credits",
        )
        .bind(req.account.as_uuid())
        .bind(amount)
        .fetch_one(&mut *tx)
        .await
        .map_err(be)?;
        let balance_after: Decimal = updated.try_get("settled_credits").map_err(be)?;
        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: req.account,
            paired_account_id: SYSTEM_ACCOUNT,
            entry_type: EntryType::Refund,
            delta_credits: credit_from_db(amount),
            balance_after: credit_from_db(balance_after),
            source: None,
            revenue_recognizable: false,
            reverses_entry_id: Some(req.charge_entry_id),
            reservation_id: None,
            run_id: req.run_id,
            idempotency_key: Some(req.idempotency_key),
            created_at: now_micros(),
        };
        insert_entry(&mut tx, &entry, org_id).await?;
        tx.commit().await.map_err(be)?;
        Ok(Some(entry))
    }

    async fn void(&self, reservation: ReservationId) -> Result<(), LedgerError> {
        let mut tx = self.begin_hot().await?;
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

    async fn extend_hold(
        &self,
        reservation: ReservationId,
        expires_at: OffsetDateTime,
    ) -> Result<(), LedgerError> {
        let mut tx = self.begin_hot().await?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM ledger_holds WHERE reservation_id = $1 FOR UPDATE",
        )
        .bind(reservation.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?;
        let result = match status.as_deref() {
            None => Err(LedgerError::ReservationNotFound(reservation)),
            Some("open") => {
                sqlx::query("UPDATE ledger_holds SET expires_at = $2 WHERE reservation_id = $1")
                    .bind(reservation.as_uuid())
                    .bind(expires_at)
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

    async fn void_run(&self, run: RunId) -> Result<RunVoidSummary, LedgerError> {
        let mut tx = self.begin_hot().await?;
        // Lock the run's holds for the duration of the reversal.
        let rows = sqlx::query(
            "SELECT reservation_id, account_id, status, settle_entry_id \
             FROM ledger_holds WHERE run_id = $1 FOR UPDATE",
        )
        .bind(run.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(be)?;

        let mut summary = RunVoidSummary::default();
        for row in rows {
            let reservation: Uuid = row.try_get("reservation_id").map_err(be)?;
            let account_id: Uuid = row.try_get("account_id").map_err(be)?;
            let status: String = row.try_get("status").map_err(be)?;
            match status.as_str() {
                "voided" => {}
                "open" => {
                    sqlx::query(
                        "UPDATE ledger_holds SET status = 'voided' WHERE reservation_id = $1",
                    )
                    .bind(reservation)
                    .execute(&mut *tx)
                    .await
                    .map_err(be)?;
                    summary.holds_released += 1;
                }
                "settled" => {
                    let settle_entry_id: Uuid = row.try_get("settle_entry_id").map_err(be)?;
                    let key = run_void_refund_key(run, ReservationId::from_uuid(reservation));
                    // Reverse only the settle's *unreversed remainder*: its original magnitude minus any
                    // refunds that already reverse it (a prior void_run, kept idempotent, OR a manual
                    // credit-note). This never double-refunds a charge corrected out-of-band, and a
                    // re-void sees its own refund and refunds zero. `FOR UPDATE` on the holds serializes
                    // concurrent voids, so the remainder read is consistent.
                    let amount = unreversed_remainder(&mut tx, settle_entry_id).await?;
                    if amount <= Decimal::ZERO {
                        continue;
                    }
                    let updated = sqlx::query(
                        "UPDATE ledger_accounts SET settled_credits = settled_credits + $2 \
                         WHERE id = $1 RETURNING settled_credits, org_id",
                    )
                    .bind(account_id)
                    .bind(amount)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(be)?;
                    let balance_after: Decimal = updated.try_get("settled_credits").map_err(be)?;
                    let org_id: Uuid = updated.try_get("org_id").map_err(be)?;
                    let entry = LedgerEntry {
                        id: EntryId::new(),
                        account_id: AccountId::from_uuid(account_id),
                        paired_account_id: SYSTEM_ACCOUNT,
                        entry_type: EntryType::Refund,
                        delta_credits: credit_from_db(amount),
                        balance_after: credit_from_db(balance_after),
                        source: None,
                        revenue_recognizable: false,
                        reverses_entry_id: Some(EntryId::from_uuid(settle_entry_id)),
                        reservation_id: Some(ReservationId::from_uuid(reservation)),
                        run_id: Some(run),
                        idempotency_key: Some(key),
                        created_at: now_micros(),
                    };
                    insert_entry(&mut tx, &entry, org_id).await?;
                    summary.charges_refunded += 1;
                    summary.credits_refunded += entry.delta_credits;
                }
                _ => {}
            }
        }

        // Reverse the run's *direct* charges (post-hoc /v1/usage metering tagged with the run, not tied
        // to a reservation). Lock them for the reversal; we refund only each charge's unreversed
        // remainder, so a re-void or a prior manual credit-note never double-refunds.
        let charge_rows = sqlx::query(
            "SELECT id, account_id FROM ledger_entries \
             WHERE run_id = $1 AND entry_type = 'usage' \
             ORDER BY created_at, id FOR UPDATE",
        )
        .bind(run.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(be)?;
        for row in charge_rows {
            let charge_id: Uuid = row.try_get("id").map_err(be)?;
            let account_id: Uuid = row.try_get("account_id").map_err(be)?;
            let key = run_void_charge_refund_key(run, EntryId::from_uuid(charge_id));
            let amount = unreversed_remainder(&mut tx, charge_id).await?;
            if amount <= Decimal::ZERO {
                continue;
            }
            let updated = sqlx::query(
                "UPDATE ledger_accounts SET settled_credits = settled_credits + $2 \
                 WHERE id = $1 RETURNING settled_credits, org_id",
            )
            .bind(account_id)
            .bind(amount)
            .fetch_one(&mut *tx)
            .await
            .map_err(be)?;
            let balance_after: Decimal = updated.try_get("settled_credits").map_err(be)?;
            let org_id: Uuid = updated.try_get("org_id").map_err(be)?;
            let entry = LedgerEntry {
                id: EntryId::new(),
                account_id: AccountId::from_uuid(account_id),
                paired_account_id: SYSTEM_ACCOUNT,
                entry_type: EntryType::Refund,
                delta_credits: credit_from_db(amount),
                balance_after: credit_from_db(balance_after),
                source: None,
                revenue_recognizable: false,
                reverses_entry_id: Some(EntryId::from_uuid(charge_id)),
                reservation_id: None,
                run_id: Some(run),
                idempotency_key: Some(key),
                created_at: now_micros(),
            };
            insert_entry(&mut tx, &entry, org_id).await?;
            summary.charges_refunded += 1;
            summary.credits_refunded += entry.delta_credits;
        }
        tx.commit().await.map_err(be)?;
        Ok(summary)
    }

    async fn open_lease(&self, req: LeaseRequest) -> Result<LedgerAccount, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut tx = self.begin_hot().await?;
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
        let mut tx = self.begin_hot().await?;
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
