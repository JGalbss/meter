//! Double-entry credit ledger for meter.
//!
//! This crate defines [`LedgerBackend`] — the single seam through which all money-truth flows — its
//! domain [`model`], the operation [`request`] types, and an in-memory reference implementation
//! ([`InMemoryLedger`]) that serves as the conformance oracle every storage backend must match.
//!
//! The ledger is append-only: balances are *derived* from immutable entries, never edited in place.
//! Reversals (refunds, chargebacks, voids, amendments) are new entries that reference the entry they
//! reverse — which is what makes "edit an event" and "void a failed run" clean to expose at the API
//! layer without ever corrupting the audit trail.

#![forbid(unsafe_code)]

pub mod backend;
pub mod error;
pub mod memory;
pub mod model;
pub mod request;

pub use backend::LedgerBackend;
pub use error::LedgerError;
pub use memory::InMemoryLedger;
pub use model::{
    AccountScope, Balance, CreditSource, EntryType, LedgerAccount, LedgerEntry, LimitClass,
    ReservationId,
};
pub use request::{GrantRequest, NewAccount, ReserveOutcome, ReserveRequest, SettleRequest};
