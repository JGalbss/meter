//! The pricing functions: usage -> cost -> customer price -> credits.

use meter_core::{Credit, Money};

use crate::card::{Margin, RateCard};
use crate::error::PricingError;
use crate::usage::Usage;

/// Credits are stored as `numeric(30,5)`; round the final credit amount exactly once, here.
const CREDIT_SCALE: u32 = 5;

/// The fully priced result of a usage event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedUsage {
    /// Provider cost of goods sold.
    pub cogs: Money,
    /// What the customer is charged (margin applied).
    pub customer_price: Money,
    /// Credits to debit (rounded once).
    pub credits: Credit,
}

/// Cost of usage against a card: the sum of quantity × unit price across the event's dimensions.
pub fn cost(usage: &Usage, card: &RateCard) -> Result<Money, PricingError> {
    let mut total = Money::zero(card.currency.clone());
    for (dimension, quantity) in &usage.quantities {
        let component = card
            .component(*dimension, usage.modality, usage.context_tier)
            .ok_or(PricingError::NoComponent(*dimension))?;
        let line = component.charge(*quantity)?;
        total = total
            .try_add(&line)
            .map_err(|_| PricingError::CurrencyMismatch)?;
    }
    Ok(total)
}

/// Apply a margin multiplier to a cost.
#[must_use]
pub fn apply_margin(cost: &Money, margin: Margin) -> Money {
    cost.scale_by(margin.multiplier())
}

/// Convert a money amount into credits given the cash value of one credit (same currency).
pub fn to_credits(amount: &Money, credit_value: &Money) -> Result<Credit, PricingError> {
    if amount.currency() != credit_value.currency() {
        return Err(PricingError::CurrencyMismatch);
    }
    if credit_value.amount().is_zero() || credit_value.amount().is_sign_negative() {
        return Err(PricingError::NonPositiveCreditValue);
    }
    let credits = (amount.amount() / credit_value.amount()).round_dp(CREDIT_SCALE);
    Ok(Credit::from_decimal(credits))
}

/// Price a usage event end to end against a provider-cost card: COGS, customer price (margin applied),
/// and the credits to charge (rounded once at the credit layer).
pub fn price_usage(
    usage: &Usage,
    card: &RateCard,
    credit_value: &Money,
) -> Result<PricedUsage, PricingError> {
    let cogs = cost(usage, card)?;
    let customer_price = apply_margin(&cogs, card.margin);
    let credits = to_credits(&customer_price, credit_value)?;
    Ok(PricedUsage {
        cogs,
        customer_price,
        credits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Margin, RateCard, RateCardKind};
    use crate::component::{ChargeModel, PriceComponent};
    use crate::dimension::{ContextTier, Modality, PricingDimension, Unit};
    use meter_core::{Currency, RateCardId};
    use rust_decimal_macros::dec;

    fn usd() -> Currency {
        Currency::new("USD").expect("valid currency")
    }

    fn anthropic_like_card() -> RateCard {
        let token_component = |dimension, price| PriceComponent {
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
            margin: Margin::from_multiplier(dec!(1.30)),
            components: vec![
                token_component(PricingDimension::InputUncached, dec!(0.000003)), // $3 / 1M
                token_component(PricingDimension::Output, dec!(0.000015)),        // $15 / 1M
            ],
        }
    }

    fn sample_usage() -> Usage {
        Usage::new(Modality::Text, ContextTier::Standard)
            .with(PricingDimension::InputUncached, dec!(1000))
            .with(PricingDimension::Output, dec!(500))
    }

    #[test]
    fn sums_dimension_costs() {
        // 1000 * 0.000003 + 500 * 0.000015 = 0.003 + 0.0075 = 0.0105
        assert_eq!(
            cost(&sample_usage(), &anthropic_like_card())
                .unwrap()
                .amount(),
            dec!(0.0105)
        );
    }

    #[test]
    fn prices_end_to_end_with_margin_and_credit_value() {
        let credit_value = Money::new(dec!(0.02), usd()); // 1 credit = 2 cents
        let priced = price_usage(&sample_usage(), &anthropic_like_card(), &credit_value).unwrap();
        assert_eq!(priced.cogs.amount(), dec!(0.0105));
        assert_eq!(priced.customer_price.amount(), dec!(0.01365)); // 0.0105 * 1.30
        assert_eq!(priced.credits.value(), dec!(0.6825)); // 0.01365 / 0.02
    }

    #[test]
    fn charges_per_action_for_tool_calls_and_web_searches() {
        // action_charge (EPIC 04): non-token dimensions billed per call via `Unit::Call`.
        let action = |dimension, price| PriceComponent {
            dimension,
            modality: Modality::None,
            context_tier: ContextTier::Standard,
            unit: Unit::Call,
            charge_model: ChargeModel::Standard,
            unit_price: Money::new(price, usd()),
        };
        let card = RateCard {
            id: RateCardId::new(),
            kind: RateCardKind::ProviderCost,
            currency: usd(),
            version: 1,
            margin: Margin::NONE,
            components: vec![
                action(PricingDimension::ToolCall, dec!(0.01)), // $0.01 per tool call
                action(PricingDimension::WebSearch, dec!(0.02)), // $0.02 per web search
            ],
        };
        let usage = Usage::new(Modality::None, ContextTier::Standard)
            .with(PricingDimension::ToolCall, dec!(3))
            .with(PricingDimension::WebSearch, dec!(2));
        // 3 * 0.01 + 2 * 0.02 = 0.07; priced 1:1 to credits at a 1-cent credit value.
        assert_eq!(cost(&usage, &card).unwrap().amount(), dec!(0.07));
        let priced = price_usage(&usage, &card, &Money::new(dec!(0.01), usd())).unwrap();
        assert_eq!(priced.credits.value(), dec!(7));
    }

    #[test]
    fn missing_component_is_an_error() {
        let usage = Usage::new(Modality::Text, ContextTier::Standard)
            .with(PricingDimension::CacheRead, dec!(10));
        assert!(matches!(
            cost(&usage, &anthropic_like_card()),
            Err(PricingError::NoComponent(PricingDimension::CacheRead))
        ));
    }

    #[test]
    fn rejects_nonpositive_credit_value() {
        let amount = Money::new(dec!(1), usd());
        assert!(matches!(
            to_credits(&amount, &Money::new(dec!(0), usd())),
            Err(PricingError::NonPositiveCreditValue)
        ));
    }

    #[test]
    fn rejects_cross_currency_credit_value() {
        let amount = Money::new(dec!(1), usd());
        let eur_value = Money::new(dec!(0.02), Currency::new("EUR").unwrap());
        assert!(matches!(
            to_credits(&amount, &eur_value),
            Err(PricingError::CurrencyMismatch)
        ));
    }
}
