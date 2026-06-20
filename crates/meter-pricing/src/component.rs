//! Price components: one priced cell of a rate card's dimensional matrix.

use meter_core::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::dimension::{ContextTier, Modality, PricingDimension, Unit};
use crate::error::PricingError;

/// One band of a tiered charge: it covers quantity up to `up_to` cumulative units (inclusive), or is
/// the final unbounded band when `up_to` is `None`. The band's units are charged at `unit_price`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceTier {
    /// Inclusive cumulative upper bound of this band in units; `None` means unbounded (the last tier).
    pub up_to: Option<Decimal>,
    pub unit_price: Money,
}

impl PriceTier {
    /// A bounded tier covering up to `up_to` cumulative units at `unit_price`.
    #[must_use]
    pub fn up_to(up_to: Decimal, unit_price: Money) -> Self {
        Self {
            up_to: Some(up_to),
            unit_price,
        }
    }

    /// The final, unbounded tier: every unit beyond the prior tiers is charged at `unit_price`.
    #[must_use]
    pub fn rest(unit_price: Money) -> Self {
        Self {
            up_to: None,
            unit_price,
        }
    }
}

/// How a component computes its charge from a quantity.
///
/// - [`Standard`](ChargeModel::Standard): flat per-unit pricing (the component's `unit_price`).
/// - [`Graduated`](ChargeModel::Graduated): each tier's price applies only to the units that fall in
///   that tier's band (like marginal tax brackets).
/// - [`Volume`](ChargeModel::Volume): the single tier the *total* quantity lands in prices every unit.
///
/// Tiered schedules must be ascending by `up_to` and end with an unbounded [`PriceTier::rest`] tier;
/// otherwise pricing returns [`PricingError::InvalidTierSchedule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeModel {
    Standard,
    Graduated(Vec<PriceTier>),
    Volume(Vec<PriceTier>),
}

/// One priced cell: a (dimension, modality, context-tier) charged at `unit_price` per `unit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceComponent {
    pub dimension: PricingDimension,
    pub modality: Modality,
    pub context_tier: ContextTier,
    pub unit: Unit,
    pub charge_model: ChargeModel,
    pub unit_price: Money,
}

impl PriceComponent {
    /// Whether this component prices the given dimension at the given modality and context tier.
    #[must_use]
    pub fn matches(
        &self,
        dimension: PricingDimension,
        modality: Modality,
        context_tier: ContextTier,
    ) -> bool {
        self.dimension == dimension
            && self.modality == modality
            && self.context_tier == context_tier
    }

    /// The charge for `quantity` units under this component's [`ChargeModel`].
    ///
    /// `Standard` is flat (`unit_price × quantity`); `Graduated`/`Volume` price against the tier
    /// schedule. Negative quantities are treated as zero (a usage quantity is never negative).
    pub fn charge(&self, quantity: Decimal) -> Result<Money, PricingError> {
        let quantity = quantity.max(Decimal::ZERO);
        match &self.charge_model {
            ChargeModel::Standard => Ok(self.unit_price.scale_by(quantity)),
            ChargeModel::Graduated(tiers) => graduated_charge(tiers, quantity),
            ChargeModel::Volume(tiers) => volume_charge(tiers, quantity),
        }
    }
}

/// The currency a tier schedule prices in (its first tier's), or an error if the schedule is empty.
fn schedule_currency(tiers: &[PriceTier]) -> Result<meter_core::Currency, PricingError> {
    tiers
        .first()
        .map(|tier| tier.unit_price.currency().clone())
        .ok_or(PricingError::InvalidTierSchedule)
}

/// Graduated (marginal) pricing: each tier prices only the units inside its band.
fn graduated_charge(tiers: &[PriceTier], quantity: Decimal) -> Result<Money, PricingError> {
    let mut total = Money::zero(schedule_currency(tiers)?);
    let mut lower = Decimal::ZERO;
    let mut remaining = quantity;
    for tier in tiers {
        if remaining <= Decimal::ZERO {
            break;
        }
        let band = match tier.up_to {
            // A bounded band must extend strictly above the previous bound.
            Some(upper) if upper > lower => upper - lower,
            Some(_) => return Err(PricingError::InvalidTierSchedule),
            None => remaining,
        };
        let qty = remaining.min(band);
        total = total
            .try_add(&tier.unit_price.scale_by(qty))
            .map_err(|_| PricingError::CurrencyMismatch)?;
        remaining -= qty;
        lower = tier.up_to.unwrap_or(lower);
    }
    // Units left over mean the schedule never reached an unbounded final tier.
    match remaining <= Decimal::ZERO {
        true => Ok(total),
        false => Err(PricingError::InvalidTierSchedule),
    }
}

