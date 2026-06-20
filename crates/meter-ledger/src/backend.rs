//! The ledger backend trait — the seam every storage backend implements.

use async_trait::async_trait;
use meter_core::{AccountId, Credit};
use time::OffsetDateTime;

use crate::error::LedgerError;
use crate::model::{Balance, LedgerAccount, LedgerEntry, ReservationId};
use crate::request::{
    ChargeRequest, GrantRequest, LeaseRequest, NewAccount, ReserveOutcome, ReserveRequest,
    SettleRequest,
};

/// A pluggable double-entry credit ledger.
///
/// All money-truth flows through this trait, so the choice of storage (Postgres by default, an
/// optional TigerBeetle accelerator) is invisible to the rest of the system. Implementations must be
/// safe under concurrency and make `grant`, `reserve`, `settle`, and `void` idempotent on their keys.
/// The [`crate::InMemoryLedger`] is the conformance oracle every backend is tested against.
#[async_trait]
pub trait LedgerBackend: Send + Sync {
    /// Open a new account and return it.
    async fn open_account(&self, req: NewAccount) -> Result<LedgerAccount, LedgerError>;

    /// The derived balance of an account.
    async fn balance(&self, account: AccountId) -> Result<Balance, LedgerError>;

    /// Add credits to an account (a paired transfer from the system account).
    async fn grant(&self, req: GrantRequest) -> Result<LedgerEntry, LedgerError>;

    /// Place a durable hold before a spend. The hold is the authorization to spend.
    async fn reserve(&self, req: ReserveRequest) -> Result<ReserveOutcome, LedgerError>;

    /// Close a reservation at the actual amount, posting the priced usage.
    async fn settle(&self, req: SettleRequest) -> Result<LedgerEntry, LedgerError>;

    /// Charge usage directly (post-hoc, no prior reservation). Always posts; idempotent on its key.
    async fn charge(&self, req: ChargeRequest) -> Result<LedgerEntry, LedgerError>;

    /// Release an open reservation without charging it (e.g. a failed or abandoned run).
    async fn void(&self, reservation: ReservationId) -> Result<(), LedgerError>;

    /// Release every open hold whose `expires_at` is at or before `now`, returning the count released.
    /// This is the auto-void sweep for stranded reservations; it never touches settled or unexpired
    /// holds, so the released credits return to the account exactly as a manual [`void`](Self::void).
    async fn void_expired_holds(&self, now: OffsetDateTime) -> Result<u64, LedgerError>;

    /// Lease credits from a parent pool into a fresh per-session sub-balance (hot-account mitigation).
    /// Moves `amount` from the parent to a new `Session` child via a conserving transfer; refuses to
    /// overdraw a no-overdraft parent. The session then reserves/settles against the returned account.
    async fn open_lease(&self, req: LeaseRequest) -> Result<LedgerAccount, LedgerError>;

    /// Return a lease's unused balance (`settled − held`) to its parent and report the amount returned.
    /// Safe to call with holds still open — it only returns what is not reserved.
    async fn close_lease(&self, lease: AccountId) -> Result<Credit, LedgerError>;

    /// Every entry touching an account, for audit and conformance checks.
    async fn entries(&self, account: AccountId) -> Result<Vec<LedgerEntry>, LedgerError>;
}
