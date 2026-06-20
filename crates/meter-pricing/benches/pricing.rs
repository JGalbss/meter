//! Microbenchmark for the per-event pricing hot path: token usage → COGS → margin → credits.
//! This runs on every metered event, so it is a representative latency-sensitive path.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use meter_core::{Currency, Money, RateCardId};
use meter_pricing::{
    cost, price_usage, ChargeModel, ContextTier, Margin, Modality, PriceComponent,
    PricingDimension, RateCard, RateCardKind, Unit, Usage,
};
use rust_decimal_macros::dec;

fn usd() -> Currency {
    Currency::new("USD").expect("valid currency")
}

fn flagship_card() -> RateCard {
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
        margin: Margin::from_multiplier(dec!(1.30)),
        components: vec![
            component(PricingDimension::InputUncached, dec!(0.000003)),
            component(PricingDimension::CacheRead, dec!(0.0000003)),
            component(PricingDimension::CacheWrite, dec!(0.00000375)),
            component(PricingDimension::Output, dec!(0.000015)),
            component(PricingDimension::ReasoningOutput, dec!(0.000015)),
        ],
    }
}

fn realistic_usage() -> Usage {
    Usage::new(Modality::Text, ContextTier::Standard)
        .with(PricingDimension::InputUncached, dec!(1200))
        .with(PricingDimension::CacheRead, dec!(800))
        .with(PricingDimension::CacheWrite, dec!(200))
        .with(PricingDimension::Output, dec!(640))
        .with(PricingDimension::ReasoningOutput, dec!(160))
}

fn bench_pricing(c: &mut Criterion) {
    let card = flagship_card();
    let usage = realistic_usage();
    let credit_value = Money::new(dec!(0.000001), usd()); // 1 credit = 1 micro-USD

    c.bench_function("cost_5_dimensions", |b| {
        b.iter(|| cost(black_box(&usage), black_box(&card)).expect("cost"))
    });

    c.bench_function("price_usage_end_to_end", |b| {
        b.iter(|| {
            price_usage(
                black_box(&usage),
                black_box(&card),
                black_box(&credit_value),
            )
            .expect("price")
        })
    });
}

criterion_group!(benches, bench_pricing);
criterion_main!(benches);
