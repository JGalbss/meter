//! A curated, versioned catalog of model-provider rate cards.
//!
//! This is the "batteries-included" pricing source: customers can build on it instead of maintaining
//! provider prices themselves. It is a **best-effort snapshot** (see [`CATALOG_AS_OF`]); the hosted
//! scraper keeps it current and extends it to more providers. Prices are USD **per token**.
//!
//! Per the architecture's open question on catalog accuracy, this carries no billing-accuracy SLA —
//! verify against the provider before billing, or supply your own rate card.

#![forbid(unsafe_code)]

use meter_core::{Currency, Money, RateCardId};
use meter_pricing::{
    ChargeModel, ContextTier, Margin, Modality, PriceComponent, PricingDimension, RateCard,
    RateCardKind, Unit,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;

/// The snapshot this catalog reflects. Treat prices as best-effort as of this date.
pub const CATALOG_AS_OF: &str = "2026-06";

/// One model's standard-tier text prices (USD per token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelCatalogEntry {
    pub provider: &'static str,
    pub model_id: &'static str,
    #[serde(with = "rust_decimal::serde::str")]
    pub input_per_token: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_read_per_token: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_write_per_token: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub output_per_token: Decimal,
}

impl ModelCatalogEntry {
    /// Build a provider-cost rate card (USD, no margin) from this entry.
    #[must_use]
    pub fn provider_cost_card(&self) -> RateCard {
        let usd = Currency::new("USD").expect("USD is a valid currency");
        let component = |dimension, price| PriceComponent {
            dimension,
            modality: Modality::Text,
            context_tier: ContextTier::Standard,
            unit: Unit::Token,
            charge_model: ChargeModel::Standard,
            unit_price: Money::new(price, usd.clone()),
        };
        RateCard {
            id: RateCardId::new(),
            kind: RateCardKind::ProviderCost,
            currency: usd.clone(),
            version: 1,
            margin: Margin::NONE,
            components: vec![
                component(PricingDimension::InputUncached, self.input_per_token),
                component(PricingDimension::CacheRead, self.cache_read_per_token),
                component(PricingDimension::CacheWrite, self.cache_write_per_token),
                component(PricingDimension::Output, self.output_per_token),
            ],
        }
    }
}

/// The curated catalog. Flagship models from the major providers are seeded with established
/// standard-context price points (USD per token); coverage is extended by the hosted scraper.
///
/// Where a provider has no distinct cache-write price (prompt caching is automatic, charged at the
/// standard input rate), `cache_write_per_token` equals `input_per_token` — a best-effort mapping
/// onto meter's per-dimension model. Prices are best-effort as of [`CATALOG_AS_OF`]; verify against
/// the provider before billing.
#[must_use]
pub fn catalog() -> Vec<ModelCatalogEntry> {
    vec![
        // --- Anthropic (explicit 5-min cache-write surcharge of 1.25x input). ---
        ModelCatalogEntry {
            provider: "anthropic",
            model_id: "claude-opus-4-8",
            input_per_token: dec!(0.000015),
            cache_read_per_token: dec!(0.0000015),
            cache_write_per_token: dec!(0.00001875),
            output_per_token: dec!(0.000075),
        },
        ModelCatalogEntry {
            provider: "anthropic",
            model_id: "claude-sonnet-4-6",
            input_per_token: dec!(0.000003),
            cache_read_per_token: dec!(0.0000003),
            cache_write_per_token: dec!(0.00000375),
            output_per_token: dec!(0.000015),
        },
        ModelCatalogEntry {
            provider: "anthropic",
            model_id: "claude-haiku-4-5",
            input_per_token: dec!(0.000001),
            cache_read_per_token: dec!(0.0000001),
            cache_write_per_token: dec!(0.00000125),
            output_per_token: dec!(0.000005),
        },
        // --- OpenAI (cached input billed at the discounted read rate; no separate write charge). ---
        ModelCatalogEntry {
            provider: "openai",
            model_id: "gpt-5",
            input_per_token: dec!(0.00000125),
            cache_read_per_token: dec!(0.000000125),
            cache_write_per_token: dec!(0.00000125),
            output_per_token: dec!(0.00001),
        },
        ModelCatalogEntry {
            provider: "openai",
            model_id: "gpt-5-mini",
            input_per_token: dec!(0.00000025),
            cache_read_per_token: dec!(0.000000025),
            cache_write_per_token: dec!(0.00000025),
            output_per_token: dec!(0.000002),
        },
        // --- Google (Gemini; standard <=200k context tier; cached billed at the read rate). ---
        ModelCatalogEntry {
            provider: "google",
            model_id: "gemini-2.5-pro",
            input_per_token: dec!(0.00000125),
            cache_read_per_token: dec!(0.00000031),
            cache_write_per_token: dec!(0.00000125),
            output_per_token: dec!(0.00001),
        },
        ModelCatalogEntry {
            provider: "google",
            model_id: "gemini-2.5-flash",
            input_per_token: dec!(0.0000003),
            cache_read_per_token: dec!(0.000000075),
            cache_write_per_token: dec!(0.0000003),
            output_per_token: dec!(0.0000025),
        },
    ]
}

