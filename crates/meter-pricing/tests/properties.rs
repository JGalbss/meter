//! Property tests for the pricing layer's invariants.
//!
//! These complement the example-based unit tests: they assert the *laws* the pricing math must obey —
//! linearity, exact margin application, tier monotonicity, the graduated/volume relationship, and
//! re-rate self-consistency — across a wide range of generated inputs.

use meter_core::{Currency, Money, RateCardId};
use meter_pricing::{
    cost, price_usage, simulate_rerate, to_credits, ChargeModel, ContextTier, Margin, Modality,
    PriceComponent, PriceTier, PricingDimension, RateCard, RateCardKind, Unit, Usage,
};
use proptest::prelude::*;
use rust_decimal::Decimal;

fn usd() -> Currency {
    Currency::new("USD").expect("valid currency")
}

/// A Decimal from "micros": `micros × 10^-6`, exact (no float).
fn micros(micros: u64) -> Decimal {
    Decimal::from(micros) / Decimal::from(1_000_000u64)
}

fn output_component(charge_model: ChargeModel, unit_price: Decimal) -> PriceComponent {
    PriceComponent {
        dimension: PricingDimension::Output,
        modality: Modality::Text,
        context_tier: ContextTier::Standard,
        unit: Unit::Token,
        charge_model,
        unit_price: Money::new(unit_price, usd()),
    }
}

fn card_with(component: PriceComponent, margin: Decimal) -> RateCard {
    RateCard {
        id: RateCardId::new(),
        kind: RateCardKind::ProviderCost,
        currency: usd(),
        version: 1,
        margin: Margin::from_multiplier(margin),
        components: vec![component],
    }
}

fn output_usage(quantity: u64) -> Usage {
    Usage::new(Modality::Text, ContextTier::Standard)
        .with(PricingDimension::Output, Decimal::from(quantity))
}

proptest! {
    /// Standard cost is linear in quantity: cost(a) + cost(b) == cost(a + b), exactly.
    #[test]
    fn standard_cost_is_linear(a in 0u64..1_000_000, b in 0u64..1_000_000, price in 1u64..1_000_000) {
        let card = card_with(output_component(ChargeModel::Standard, micros(price)), Decimal::ONE);
        let ca = cost(&output_usage(a), &card).unwrap();
        let cb = cost(&output_usage(b), &card).unwrap();
        let cab = cost(&output_usage(a + b), &card).unwrap();
        prop_assert_eq!(ca.amount() + cb.amount(), cab.amount());
    }

    /// The customer price is exactly the cost scaled by the margin multiplier (no premature rounding).
    #[test]
    fn margin_is_applied_exactly(qty in 0u64..1_000_000, price in 1u64..1_000_000, margin_bp in 100u64..50_000) {
        let multiplier = Decimal::from(margin_bp) / Decimal::from(100u64); // basis-point-ish multiplier
        let card = card_with(output_component(ChargeModel::Standard, micros(price)), multiplier);
        let credit_value = Money::new(micros(1), usd());
        let priced = price_usage(&output_usage(qty), &card, &credit_value).unwrap();
        prop_assert_eq!(priced.customer_price.amount(), priced.cogs.amount() * multiplier);
        prop_assert!(priced.credits.value() >= Decimal::ZERO);
    }

    /// Converting money to credits is monotonic: a larger amount never yields fewer credits.
    #[test]
    fn to_credits_is_monotonic(a in 0u64..10_000_000, delta in 0u64..10_000_000, value in 1u64..1_000_000) {
        let credit_value = Money::new(micros(value), usd());
        let lo = to_credits(&Money::new(micros(a), usd()), &credit_value).unwrap();
        let hi = to_credits(&Money::new(micros(a + delta), usd()), &credit_value).unwrap();
        prop_assert!(hi.value() >= lo.value());
    }

    /// A graduated charge is monotonic non-decreasing in quantity.
    #[test]
    fn graduated_charge_is_monotonic(q in 0u64..5_000, delta in 0u64..5_000) {
        let tiers = vec![
            PriceTier::up_to(Decimal::from(100u64), Money::new(micros(10), usd())),
            PriceTier::up_to(Decimal::from(1_000u64), Money::new(micros(5), usd())),
            PriceTier::rest(Money::new(micros(1), usd())),
        ];
        let card = card_with(output_component(ChargeModel::Graduated(tiers), Decimal::ONE), Decimal::ONE);
        let lo = cost(&output_usage(q), &card).unwrap();
        let hi = cost(&output_usage(q + delta), &card).unwrap();
        prop_assert!(hi.amount() >= lo.amount());
    }

    /// When every tier shares one price, graduated and volume both reduce to flat pricing — and equal it.
    #[test]
    fn graduated_and_volume_agree_when_flat(q in 0u64..10_000, price in 1u64..1_000_000) {
        let flat = micros(price);
        let one_tier = || vec![PriceTier::rest(Money::new(flat, usd()))];
        let graduated = card_with(output_component(ChargeModel::Graduated(one_tier()), Decimal::ONE), Decimal::ONE);
        let volume = card_with(output_component(ChargeModel::Volume(one_tier()), Decimal::ONE), Decimal::ONE);
        let standard = card_with(output_component(ChargeModel::Standard, flat), Decimal::ONE);
        let g = cost(&output_usage(q), &graduated).unwrap().amount();
        let v = cost(&output_usage(q), &volume).unwrap().amount();
        let s = cost(&output_usage(q), &standard).unwrap().amount();
        prop_assert_eq!(g, s);
        prop_assert_eq!(v, s);
    }

    /// Re-rating a usage stream against the *same* card yields a zero credit delta.
    #[test]
    fn rerate_against_same_card_is_zero(quantities in prop::collection::vec(0u64..1_000_000, 0..16), price in 1u64..1_000_000) {
        let card = card_with(output_component(ChargeModel::Standard, micros(price)), Decimal::ONE);
        let credit_value = Money::new(micros(1), usd());
        let usages: Vec<Usage> = quantities.iter().map(|q| output_usage(*q)).collect();
        let summary = simulate_rerate(&usages, &card, &card, &credit_value).unwrap();
        prop_assert_eq!(summary.credit_delta.value(), Decimal::ZERO);
        prop_assert_eq!(summary.credits_current.value(), summary.credits_proposed.value());
        prop_assert_eq!(summary.event_count, quantities.len());
    }
}
