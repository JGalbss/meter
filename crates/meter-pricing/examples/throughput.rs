//! Aggregate throughput harness for the pricing hot path: how many realistic usage events meter can
//! price → credit per second on this machine, single-core and across all cores. Run with:
//!
//! ```bash
//! cargo run --release --example throughput -p meter-pricing
//! ```
//!
//! This measures the per-event CPU cost (`price_usage`), which is the work meter does on every metered
//! event. It is O(1) in ledger history and embarrassingly parallel, so aggregate throughput scales with
//! cores. The numbers are wall-clock measured here, not extrapolated from a microbenchmark.

use std::thread;
use std::time::Instant;

use meter_core::{Currency, Money, RateCardId};
use meter_pricing::{
    price_usage, ChargeModel, ContextTier, Margin, Modality, PriceComponent, PricingDimension,
    RateCard, RateCardKind, Unit, Usage,
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

/// Price `iterations` events on one thread; returns the number priced (kept honest against the
/// optimizer by accumulating the credit count).
fn run(iterations: u64) -> u64 {
    let card = flagship_card();
    let usage = realistic_usage();
    let credit_value = Money::new(dec!(0.000001), usd());
    let mut sink = 0u64;
    for _ in 0..iterations {
        let priced = price_usage(&usage, &card, &credit_value).expect("price");
        sink = sink.wrapping_add(u64::from(!priced.credits.is_zero()));
    }
    sink
}

fn rate_per_sec(iterations: u64, seconds: f64) -> f64 {
    iterations as f64 / seconds
}

fn report(label: &str, events: u64, seconds: f64) {
    let per_sec = rate_per_sec(events, seconds);
    let per_day = per_sec * 86_400.0;
    println!(
        "{label:<22} {events:>13} events in {seconds:>6.3}s  =>  {per_sec:>14.0}/s  ({:>6.2} B/day)",
        per_day / 1_000_000_000.0
    );
}

fn main() {
    let cores = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let per_core: u64 = 20_000_000;

    // Warm up (cache, branch predictor) so the first measurement isn't penalized.
    let _ = run(1_000_000);

    println!(
        "meter pricing throughput — price_usage (5-dimension event -> COGS -> margin -> credits)"
    );
    println!("cores available: {cores}\n");

    // Single core.
    let start = Instant::now();
    let s = run(per_core);
    let single_secs = start.elapsed().as_secs_f64();
    std::hint::black_box(s);
    report("single core", per_core, single_secs);

    // All cores.
    let total = per_core * cores as u64;
    let start = Instant::now();
    thread::scope(|scope| {
        let handles: Vec<_> = (0..cores)
            .map(|_| scope.spawn(move || run(per_core)))
            .collect();
        for handle in handles {
            std::hint::black_box(handle.join().expect("thread"));
        }
    });
    let all_secs = start.elapsed().as_secs_f64();
    report(&format!("all {cores} cores"), total, all_secs);

    let single_rate = rate_per_sec(per_core, single_secs);
    let all_rate = rate_per_sec(total, all_secs);
    println!(
        "\nper-core: {single_rate:.0}/s   |   aggregate: {all_rate:.0}/s   |   scaling: {:.1}x",
        all_rate / single_rate
    );
    println!(
        "aggregate per day: {:.1} billion  ({:.2} trillion)",
        all_rate * 86_400.0 / 1_000_000_000.0,
        all_rate * 86_400.0 / 1_000_000_000_000.0
    );
}
