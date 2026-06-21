//! The [`LedgerBackend`] implementation for [`InMemoryLedger`].

use async_trait::async_trait;
use meter_core::{AccountId, Credit, EntryId, RunId};
use time::OffsetDateTime;

use crate::backend::{run_void_charge_refund_key, run_void_refund_key, LedgerBackend};
use crate::error::LedgerError;
use crate::model::{
    AccountScope, Balance, EntryType, LedgerAccount, LedgerEntry, LimitClass, ReservationId,
};
use crate::request::{
    ChargeRequest, GrantRequest, LeaseRequest, NewAccount, RefundRequest, ReserveOutcome,
    ReserveRequest, ReverseChargeRequest, RunVoidSummary, SettleRequest,
};

use super::state::{AccountRow, Hold, HoldStatus, State};
use super::InMemoryLedger;

/// The still-unreversed magnitude of a charge/settle entry: its original spend minus every refund that
/// **references it** via `reverses_entry_id`. Zero or negative means fully reversed, so `void_run` posts
/// nothing — keeping a re-void idempotent and never double-refunding a charge corrected by a *linked*
/// credit-note. An unlinked credit-note is an independent posting and does not offset the charge.
/// Mirrors the Postgres `unreversed_remainder`.
fn unreversed_remainder(state: &State, entry_id: EntryId) -> Credit {
    let charged = state
        .entries
        .iter()
        .find(|entry| entry.id == entry_id)
        .map_or(Credit::ZERO, |entry| entry.delta_credits);
    let reversed = state
        .entries
        .iter()
        .filter(|entry| entry.reverses_entry_id == Some(entry_id))
        .fold(Credit::ZERO, |acc, entry| acc + entry.delta_credits);
    -charged - reversed
}

/// Post a conserving double-entry transfer between two existing accounts (both must be present).
fn post_transfer(state: &mut State, from: AccountId, to: AccountId, amount: Credit) {
    let from_after = {
        let row = state
            .accounts
            .get_mut(&from)
            .expect("transfer source exists");
        row.settled -= amount;
        row.settled
    };
    let to_after = {
        let row = state
            .accounts
            .get_mut(&to)
            .expect("transfer destination exists");
        row.settled += amount;
        row.settled
    };
    let now = OffsetDateTime::now_utc();
    state.entries.push(LedgerEntry {
        id: EntryId::new(),
        account_id: from,
        paired_account_id: to,
        entry_type: EntryType::Transfer,
        delta_credits: -amount,
        balance_after: from_after,
        source: None,
        revenue_recognizable: false,
        reverses_entry_id: None,
        reservation_id: None,
        run_id: None,
        idempotency_key: None,
        created_at: now,
    });
    state.entries.push(LedgerEntry {
        id: EntryId::new(),
        account_id: to,
        paired_account_id: from,
        entry_type: EntryType::Transfer,
        delta_credits: amount,
        balance_after: to_after,
        source: None,
        revenue_recognizable: false,
        reverses_entry_id: None,
        reservation_id: None,
        run_id: None,
        idempotency_key: None,
        created_at: now,
    });
}

