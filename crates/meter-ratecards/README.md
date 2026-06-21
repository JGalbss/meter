# meter-ratecards

A curated, dated catalog of provider model rate cards — the default pricing source. Build on
it instead of maintaining provider prices yourself, or supply your own rate card. Prices are USD per
token, as `rust_decimal::Decimal`, never floats.

## What's inside

- `catalog() -> Vec<ModelCatalogEntry>` — the curated set: 11 flagship models across Anthropic, OpenAI,
  Google, DeepSeek, and Alibaba, each with standard-tier text prices for the four token dimensions
  (input, cache read, cache write, output).
- `ModelCatalogEntry` — one model's prices. `provider_cost_card()` builds a provider-cost `RateCard`
  (USD, no margin, four `Standard`-charge components) ready for the pricing pipeline.
- `CATALOG_AS_OF` — the snapshot date, `"2026-06"`. Treat prices as best-effort as of this date.
- `find(model_id)` / `rate_card_for(model_id)` — look up an entry, or build its rate card directly, by
  model id.

## Accuracy

This is a best-effort snapshot with no billing-accuracy SLA. Verify against the provider before
billing, or supply your own rate card. Where a provider has no distinct cache-write price (caching is
automatic at the input rate), `cache_write_per_token` equals `input_per_token` — a best-effort mapping
onto meter's per-dimension model.

## Where it sits

Depends on `meter-core` (Money/Currency/ids) and `meter-pricing` (`RateCard` and the charge model).
`meter-api` serves the catalog at `GET /v1/catalog`; the hosted scraper keeps it current and extends
coverage to more providers.
