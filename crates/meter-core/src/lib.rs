//! Core domain primitives shared across the meter backend.
//!
//! This crate is deliberately tiny and dependency-light: it holds the value types that the rest of
//! the system is built on — strongly-typed identifiers ([`Id`]), exact-decimal [`Money`], and the
//! tenant-defined [`Credit`] unit of account. Nothing here touches IO, storage, or async.

#![forbid(unsafe_code)]

pub mod credit;
pub mod id;
pub mod money;

pub use credit::Credit;
pub use id::{
    AccountId, AgentId, BudgetId, GrantId, Id, InvoiceId, OrgId, ProductId, RateCardId, RoleId,
    TeamId, TransactionId, UserId,
};
pub use money::{Currency, Money, MoneyError};