#[async_trait]
impl LedgerBackend for InMemoryLedger {
    async fn open_account(&self, req: NewAccount) -> Result<LedgerAccount, LedgerError> {
        let mut state = self.lock();
        let id = AccountId::new();
        let account = LedgerAccount {
            id,
            org_id: req.org_id,
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
            if let Some(existing) = state.entries.iter().find(|entry| {
                entry.account_id == req.account && entry.idempotency_key.as_deref() == Some(key)
            }) {
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
            run_id: None,
            idempotency_key: req.idempotency_key,
            created_at: OffsetDateTime::now_utc(),
        };
        state.entries.push(entry.clone());
        Ok(entry)
    }

    async fn refund(&self, req: RefundRequest) -> Result<LedgerEntry, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        if !state.accounts.contains_key(&req.account) {
            return Err(LedgerError::AccountNotFound(req.account));
        }
        if let Some(key) = req.idempotency_key.as_deref() {
            if let Some(existing) = state.entries.iter().find(|entry| {
                entry.account_id == req.account && entry.idempotency_key.as_deref() == Some(key)
            }) {
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
            entry_type: EntryType::Refund,
            delta_credits: req.amount,
            balance_after,
            source: None,
            revenue_recognizable: false,
            reverses_entry_id: req.reverses_entry_id,
            reservation_id: None,
            run_id: None,
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
                expires_at: req.expires_at,
                run_id: req.run_id,
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
            run_id: None,
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

    async fn charge(&self, req: ChargeRequest) -> Result<LedgerEntry, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        if !state.accounts.contains_key(&req.account) {
            return Err(LedgerError::AccountNotFound(req.account));
        }
        if let Some(key) = req.idempotency_key.as_deref() {
            if let Some(existing) = state.entries.iter().find(|entry| {
                entry.account_id == req.account && entry.idempotency_key.as_deref() == Some(key)
            }) {
                return Ok(existing.clone());
            }
        }
        let balance_after = {
            let account = state
                .accounts
                .get_mut(&req.account)
                .expect("existence checked above");
            account.settled -= req.amount;
            account.settled
        };
        if let Some(system) = state.accounts.get_mut(&self.system) {
            system.settled += req.amount;
        }
        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: req.account,
            paired_account_id: self.system,
            entry_type: EntryType::Usage,
            delta_credits: -req.amount,
            balance_after,
            source: None,
            revenue_recognizable: true,
            reverses_entry_id: None,
            reservation_id: None,
            run_id: req.run_id,
            idempotency_key: req.idempotency_key,
            created_at: OffsetDateTime::now_utc(),
        };
        state.entries.push(entry.clone());
        Ok(entry)
    }

    async fn reverse_charge(
        &self,
        req: ReverseChargeRequest,
    ) -> Result<Option<LedgerEntry>, LedgerError> {
        let mut state = self.lock();
        if !state.accounts.contains_key(&req.account) {
            return Err(LedgerError::AccountNotFound(req.account));
        }
        // Idempotent: a refund already posted under this key means this reversal happened.
        if let Some(existing) = state
            .entries
            .iter()
            .find(|entry| entry.idempotency_key.as_deref() == Some(req.idempotency_key.as_str()))
        {
            return Ok(Some(existing.clone()));
        }
        let amount = unreversed_remainder(&state, req.charge_entry_id);
        if !amount.is_positive() {
            return Ok(None);
        }
        if let Some(system) = state.accounts.get_mut(&self.system) {
            system.settled -= amount;
        }
        let balance_after = {
            let row = state
                .accounts
                .get_mut(&req.account)
                .expect("existence checked above");
            row.settled += amount;
            row.settled
        };
        let entry = LedgerEntry {
            id: EntryId::new(),
            account_id: req.account,
            paired_account_id: self.system,
            entry_type: EntryType::Refund,
            delta_credits: amount,
            balance_after,
            source: None,
            revenue_recognizable: false,
            reverses_entry_id: Some(req.charge_entry_id),
            reservation_id: None,
            run_id: req.run_id,
            idempotency_key: Some(req.idempotency_key),
            created_at: OffsetDateTime::now_utc(),
        };
        state.entries.push(entry.clone());
        Ok(Some(entry))
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

    async fn void_expired_holds(&self, now: OffsetDateTime) -> Result<u64, LedgerError> {
        let mut state = self.lock();
        let mut released = 0_u64;
        for hold in state.holds.values_mut() {
            let expired = hold.expires_at.is_some_and(|expiry| expiry <= now);
            if hold.status == HoldStatus::Open && expired {
                hold.status = HoldStatus::Voided;
                released += 1;
            }
        }
        Ok(released)
    }

    async fn extend_hold(
        &self,
        reservation: ReservationId,
        expires_at: OffsetDateTime,
    ) -> Result<(), LedgerError> {
        let mut state = self.lock();
        match state.holds.get_mut(&reservation) {
            None => Err(LedgerError::ReservationNotFound(reservation)),
            Some(hold) => match hold.status {
                HoldStatus::Open => {
                    hold.expires_at = Some(expires_at);
                    Ok(())
                }
                HoldStatus::Settled | HoldStatus::Voided => {
                    Err(LedgerError::ReservationClosed(reservation))
                }
            },
        }
    }

    async fn void_run(&self, run: RunId) -> Result<RunVoidSummary, LedgerError> {
        let mut state = self.lock();
        // Snapshot the run's holds before mutating, so the borrow on `holds` is released.
        let members: Vec<(ReservationId, HoldStatus, AccountId, Option<EntryId>)> = state
            .holds
            .iter()
            .filter(|(_, hold)| hold.run_id == Some(run))
            .map(|(id, hold)| (*id, hold.status, hold.account, hold.settle_entry))
            .collect();

        let mut summary = RunVoidSummary::default();
        for (reservation, status, account, settle_entry) in members {
            match status {
                HoldStatus::Voided => {}
                HoldStatus::Open => {
                    if let Some(hold) = state.holds.get_mut(&reservation) {
                        hold.status = HoldStatus::Voided;
                    }
                    summary.holds_released += 1;
                }
                HoldStatus::Settled => {
                    let settle_id = settle_entry.expect("a settled hold records its settle entry");
                    let key = run_void_refund_key(run, reservation);
                    // Reverse only the settle's unreversed remainder (idempotent on re-void; never
                    // double-refunds a settle already corrected by a manual credit-note).
                    let amount = unreversed_remainder(&state, settle_id);
                    if !amount.is_positive() {
                        continue;
                    }
                    if let Some(system) = state.accounts.get_mut(&self.system) {
                        system.settled -= amount;
                    }
                    let balance_after = {
                        let row = state
                            .accounts
                            .get_mut(&account)
                            .expect("the hold's account exists");
                        row.settled += amount;
                        row.settled
                    };
                    state.entries.push(LedgerEntry {
                        id: EntryId::new(),
                        account_id: account,
                        paired_account_id: self.system,
                        entry_type: EntryType::Refund,
                        delta_credits: amount,
                        balance_after,
                        source: None,
                        revenue_recognizable: false,
                        reverses_entry_id: Some(settle_id),
                        reservation_id: Some(reservation),
                        run_id: Some(run),
                        idempotency_key: Some(key),
                        created_at: OffsetDateTime::now_utc(),
                    });
                    summary.charges_refunded += 1;
                    summary.credits_refunded += amount;
                }
            }
        }

        // Reverse the run's *direct* charges (post-hoc /v1/usage metering, not tied to a reservation),
        // each by its unreversed remainder. Snapshot the run's usage entries before mutating.
        let charges: Vec<(EntryId, AccountId)> = state
            .entries
            .iter()
            .filter(|entry| entry.run_id == Some(run) && entry.entry_type == EntryType::Usage)
            .map(|entry| (entry.id, entry.account_id))
            .collect();
        for (charge_id, account) in charges {
            let key = run_void_charge_refund_key(run, charge_id);
            let amount = unreversed_remainder(&state, charge_id);
            if !amount.is_positive() {
                continue;
            }
            if let Some(system) = state.accounts.get_mut(&self.system) {
                system.settled -= amount;
            }
            let balance_after = {
                let row = state
                    .accounts
                    .get_mut(&account)
                    .expect("the charge's account exists");
                row.settled += amount;
                row.settled
            };
            state.entries.push(LedgerEntry {
                id: EntryId::new(),
                account_id: account,
                paired_account_id: self.system,
                entry_type: EntryType::Refund,
                delta_credits: amount,
                balance_after,
                source: None,
                revenue_recognizable: false,
                reverses_entry_id: Some(charge_id),
                reservation_id: None,
                run_id: Some(run),
                idempotency_key: Some(key),
                created_at: OffsetDateTime::now_utc(),
            });
            summary.charges_refunded += 1;
            summary.credits_refunded += amount;
        }
        Ok(summary)
    }

    async fn open_lease(&self, req: LeaseRequest) -> Result<LedgerAccount, LedgerError> {
        if !req.amount.is_positive() {
            return Err(LedgerError::NonPositiveAmount);
        }
        let mut state = self.lock();
        let (org_id, no_overdraft) = match state.accounts.get(&req.parent) {
            None => return Err(LedgerError::AccountNotFound(req.parent)),
            Some(row) => (row.account.org_id, row.account.no_overdraft),
        };
        let available = {
            let settled = state
                .accounts
                .get(&req.parent)
                .expect("parent checked")
                .settled;
            settled - state.held(req.parent)
        };
        if no_overdraft && available < req.amount {
            return Err(LedgerError::InsufficientFunds {
                available,
                requested: req.amount,
            });
        }
        let child = LedgerAccount {
            id: AccountId::new(),
            org_id,
            scope: AccountScope::Session,
            no_overdraft: true,
            parent_id: Some(req.parent),
        };
        state
            .accounts
            .insert(child.id, AccountRow::new(child.clone()));
        post_transfer(&mut state, req.parent, child.id, req.amount);
        Ok(child)
    }

    async fn close_lease(&self, lease: AccountId) -> Result<Credit, LedgerError> {
        let mut state = self.lock();
        let parent = match state.accounts.get(&lease) {
            None => return Err(LedgerError::AccountNotFound(lease)),
            Some(row) => match row.account.parent_id {
                None => return Err(LedgerError::NotALease(lease)),
                Some(parent) => parent,
            },
        };
        let available = {
            let settled = state.accounts.get(&lease).expect("lease checked").settled;
            settled - state.held(lease)
        };
        if !available.is_positive() {
            return Ok(Credit::ZERO);
        }
        post_transfer(&mut state, lease, parent, available);
        Ok(available)
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
