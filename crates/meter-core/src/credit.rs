//! Credits: the tenant-defined unit of account.
//!
//! A credit is decoupled from any currency. Tenants meter and budget in credits, and separately peg a
//! credit to a cash value (e.g. 1 credit = $0.02) in their pricing configuration. Like money, credits
//! are exact decimals — never floats — and can be fractional for fine-grained metering.

use core::fmt;
use core::iter::Sum;
use core::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// An amount of credits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Credit(#[serde(with = "rust_decimal::serde::str")] Decimal);

impl Credit {
    /// Zero credits.
    pub const ZERO: Credit = Credit(Decimal::ZERO);

    /// Construct from an exact decimal amount.
    #[must_use]
    pub const fn from_decimal(value: Decimal) -> Self {
        Self(value)
    }

    /// The underlying decimal value.
    #[must_use]
    pub const fn value(self) -> Decimal {
        self.0
    }

    /// Whether this is exactly zero.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    /// Whether this is strictly negative.
    #[must_use]
    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    /// Whether this is strictly positive.
    #[must_use]
    pub fn is_positive(self) -> bool {
        !self.0.is_sign_negative() && !self.0.is_zero()
    }

    /// Scale by a dimensionless factor.
    #[must_use]
    pub fn scale_by(self, factor: Decimal) -> Self {
        Self(self.0 * factor)
    }
}

impl Add for Credit {
    type Output = Credit;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}
impl Sub for Credit {
    type Output = Credit;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}
impl Neg for Credit {
    type Output = Credit;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}
impl AddAssign for Credit {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}
impl SubAssign for Credit {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}
impl Sum for Credit {
    fn sum<I: Iterator<Item = Credit>>(iter: I) -> Self {
        iter.fold(Credit::ZERO, Add::add)
    }
}

impl fmt::Display for Credit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} credits", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn arithmetic_is_exact() {
        let a = Credit::from_decimal(dec!(10.5));
        let b = Credit::from_decimal(dec!(0.25));
        assert_eq!((a + b).value(), dec!(10.75));
        assert_eq!((a - b).value(), dec!(10.25));
        assert_eq!((-b).value(), dec!(-0.25));
    }

    #[test]
    fn sign_predicates() {
        assert!(Credit::ZERO.is_zero());
        assert!(Credit::from_decimal(dec!(1)).is_positive());
        assert!(Credit::from_decimal(dec!(-1)).is_negative());
        assert!(!Credit::ZERO.is_positive());
        assert!(!Credit::ZERO.is_negative());
    }

    #[test]
    fn sums_a_sequence() {
        let total: Credit = [dec!(1), dec!(2), dec!(3)]
            .into_iter()
            .map(Credit::from_decimal)
            .sum();
        assert_eq!(total.value(), dec!(6));
    }

    #[test]
    fn serializes_as_string() {
        let c = Credit::from_decimal(dec!(2.50));
        let json = serde_json::to_string(&c).expect("serialize");
        assert_eq!(json, "\"2.50\"");
        let back: Credit = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(c, back);
    }
}
