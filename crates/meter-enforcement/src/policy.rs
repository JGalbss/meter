//! Reservation sizing policy.

use serde::{Deserialize, Serialize};

/// How a reservation is sized before the actual usage is known.
///
/// v1 ships [`ReservationPolicy::WorstCase`] — the caller supplies a worst-case estimate (e.g.
/// `max_output + reasoning + tools + cache-write`), which is reserved in full so a HARD limit can
/// never overdraft. A statistical (p95) policy is added later behind this enum without changing call
/// sites; the bounded overage sub-account is always the tail backstop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationPolicy {
    #[default]
    WorstCase,
}
