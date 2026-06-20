//! Budget / alert status: usage in a period against a limit, with threshold classification.

use axum::extract::{Path, Query, State};
use axum::Json;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::AccountId;

use crate::error::ApiError;
use crate::AppState;

/// `?start=<rfc3339>&end=<rfc3339>&limit=<credits>`
#[derive(Debug, Deserialize)]
pub struct BudgetQuery {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit: Decimal,
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
pub async fn budget_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<BudgetQuery>,
) -> Result<Json<Value>, ApiError> {
    let account = AccountId::from_uuid(id);
    let usage = state
        .ledger
        .period_usage(account, query.start, query.end)
        .await?;
    let used = usage.total_credits.value();
    let (ratio, status) = classify(used, query.limit);
    let remaining = query.limit - used;
    Ok(Json(json!({
        "used_credits": used.normalize().to_string(),
        "limit_credits": query.limit.normalize().to_string(),
        "remaining_credits": remaining.normalize().to_string(),
        "ratio": ratio.normalize().to_string(),
        "status": status,
    })))
}
