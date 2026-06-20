//! Invoicing: a deterministic statement summed from the ledger for a period.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::AccountId;

use crate::error::ApiError;
use crate::AppState;

/// `?start=<rfc3339>&end=<rfc3339>`
#[derive(Debug, Deserialize)]
pub struct Period {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
}

/// `GET /v1/accounts/{id}/invoice?start=..&end=..`
pub async fn invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(period): Query<Period>,
) -> Result<Json<Value>, ApiError> {
    let account = AccountId::from_uuid(id);
    let usage = state
        .ledger
        .period_usage(account, period.start, period.end)
        .await?;
    Ok(Json(json!({
        "account_id": account,
        "total_credits": usage.total_credits,
        "settle_count": usage.settle_count,
    })))
}
