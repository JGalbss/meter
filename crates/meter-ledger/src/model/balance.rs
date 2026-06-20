//! Derived account balances.

use meter_core::Credit;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A point-in-time balance: settled credits, and credits currently held by open reservations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Balance {
    /// Settled credit balance, as an exact decimal string.
    #[schema(value_type = String)]
    pub settled: Credit,
    /// Credits currently locked by open reservations, as an exact decimal string.
    #[schema(value_type = String)]
    pub held: Credit,
}

impl Balance {
    /// Credits available to spend right now: settled minus open holds.
    #[must_use]
    pub fn available(self) -> Credit {
        self.settled - self.held
    }
}
