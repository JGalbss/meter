# meter-pricing

Rate cards and the pricing math: it turns a metered usage event into a provider cost, a customer price,
and the credits to charge. Pure functions over value types — no ledger, no store, no IO.

## What's inside

| Item | What it is |
|---|---|
| `RateCard` | A versioned card (`kind` ∈ `ProviderCost` / `Customer`) holding a `Margin` and a set of `PriceComponent`s. |
| `PriceComponent` | One priced line: a `PricingDimension` × `Modality` × `ContextTier`, billed in some `Unit` under a `ChargeModel` (`Standard`, `Graduated`, `Volume`, `Package`). |
| `Usage` | The metered quantities of a single event, built fluently with `Usage::with(dimension, qty)`. |
| `PricedUsage` | The result: `cogs`, `customer_price`, and `credits`. |
| `price_usage` | The end-to-end pipeline. Also exposed in pieces: `cost`, `apply_margin`, `to_credits`. |
| `rerate_event` / `simulate_rerate` | Re-rate historical usage against a proposed card to preview a pricing change (`RerateLine`, `RerateSummary`). |

The pipeline is `cost` (COGS = Σ qty × unit price) → `apply_margin` (× the card's multiplier) →
`to_credits` (customer price ÷ credit value). Money stays exact the whole way; the result is rounded
**exactly once**, at the credit layer, to `CREDIT_SCALE = 5` decimal places. A usage dimension with no
matching component is a `PricingError::NoComponent`, never a silent zero.

## Where it sits

Builds on `meter-core` for `Money`, `Credit`, and the typed ids. `meter-enforcement`, `meter-api`, and
`meter-ratecards` build on it.

Edition 2021, `#![forbid(unsafe_code)]`. The COGS → margin → credits invariants are property-tested:

```bash
cargo test -p meter-pricing
cargo bench -p meter-pricing      # pricing hot path, no external dependencies
```
