//! Budget / alert status: usage in a period against a limit, with threshold classification.

use std::str::FromStr;

use axum::extract::{Path, Query, State};
use axum::Json;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use meter_core::AccountId;
use meter_store_pg::PgConfig;

use crate::error::ApiError;
use crate::AppState;

/// `?start=<rfc3339>&end=<rfc3339>[&limit=<credits>]`
#[derive(Debug, Deserialize)]
pub struct BudgetQuery {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    /// Optional override; when absent the account's configured budget is used.
    #[serde(default)]
    pub limit: Option<String>,
}

/// Usage in a period against a limit, with threshold classification. Credit amounts are exact decimal
/// strings.
#[derive(Debug, Serialize, ToSchema)]
pub struct BudgetStatusResponse {
    pub used_credits: String,
    pub limit_credits: String,
    pub remaining_credits: String,
    /// Used / limit, rounded to 4 dp.
    pub ratio: String,
    /// `ok` | `warning` (>=80%) | `exceeded` (>=100%).
    pub status: String,
}

const WARNING_RATIO: (i64, u32) = (8, 1); // 0.8

/// Classify usage against a limit: `exceeded` at >=100%, `warning` at >=80%, else `ok`.
fn classify(used: Decimal, limit: Decimal) -> (Decimal, &'static str) {
    if limit.is_zero() || limit.is_sign_negative() {
        return (Decimal::ZERO, "ok");
    }
    let ratio = (used / limit).round_dp(4);
    if used >= limit {
        return (ratio, "exceeded");
    }
    let warning_threshold = limit * Decimal::new(WARNING_RATIO.0, WARNING_RATIO.1);
    if used >= warning_threshold {
        return (ratio, "warning");
    }
    (ratio, "ok")
}

/// `GET /v1/accounts/{id}/budget?start&end&limit`
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/budget",
    params(
        ("id" = String, Path, description = "Account id (UUID)"),
        ("start" = String, Query, description = "Period start (RFC3339)"),
        ("end" = String, Query, description = "Period end (RFC3339)"),
        ("limit" = Option<String>, Query, description = "Credit limit override (decimal string)")
    ),
    responses((status = 200, description = "Usage vs limit with threshold status", body = BudgetStatusResponse)),
    tag = "analytics"
)]
pub async fn budget_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<BudgetQuery>,
) -> Result<Json<BudgetStatusResponse>, ApiError> {
    let account = AccountId::from_uuid(id);
    // Use the explicit limit if given, else the account's configured budget.
    let limit = match query.limit {
        Some(value) => {
            Decimal::from_str(&value).map_err(|_| ApiError::unprocessable("invalid limit"))?
        }
        None => PgConfig::new(state.ledger.pool().clone())
            .budget(id)
            .await?
            .map(|budget| budget.limit_credits)
            .ok_or_else(|| {
                ApiError::unprocessable("limit required: no budget configured for this account")
            })?,
    };
    let usage = state
        .ledger
        .period_usage(account, query.start, query.end)
        .await?;
    let used = usage.total_credits.value();
    let (ratio, status) = classify(used, limit);
    let remaining = limit - used;
    Ok(Json(BudgetStatusResponse {
        used_credits: used.normalize().to_string(),
        limit_credits: limit.normalize().to_string(),
        remaining_credits: remaining.normalize().to_string(),
        ratio: ratio.normalize().to_string(),
        status: status.to_owned(),
    }))
}
