//! gRPC `ConfigService`: the control plane syncs rate cards and budgets into the engine's store.
//!
//! `Result<_, tonic::Status>` is imposed by the generated service trait; see the module docs.
#![allow(clippy::result_large_err)]

use std::str::FromStr;

use rust_decimal::Decimal;
use serde_json::{json, Value};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use meter_proto::v1;
use meter_store_pg::{BudgetRecord, PgConfig, RateCardRecord};

use super::{parse_uuid, status_from_ledger};

/// The gRPC config service over the engine's Postgres config store.
pub struct ConfigGrpc {
    config: PgConfig,
}

impl ConfigGrpc {
    /// Build the service over a config store.
    #[must_use]
    pub const fn new(config: PgConfig) -> Self {
        Self { config }
    }
}

fn rate_card_kind(kind: i32) -> Result<String, Status> {
    match v1::RateCardKind::try_from(kind) {
        Ok(v1::RateCardKind::ProviderCost) => Ok("provider_cost".to_owned()),
        Ok(v1::RateCardKind::Customer) => Ok("customer".to_owned()),
        Ok(v1::RateCardKind::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("kind is required"))
        }
    }
}

/// Render the proto price components as the JSON the store keeps.
fn components_json(components: &[v1::PriceComponent]) -> Value {
    let cells: Vec<Value> = components
        .iter()
        .map(|component| {
            json!({
                "dimension": component.dimension,
                "modality": component.modality,
                "context_tier": component.context_tier,
                "unit": component.unit,
                "charge_model": component.charge_model,
                "unit_price": component.unit_price.as_ref().map(|price| json!({
                    "amount": price.amount,
                    "currency": price.currency,
                })),
            })
        })
        .collect();
    Value::Array(cells)
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, Status> {
    Decimal::from_str(value)
        .map_err(|_| Status::invalid_argument(format!("invalid {field}: {value}")))
}

#[tonic::async_trait]
impl v1::config_service_server::ConfigService for ConfigGrpc {
    async fn put_rate_card(
        &self,
        request: Request<v1::PutRateCardRequest>,
    ) -> Result<Response<v1::PutRateCardResponse>, Status> {
        let card = request
            .into_inner()
            .card
            .ok_or_else(|| Status::invalid_argument("card is required"))?;
        // A new card may arrive without an id; the engine assigns one.
        let id = match card.id.is_empty() {
            true => Uuid::now_v7(),
            false => parse_uuid(&card.id, "card.id")?,
        };
        let record = RateCardRecord {
            id,
            version: i32::try_from(card.version)
                .map_err(|_| Status::invalid_argument("version out of range"))?,
            kind: rate_card_kind(card.kind)?,
            currency: card.currency,
            margin: parse_decimal(&card.margin, "margin")?,
            components: components_json(&card.components),
        };
        self.config
            .put_rate_card(&record)
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::PutRateCardResponse {
            rate_card_id: id.to_string(),
        }))
    }

    async fn set_budget(
        &self,
        request: Request<v1::SetBudgetRequest>,
    ) -> Result<Response<v1::SetBudgetResponse>, Status> {
        let budget = request
            .into_inner()
            .budget
            .ok_or_else(|| Status::invalid_argument("budget is required"))?;
        let limit = budget
            .limit
            .ok_or_else(|| Status::invalid_argument("limit is required"))?;
        self.config
            .set_budget(&BudgetRecord {
                account_id: parse_uuid(&budget.account_id, "account_id")?,
                limit_credits: parse_decimal(&limit.amount, "limit")?,
                period: budget.period,
            })
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::SetBudgetResponse {}))
    }
}
