//! Derived account balances.

use meter_core::Credit;
use serde::{Deserialize, Serialize};

/// A point-in-time balance: settled credits, and credits currently held by open reservations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub settled: Credit,
    pub held: Credit,
}

impl Balance {
    /// Credits available to spend right now: settled minus open holds.
    #[must_use]
    pub fn available(self) -> Credit {
        self.settled - self.held
    }
}