/// Volume pricing: the single tier the total quantity lands in prices every unit.
fn volume_charge(tiers: &[PriceTier], quantity: Decimal) -> Result<Money, PricingError> {
    for tier in tiers {
        match tier.up_to {
            Some(upper) if quantity <= upper => return Ok(tier.unit_price.scale_by(quantity)),
            Some(_) => continue,
            None => return Ok(tier.unit_price.scale_by(quantity)),
        }
    }
    // No unbounded tier and the quantity exceeded every bounded one.
    Err(PricingError::InvalidTierSchedule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::Currency;
    use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::new("USD").expect("valid currency")
    }

    fn component(charge_model: ChargeModel) -> PriceComponent {
        PriceComponent {
            dimension: PricingDimension::Output,
            modality: Modality::Text,
            context_tier: ContextTier::Standard,
            unit: Unit::Token,
            charge_model,
            unit_price: Money::new(dec!(2), usd()),
        }
    }

    /// First 100 units @ $1, next 100 (to 200) @ $0.50, the rest @ $0.10.
    fn tiers() -> Vec<PriceTier> {
        vec![
            PriceTier::up_to(dec!(100), Money::new(dec!(1), usd())),
            PriceTier::up_to(dec!(200), Money::new(dec!(0.50), usd())),
            PriceTier::rest(Money::new(dec!(0.10), usd())),
        ]
    }

    #[test]
    fn standard_is_flat() {
        let c = component(ChargeModel::Standard);
        assert_eq!(c.charge(dec!(10)).unwrap().amount(), dec!(20)); // 10 * $2
    }

    #[test]
    fn graduated_prices_each_band_marginally() {
        let c = component(ChargeModel::Graduated(tiers()));
        // 250 units: 100*1 + 100*0.5 + 50*0.1 = 100 + 50 + 5 = 155
        assert_eq!(c.charge(dec!(250)).unwrap().amount(), dec!(155));
        // 150 units: 100*1 + 50*0.5 = 125
        assert_eq!(c.charge(dec!(150)).unwrap().amount(), dec!(125));
        // exactly on a boundary: 100 units = 100*1 = 100
        assert_eq!(c.charge(dec!(100)).unwrap().amount(), dec!(100));
        assert_eq!(c.charge(dec!(0)).unwrap().amount(), dec!(0));
    }

    #[test]
    fn volume_prices_all_units_at_the_landing_tier() {
        let c = component(ChargeModel::Volume(tiers()));
        // 250 lands in the unbounded tier: 250 * 0.10 = 25
        assert_eq!(c.charge(dec!(250)).unwrap().amount(), dec!(25));
        // 150 lands in the second tier (<=200): 150 * 0.50 = 75
        assert_eq!(c.charge(dec!(150)).unwrap().amount(), dec!(75));
        // 80 lands in the first tier (<=100): 80 * 1 = 80
        assert_eq!(c.charge(dec!(80)).unwrap().amount(), dec!(80));
    }

    #[test]
    fn graduated_without_an_unbounded_tail_is_invalid() {
        let bounded = vec![PriceTier::up_to(dec!(100), Money::new(dec!(1), usd()))];
        let c = component(ChargeModel::Graduated(bounded));
        // 150 units exceed the only (bounded) tier.
        assert_eq!(c.charge(dec!(150)), Err(PricingError::InvalidTierSchedule));
        // ...but a quantity within the schedule still prices.
        assert_eq!(c.charge(dec!(50)).unwrap().amount(), dec!(50));
    }

    #[test]
    fn non_ascending_graduated_tiers_are_invalid() {
        let bad = vec![
            PriceTier::up_to(dec!(100), Money::new(dec!(1), usd())),
            PriceTier::up_to(dec!(100), Money::new(dec!(0.5), usd())), // not strictly above
            PriceTier::rest(Money::new(dec!(0.1), usd())),
        ];
        let c = component(ChargeModel::Graduated(bad));
        assert_eq!(c.charge(dec!(150)), Err(PricingError::InvalidTierSchedule));
    }

    #[test]
    fn empty_tier_schedule_is_invalid() {
        assert_eq!(
            component(ChargeModel::Graduated(vec![])).charge(dec!(10)),
            Err(PricingError::InvalidTierSchedule)
        );
        assert_eq!(
            component(ChargeModel::Volume(vec![])).charge(dec!(10)),
            Err(PricingError::InvalidTierSchedule)
        );
    }
}
