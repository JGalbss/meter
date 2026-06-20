//! Internal mutable state of the in-memory ledger. Not part of the public API.

use std::collections::HashMap;

use meter_core::{AccountId, Credit, EntryId};

use crate::model::{LedgerAccount, LedgerEntry, ReservationId};

/// A stored account and its current settled balance.
#[derive(Debug)]
pub(super) struct AccountRow {
    pub(super) account: LedgerAccount,
    pub(super) settled: Credit,
}

impl AccountRow {
    pub(super) fn new(account: LedgerAccount) -> Self {
        Self {
            account,
            settled: Credit::ZERO,
        }
    }
}

/// The lifecycle of a reservation hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HoldStatus {
    Open,
    Settled,
    Voided,
}

/// A durable hold against an account.
#[derive(Debug)]
pub(super) struct Hold {
    pub(super) account: AccountId,
    pub(super) amount: Credit,
    pub(super) status: HoldStatus,
    pub(super) settle_entry: Option<EntryId>,
}

/// All mutable ledger state behind a single mutex.
#[derive(Debug, Default)]
pub(super) struct State {
    pub(super) accounts: HashMap<AccountId, AccountRow>,
    pub(super) holds: HashMap<ReservationId, Hold>,
    pub(super) entries: Vec<LedgerEntry>,
}

impl State {
    /// Sum of credits locked by open holds against an account.
    pub(super) fn held(&self, account: AccountId) -> Credit {
        self.holds
            .values()
            .filter(|hold| hold.account == account && hold.status == HoldStatus::Open)
            .map(|hold| hold.amount)
            .sum()
    }
}
