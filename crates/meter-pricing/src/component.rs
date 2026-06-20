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
/// - [`Package`](ChargeModel::Package): units are sold in fixed-size bundles — the quantity is rounded
///   *up* to a whole number of packages, each charged at the component's `unit_price` (e.g. "$0.01 per
///   1000 tokens, any partial 1000 rounds up").
///
/// Tiered schedules must be ascending by `up_to` and end with an unbounded [`PriceTier::rest`] tier;
/// otherwise pricing returns [`PricingError::InvalidTierSchedule`]. A `Package` size must be positive
/// or pricing returns [`PricingError::InvalidPackageSize`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeModel {
    Standard,
    Graduated(Vec<PriceTier>),
    Volume(Vec<PriceTier>),
    /// Bundled pricing: charge `unit_price` per `size`-unit package, rounding the quantity up.
    Package {
        size: Decimal,
    },
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
    /// schedule; `Package` rounds the quantity up to whole bundles. Negative quantities are treated as
    /// zero (a usage quantity is never negative).
    pub fn charge(&self, quantity: Decimal) -> Result<Money, PricingError> {
        let quantity = quantity.max(Decimal::ZERO);
        match &self.charge_model {
            ChargeModel::Standard => Ok(self.unit_price.scale_by(quantity)),
            ChargeModel::Graduated(tiers) => graduated_charge(tiers, quantity),
            ChargeModel::Volume(tiers) => volume_charge(tiers, quantity),
            ChargeModel::Package { size } => self.package_charge(*size, quantity),
        }
    }

    /// Bundled pricing: round `quantity` up to a whole number of `size`-unit packages, each charged at
    /// the component's `unit_price`. A zero/negative `size` is invalid.
    fn package_charge(&self, size: Decimal, quantity: Decimal) -> Result<Money, PricingError> {
        if size <= Decimal::ZERO {
            return Err(PricingError::InvalidPackageSize);
        }
        let packages = (quantity / size).ceil();
        Ok(self.unit_price.scale_by(packages))
    }

    /// Validate the charge model's tier schedule (for `Graduated`/`Volume`): non-empty, strictly
    /// ascending bounds, and an unbounded final tier. `Package` requires a positive size. `Standard`
    /// is always valid.
    pub fn validate(&self) -> Result<(), PricingError> {
        match &self.charge_model {
            ChargeModel::Standard => Ok(()),
            ChargeModel::Graduated(tiers) | ChargeModel::Volume(tiers) => validate_tiers(tiers),
            ChargeModel::Package { size } if *size > Decimal::ZERO => Ok(()),
            ChargeModel::Package { .. } => Err(PricingError::InvalidPackageSize),
        }
    }
}

/// A tier schedule is well-formed iff it is non-empty, every bounded tier strictly ascends above the
/// previous bound (and above zero), and only the final tier is unbounded.
fn validate_tiers(tiers: &[PriceTier]) -> Result<(), PricingError> {
    if tiers.is_empty() {
        return Err(PricingError::InvalidTierSchedule);
    }
    let last = tiers.len() - 1;
    let mut previous = Decimal::ZERO;
    for (index, tier) in tiers.iter().enumerate() {
        match (tier.up_to, index == last) {
            (None, true) => {}
            (Some(upper), false) if upper > previous => previous = upper,
            // Unbounded before the end, a bounded final tier, or a non-ascending bound.
            _ => return Err(PricingError::InvalidTierSchedule),
        }
    }
    Ok(())
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
    fn package_rounds_up_to_whole_bundles() {
        // $2 per 1000-unit package.
        let c = component(ChargeModel::Package { size: dec!(1000) });
        assert_eq!(c.charge(dec!(0)).unwrap().amount(), dec!(0)); // 0 packages
        assert_eq!(c.charge(dec!(1)).unwrap().amount(), dec!(2)); // rounds up to 1
        assert_eq!(c.charge(dec!(1000)).unwrap().amount(), dec!(2)); // exactly 1
        assert_eq!(c.charge(dec!(1001)).unwrap().amount(), dec!(4)); // spills into a 2nd
        assert_eq!(c.charge(dec!(2500)).unwrap().amount(), dec!(6)); // ceil(2.5) = 3
    }

    #[test]
    fn package_with_a_non_positive_size_is_invalid() {
        assert_eq!(
            component(ChargeModel::Package { size: dec!(0) }).charge(dec!(10)),
            Err(PricingError::InvalidPackageSize)
        );
        assert_eq!(
            component(ChargeModel::Package { size: dec!(-5) }).validate(),
            Err(PricingError::InvalidPackageSize)
        );
        assert_eq!(
            component(ChargeModel::Package { size: dec!(1000) }).validate(),
            Ok(())
        );
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

    #[test]
    fn validate_accepts_well_formed_schedules() {
        assert_eq!(component(ChargeModel::Standard).validate(), Ok(()));
        assert_eq!(
            component(ChargeModel::Graduated(tiers())).validate(),
            Ok(())
        );
        assert_eq!(component(ChargeModel::Volume(tiers())).validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_malformed_schedules() {
        // Empty.
        assert_eq!(
            component(ChargeModel::Graduated(vec![])).validate(),
            Err(PricingError::InvalidTierSchedule)
        );
        // No unbounded final tier.
        let bounded = vec![PriceTier::up_to(dec!(100), Money::new(dec!(1), usd()))];
        assert_eq!(
            component(ChargeModel::Volume(bounded)).validate(),
            Err(PricingError::InvalidTierSchedule)
        );
        // Non-ascending bounds.
        let non_ascending = vec![
            PriceTier::up_to(dec!(100), Money::new(dec!(1), usd())),
            PriceTier::up_to(dec!(100), Money::new(dec!(1), usd())),
            PriceTier::rest(Money::new(dec!(1), usd())),
        ];
        assert_eq!(
            component(ChargeModel::Graduated(non_ascending)).validate(),
            Err(PricingError::InvalidTierSchedule)
        );
    }
}
