//! In-memory reference implementation of [`crate::LedgerBackend`].
//!
//! The simplest correct double-entry ledger: every grant and settle is a paired transfer against a
//! single system account, so the sum of all balances is always exactly zero (conservation). It exists
//! to (1) let the reserve/settle and no-overdraft logic be property-tested before any database exists,
//! and (2) act as the oracle the Postgres and `TigerBeetle` backends must match byte-for-byte.

mod ops;
mod state;
#[cfg(test)]
mod tests;

use std::sync::{Mutex, MutexGuard};

use meter_core::{AccountId, Credit};

use crate::model::{AccountScope, LedgerAccount, SYSTEM_ACCOUNT, SYSTEM_ORG};

use self::state::{AccountRow, State};

/// An entirely in-memory [`crate::LedgerBackend`]. Cheap to construct; safe to share across tasks.
#[derive(Debug)]
pub struct InMemoryLedger {
    state: Mutex<State>,
    system: AccountId,
}

impl InMemoryLedger {
    /// Create an empty ledger with its single system (mint + usage) account.
    #[must_use]
    pub fn new() -> Self {
        let system = SYSTEM_ACCOUNT;
        let mut state = State::default();
        state.accounts.insert(
            system,
            AccountRow::new(LedgerAccount {
                id: system,
                org_id: SYSTEM_ORG,
                scope: AccountScope::System,
                no_overdraft: false,
                parent_id: None,
            }),
        );
        Self {
            state: Mutex::new(state),
            system,
        }
    }

    /// The id of the system account every transfer pairs against.
    #[must_use]
    pub const fn system_account(&self) -> AccountId {
        self.system
    }

    /// Audit helper: the sum of every account's settled balance. Conservation requires it to be zero
    /// after any sequence of operations.
    #[must_use]
    pub fn net_settled(&self) -> Credit {
        self.lock().accounts.values().map(|row| row.settled).sum()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("ledger mutex poisoned")
    }
}

impl Default for InMemoryLedger {
    fn default() -> Self {
        Self::new()
    }
}
