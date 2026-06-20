//! Rate-card simulation: re-rate historical usage against a proposed card to preview the impact of a
//! pricing change before adopting it.
//!
//! This is a pure projection over [`price_usage`] — it never touches the ledger or any store. Given a
//! stream of past [`Usage`] events, it prices each one under both the current and the proposed card
//! and reports the per-event and aggregate credit delta. The credit layer rounds exactly once per
//! event (as in production), so the simulated totals match what would actually be charged.

use meter_core::{Credit, Money};

use crate::card::RateCard;
use crate::error::PricingError;
use crate::price::{price_usage, PricedUsage};
use crate::usage::Usage;

/// One event priced under both cards, with the credit difference (`proposed − current`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerateLine {
    pub current: PricedUsage,
    pub proposed: PricedUsage,
    /// `proposed.credits − current.credits`: positive means the proposed card charges more.
    pub credit_delta: Credit,
}

/// Aggregate result of re-rating a stream of usage events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerateSummary {
    pub event_count: usize,
    pub credits_current: Credit,
    pub credits_proposed: Credit,
    /// `credits_proposed − credits_current` over every event.
    pub credit_delta: Credit,
    /// Per-event detail, in input order.
    pub lines: Vec<RerateLine>,
}

/// Re-rate one usage event against the current and proposed cards, returning both priced results and
/// their credit delta. Both cards must price in the same currency as `credit_value`.
pub fn rerate_event(
    usage: &Usage,
    current: &RateCard,
    proposed: &RateCard,
    credit_value: &Money,
) -> Result<RerateLine, PricingError> {
    let current_priced = price_usage(usage, current, credit_value)?;
    let proposed_priced = price_usage(usage, proposed, credit_value)?;
    // Credits are currency-less decimals, so the delta is always well-defined.
    let credit_delta = proposed_priced.credits - current_priced.credits;
    Ok(RerateLine {
        current: current_priced,
        proposed: proposed_priced,
        credit_delta,
    })
}

/// Re-rate a stream of historical usage events against a proposed card, summing the credit impact.
///
/// Deterministic and order-preserving: each event is priced independently (credits rounded once per
/// event), so the aggregate equals the sum of what each event would be charged under each card.
pub fn simulate_rerate(
    usages: &[Usage],
    current: &RateCard,
    proposed: &RateCard,
    credit_value: &Money,
) -> Result<RerateSummary, PricingError> {
    let mut credits_current = Credit::ZERO;
    let mut credits_proposed = Credit::ZERO;
    let mut lines = Vec::with_capacity(usages.len());
    for usage in usages {
        let line = rerate_event(usage, current, proposed, credit_value)?;
        credits_current += line.current.credits;
        credits_proposed += line.proposed.credits;
        lines.push(line);
    }
    let credit_delta = credits_proposed - credits_current;
    Ok(RerateSummary {
        event_count: usages.len(),
        credits_current,
        credits_proposed,
        credit_delta,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Margin, RateCardKind};
    use crate::component::{ChargeModel, PriceComponent};
    use crate::dimension::{ContextTier, Modality, PricingDimension, Unit};
    use meter_core::{Currency, RateCardId};
    use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::new("USD").expect("valid currency")
    }

    /// A two-dimension token card at the given input/output per-token prices and margin multiplier.
    fn card(input: rust_decimal::Decimal, output: rust_decimal::Decimal, margin: &str) -> RateCard {
        let component = |dimension, price| PriceComponent {
            dimension,
            modality: Modality::Text,
            context_tier: ContextTier::Standard,
            unit: Unit::Token,
            charge_model: ChargeModel::Standard,
            unit_price: Money::new(price, usd()),
        };
        RateCard {
            id: RateCardId::new(),
            kind: RateCardKind::ProviderCost,
            currency: usd(),
            version: 1,
            margin: Margin::from_multiplier(margin.parse().expect("decimal")),
            components: vec![
                component(PricingDimension::InputUncached, input),
                component(PricingDimension::Output, output),
            ],
        }
    }

    fn usage(input: i64, output: i64) -> Usage {
        Usage::new(Modality::Text, ContextTier::Standard)
            .with(
                PricingDimension::InputUncached,
                rust_decimal::Decimal::from(input),
            )
            .with(
                PricingDimension::Output,
                rust_decimal::Decimal::from(output),
            )
    }

    #[test]
    fn rerate_reports_the_credit_delta_for_a_price_increase() {
        let credit_value = Money::new(dec!(0.01), usd()); // 1 credit = 1 cent
        let current = card(dec!(0.000003), dec!(0.000015), "1.00"); // $3/$15 per M, no margin
        let proposed = card(dec!(0.000004), dec!(0.000020), "1.00"); // pricier
        let line = rerate_event(&usage(1000, 500), &current, &proposed, &credit_value).unwrap();
        // current: 1000*3e-6 + 500*15e-6 = 0.0105 -> /0.01 = 1.05 credits
        // proposed: 1000*4e-6 + 500*20e-6 = 0.014 -> /0.01 = 1.4 credits
        assert_eq!(line.current.credits.value(), dec!(1.05));
        assert_eq!(line.proposed.credits.value(), dec!(1.4));
        assert_eq!(line.credit_delta.value(), dec!(0.35));
    }

    #[test]
    fn simulate_sums_a_stream_and_is_order_preserving() {
        let credit_value = Money::new(dec!(0.01), usd());
        let current = card(dec!(0.000003), dec!(0.000015), "1.00");
        let proposed = card(dec!(0.000003), dec!(0.000015), "1.20"); // same cost, +20% margin
        let usages = vec![usage(1000, 500), usage(2000, 0)];
        let summary = simulate_rerate(&usages, &current, &proposed, &credit_value).unwrap();
        assert_eq!(summary.event_count, 2);
        assert_eq!(summary.lines.len(), 2);
        // event 1: 0.0105 -> 1.05 / 1.26 ; event 2: 2000*3e-6=0.006 -> 0.6 / 0.72
        assert_eq!(summary.credits_current.value(), dec!(1.65)); // 1.05 + 0.6
        assert_eq!(summary.credits_proposed.value(), dec!(1.98)); // 1.26 + 0.72
        assert_eq!(summary.credit_delta.value(), dec!(0.33));
    }

    #[test]
    fn empty_stream_is_a_zero_summary() {
        let credit_value = Money::new(dec!(0.01), usd());
        let c = card(dec!(0.000003), dec!(0.000015), "1.00");
        let summary = simulate_rerate(&[], &c, &c, &credit_value).unwrap();
        assert_eq!(summary.event_count, 0);
        assert_eq!(summary.credit_delta.value(), dec!(0));
        assert!(summary.lines.is_empty());
    }

    #[test]
    fn a_cheaper_proposed_card_yields_a_negative_delta() {
        let credit_value = Money::new(dec!(0.01), usd());
        let current = card(dec!(0.000006), dec!(0.000030), "1.00");
        let proposed = card(dec!(0.000003), dec!(0.000015), "1.00"); // half price
        let summary =
            simulate_rerate(&[usage(1000, 500)], &current, &proposed, &credit_value).unwrap();
        assert!(summary.credit_delta.value().is_sign_negative());
        assert_eq!(summary.credit_delta.value(), dec!(-1.05));
    }
}