/// Find a catalog entry by model id.
#[must_use]
pub fn find(model_id: &str) -> Option<ModelCatalogEntry> {
    catalog()
        .into_iter()
        .find(|entry| entry.model_id == model_id)
}

/// Build a provider-cost rate card for a known model id.
#[must_use]
pub fn rate_card_for(model_id: &str) -> Option<RateCard> {
    find(model_id).map(|entry| entry.provider_cost_card())
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_pricing::{cost, Usage};
    use rust_decimal_macros::dec;

    #[test]
    fn catalog_is_populated() {
        assert!(!catalog().is_empty());
        assert!(find("claude-opus-4-8").is_some());
        assert!(find("nonexistent-model").is_none());
    }

    #[test]
    fn covers_the_major_providers() {
        for model in ["claude-opus-4-8", "gpt-5", "gemini-2.5-pro"] {
            assert!(find(model).is_some(), "{model} should be in the catalog");
        }
        let providers: std::collections::BTreeSet<_> =
            catalog().iter().map(|entry| entry.provider).collect();
        assert!(providers.contains("anthropic"));
        assert!(providers.contains("openai"));
        assert!(providers.contains("google"));
    }

    #[test]
    fn every_entry_builds_a_four_component_card() {
        for entry in catalog() {
            let card = entry.provider_cost_card();
            assert_eq!(card.components.len(), 4, "{} components", entry.model_id);
            assert_eq!(card.kind, RateCardKind::ProviderCost);
            assert_eq!(card.margin, Margin::NONE);
        }
    }

    #[test]
    fn prices_openai_and_google_models() {
        // gpt-5: 1000 input @ $1.25/M + 500 output @ $10/M = 0.00125 + 0.005 = 0.00625
        let gpt5 = rate_card_for("gpt-5").expect("gpt-5 in catalog");
        let usage = Usage::new(Modality::Text, ContextTier::Standard)
            .with(PricingDimension::InputUncached, dec!(1000))
            .with(PricingDimension::Output, dec!(500));
        assert_eq!(cost(&usage, &gpt5).expect("priced").amount(), dec!(0.00625));

        // gemini-2.5-flash: 1000 input @ $0.30/M + 500 output @ $2.50/M = 0.0003 + 0.00125 = 0.00155
        let flash = rate_card_for("gemini-2.5-flash").expect("gemini flash in catalog");
        assert_eq!(
            cost(&usage, &flash).expect("priced").amount(),
            dec!(0.00155)
        );
    }

    #[test]
    fn builds_a_priced_card_for_a_known_model() {
        let card = rate_card_for("claude-opus-4-8").expect("opus in catalog");
        assert_eq!(card.components.len(), 4);
        // 1000 input + 500 output at $15/$75 per M = 0.015 + 0.0375 = 0.0525
        let usage = Usage::new(Modality::Text, ContextTier::Standard)
            .with(PricingDimension::InputUncached, dec!(1000))
            .with(PricingDimension::Output, dec!(500));
        assert_eq!(cost(&usage, &card).expect("priced").amount(), dec!(0.0525));
    }

    #[test]
    fn unknown_model_has_no_card() {
        assert!(rate_card_for("gpt-does-not-exist").is_none());
    }
}
