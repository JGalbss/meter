//! Exact-decimal money with an explicit currency.
//!
//! Money is never a float. Amounts are [`rust_decimal::Decimal`] (128-bit, 28–29 significant digits),
//! which is exact for currency math and for the very small per-token rates we deal with. Arithmetic
//! across different currencies is a typed error, never a silent mix.

use core::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors arising from money operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MoneyError {
    /// Attempted to combine amounts in different currencies.
    #[error("currency mismatch: {left} vs {right}")]
    CurrencyMismatch { left: Currency, right: Currency },
    /// A currency code was not three uppercase ASCII letters.
    #[error("invalid currency code: {0:?}")]
    InvalidCurrency(String),
}

/// An ISO-4217-style alphabetic currency code (three uppercase ASCII letters, e.g. `USD`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, utoipa::ToSchema)]
#[schema(value_type = String)]
pub struct Currency(String);

impl Currency {
    /// Construct a currency, validating the code shape.
    pub fn new(code: impl Into<String>) -> Result<Self, MoneyError> {
        let code = code.into();
        let well_formed = code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase());
        if !well_formed {
            return Err(MoneyError::InvalidCurrency(code));
        }
        Ok(Self(code))
    }

    /// The three-letter code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// Validate on the way in, even from untrusted serialized data.
impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        Self::new(code).map_err(serde::de::Error::custom)
    }
}

/// A monetary amount in a specific currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Money {
    /// The amount as an exact decimal string.
    #[serde(with = "rust_decimal::serde::str")]
    #[schema(value_type = String)]
    amount: Decimal,
    currency: Currency,
}

impl Money {
    /// Construct an amount in the given currency.
    #[must_use]
    pub const fn new(amount: Decimal, currency: Currency) -> Self {
        Self { amount, currency }
    }

    /// Zero in the given currency.
    #[must_use]
    pub const fn zero(currency: Currency) -> Self {
        Self {
            amount: Decimal::ZERO,
            currency,
        }
    }

    /// The signed amount.
    #[must_use]
    pub const fn amount(&self) -> Decimal {
        self.amount
    }

    /// The currency.
    #[must_use]
    pub const fn currency(&self) -> &Currency {
        &self.currency
    }

    /// Whether the amount is exactly zero.
    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.amount.is_zero()
    }

    /// Whether the amount is strictly negative.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.amount.is_sign_negative() && !self.amount.is_zero()
    }

    /// Add two amounts, requiring matching currencies.
    pub fn try_add(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_same_currency(other)?;
        Ok(Self::new(self.amount + other.amount, self.currency.clone()))
    }

    /// Subtract `other` from `self`, requiring matching currencies.
    pub fn try_sub(&self, other: &Self) -> Result<Self, MoneyError> {
        self.ensure_same_currency(other)?;
        Ok(Self::new(self.amount - other.amount, self.currency.clone()))
    }

    /// Scale the amount by a dimensionless factor (e.g. quantity × unit price).
    #[must_use]
    pub fn scale_by(&self, factor: Decimal) -> Self {
        Self::new(self.amount * factor, self.currency.clone())
    }

    fn ensure_same_currency(&self, other: &Self) -> Result<(), MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch {
                left: self.currency.clone(),
                right: other.currency.clone(),
            });
        }
        Ok(())
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.amount, self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::new("USD").expect("valid")
    }

    #[test]
    fn rejects_malformed_currency() {
        assert!(Currency::new("usd").is_err());
        assert!(Currency::new("US").is_err());
        assert!(Currency::new("DOLLAR").is_err());
        assert!(Currency::new("USD").is_ok());
    }

    #[test]
    fn adds_within_a_currency() {
        let a = Money::new(dec!(1.50), usd());
        let b = Money::new(dec!(2.25), usd());
        assert_eq!(a.try_add(&b).expect("same currency").amount(), dec!(3.75));
    }

    #[test]
    fn refuses_cross_currency_arithmetic() {
        let a = Money::new(dec!(1), usd());
        let b = Money::new(dec!(1), Currency::new("EUR").expect("valid"));
        assert!(matches!(
            a.try_add(&b),
            Err(MoneyError::CurrencyMismatch { .. })
        ));
    }

    #[test]
    fn scales_for_unit_pricing() {
        // 1_000 tokens at $0.000003 / token.
        let unit = Money::new(dec!(0.000003), usd());
        assert_eq!(unit.scale_by(dec!(1000)).amount(), dec!(0.003000));
    }

    #[test]
    fn serializes_amount_as_string() {
        let m = Money::new(dec!(0.000003), usd());
        let json = serde_json::to_string(&m).expect("serialize");
        assert!(json.contains("\"0.000003\""), "got {json}");
        let back: Money = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }
}
