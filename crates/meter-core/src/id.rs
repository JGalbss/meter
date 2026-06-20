//! Strongly-typed, time-ordered identifiers.
//!
//! [`Id<T>`] wraps a UUIDv7 (time-ordered, so it indexes well in Postgres) and is parameterised by a
//! zero-sized marker type, so an [`OrgId`] can never be passed where a [`UserId`] is expected. The
//! marker is carried as `PhantomData<fn() -> T>`, which keeps `Id<T>` `Copy + Send + Sync` regardless
//! of `T`.

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// A strongly-typed identifier for entity `T`, backed by a UUIDv7.
pub struct Id<T> {
    value: Uuid,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// Generate a fresh, time-ordered identifier.
    #[must_use]
    pub fn new() -> Self {
        Self::from_uuid(Uuid::now_v7())
    }

    /// Wrap an existing UUID as a typed id.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self {
            value,
            _marker: PhantomData,
        }
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.value
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Manual trait impls: deriving would wrongly require the marker `T` to implement these too.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<T> Eq for Id<T> {}
impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}
impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
impl<T> FromStr for Id<T> {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(Uuid::parse_str(s)?))
    }
}
impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}
impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_uuid(Uuid::deserialize(deserializer)?))
    }
}

/// Declare entity marker types and their id aliases in one place.
macro_rules! entities {
    ($($(#[$meta:meta])* $marker:ident => $alias:ident),* $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub enum $marker {}
            #[doc = concat!("Identifier for a `", stringify!($marker), "`.")]
            pub type $alias = Id<$marker>;
        )*
    };
}

entities! {
    /// A top-level tenant / organization.
    Org => OrgId,
    /// A team within an organization.
    Team => TeamId,
    /// A user (human or service principal).
    User => UserId,
    /// An RBAC role.
    Role => RoleId,
    /// A billable product.
    Product => ProductId,
    /// An agent that produces usage.
    Agent => AgentId,
    /// A ledger account (holds a credit balance).
    Account => AccountId,
    /// A ledger transaction (a balanced set of entries).
    Transaction => TransactionId,
    /// A rate card (pricing definition).
    RateCard => RateCardId,
    /// A credit grant.
    Grant => GrantId,
    /// A budget.
    Budget => BudgetId,
    /// An invoice.
    Invoice => InvoiceId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrips() {
        let id = OrgId::new();
        let parsed: OrgId = id.to_string().parse().expect("valid uuid");
        assert_eq!(id, parsed);
    }

    #[test]
    fn serializes_as_bare_uuid_string() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, format!("\"{id}\""));
        let back: UserId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn ids_are_time_ordered() {
        let first = AccountId::new();
        let second = AccountId::new();
        // UUIDv7 is time-ordered, so a later id sorts after an earlier one.
        assert!(second >= first);
    }
}
