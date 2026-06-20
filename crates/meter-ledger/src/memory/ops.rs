//! The [`LedgerBackend`] implementation for [`InMemoryLedger`].

use async_trait::async_trait;
use meter_core::{AccountId, EntryId};
use time::OffsetDateTime;

use crate::backend::LedgerBackend;
use crate::error::LedgerError;
use crate::model::{Balance, EntryType, LedgerAccount, LedgerEntry, LimitClass, ReservationId};
use crate::request::{GrantRequest, NewAccount, ReserveOutcome, ReserveRequest, SettleRequest};

use super::state::{AccountRow, Hold, HoldStatus};
use super::InMemoryLedger;

#[async_trait]
impl LedgerBackend for InMemoryLedger {
    async fn open_account(&self, req: NewAccount) -> Result<LedgerAccount, LedgerError> {
        let mut state = self.lock();
        let id = AccountId::new();
        let account = LedgerAccount {
            id,
            scope: req.scope,
            no_overdraft: req.no_overdraft,
            parent_id: req.parent_id,
        };
        state.accounts.insert(id, AccountRow::new(account.clone()));
        Ok(account)
    }

    async fn balance(&self, account: AccountId) -> Result<Balance, LedgerError> {
        let state = self.lock();
        let settled = state
            .accounts
            .get(&account)
            .ok_or(LedgerError::AccountNotFound(account))?
            .settled;
        let held = state.held(account);
        Ok(Balance { settled, held })
    }

    async fn grant(&self, req: GrantRequest) -> Result<LedgerEntry, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        if !state.accounts.contains_key(&req.account) {
            return Err(LedgerError::AccountNotFound(req.account));
        }
        if let Some(key) = req.idempotency_key.as_deref() {
            if let Some(existing) = state
                .entries
                .iter()
                .find(|entry| entry.idempotency_key.as_deref() == Some(key))
            {
                return Ok(existing.clone());
            }
        }
        if let Some(system) = state.accounts.get_mut(&self.system) {
            system.settled -= req.amount;
        }
        let balance_after = {
            let account = state
                .accounts
                .get_mut(&req.account)
                .expect("existence checked above");
            account.settled += req.amount;
            account.settled
        };
        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: req.account,
            paired_account_id: self.system,
            entry_type: EntryType::Grant,
            delta_credits: req.amount,
            balance_after,
            source: Some(req.source),
            revenue_recognizable: false,
            reverses_entry_id: None,
            reservation_id: None,
            idempotency_key: req.idempotency_key,
            created_at: OffsetDateTime::now_utc(),
        };
        state.entries.push(entry.clone());
        Ok(entry)
    }

    async fn reserve(&self, req: ReserveRequest) -> Result<ReserveOutcome, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        let no_overdraft = match state.accounts.get(&req.account) {
            None => return Err(LedgerError::AccountNotFound(req.account)),
            Some(row) => row.account.no_overdraft,
        };
        if let Some(hold) = state.holds.get(&req.reservation_id) {
            if hold.status == HoldStatus::Open {
                return Ok(ReserveOutcome::Allowed {
                    reservation: req.reservation_id,
                });
            }
            return Err(LedgerError::ReservationClosed(req.reservation_id));
        }
        let available = {
            let settled = state
                .accounts
                .get(&req.account)
                .expect("existence checked above")
                .settled;
            settled - state.held(req.account)
        };
        let hard = matches!(req.limit, LimitClass::Hard) || no_overdraft;
        if hard && available < req.amount {
            return Ok(ReserveOutcome::Denied {
                available,
                requested: req.amount,
            });
        }
        state.holds.insert(
            req.reservation_id,
            Hold {
                account: req.account,
                amount: req.amount,
                status: HoldStatus::Open,
                settle_entry: None,
            },
        );
        Ok(ReserveOutcome::Allowed {
            reservation: req.reservation_id,
        })
    }

    async fn settle(&self, req: SettleRequest) -> Result<LedgerEntry, LedgerError> {
        if req.actual.is_negative() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        let (status, account, settle_entry) = match state.holds.get(&req.reservation_id) {
            None => return Err(LedgerError::ReservationNotFound(req.reservation_id)),
            Some(hold) => (hold.status, hold.account, hold.settle_entry),
        };
        match status {
            HoldStatus::Settled => {
                let id = settle_entry.expect("a settled hold records its entry id");
                let entry = state
                    .entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .expect("settle entry exists")
                    .clone();
                return Ok(entry);
            }
            HoldStatus::Voided => return Err(LedgerError::ReservationClosed(req.reservation_id)),
            HoldStatus::Open => {}
        }
        let balance_after = {
            let account = state
                .accounts
                .get_mut(&account)
                .expect("the hold's account exists");
            account.settled -= req.actual;
            account.settled
        };
        if let Some(system) = state.accounts.get_mut(&self.system) {
            system.settled += req.actual;
        }
        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: account,
            paired_account_id: self.system,
            entry_type: EntryType::Settle,
            delta_credits: -req.actual,
            balance_after,
            source: None,
            revenue_recognizable: true,
            reverses_entry_id: None,
            reservation_id: Some(req.reservation_id),
            idempotency_key: None,
            created_at: OffsetDateTime::now_utc(),
        };
        state.entries.push(entry.clone());
        if let Some(hold) = state.holds.get_mut(&req.reservation_id) {
            hold.status = HoldStatus::Settled;
            hold.settle_entry = Some(entry.id);
        }
        Ok(entry)
    }

    async fn void(&self, reservation: ReservationId) -> Result<(), LedgerError> {
        let mut state = self.lock();
        match state.holds.get_mut(&reservation) {
            None => Ok(()),
            Some(hold) => match hold.status {
                HoldStatus::Open => {
                    hold.status = HoldStatus::Voided;
                    Ok(())
                }
                HoldStatus::Voided => Ok(()),
                HoldStatus::Settled => Err(LedgerError::ReservationClosed(reservation)),
            },
        }
    }

    async fn entries(&self, account: AccountId) -> Result<Vec<LedgerEntry>, LedgerError> {
        let state = self.lock();
        if !state.accounts.contains_key(&account) {
            return Err(LedgerError::AccountNotFound(account));
        }
        Ok(state
            .entries
            .iter()
            .filter(|entry| entry.account_id == account || entry.paired_account_id == account)
            .cloned()
            .collect())
    }
}
