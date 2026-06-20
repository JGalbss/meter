//! Analytics: read-only usage rollups over the ledger (the authoritative Postgres data).

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::AccountId;
use meter_store_pg::DayUsage;

use crate::error::ApiError;
use crate::AppState;

/// `?start=<rfc3339>&end=<rfc3339>`
#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
}

/// `GET /v1/accounts/{id}/usage-by-day?start&end` — daily credit usage time series.
pub async fn usage_by_day(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<PeriodQuery>,
) -> Result<Json<Vec<DayUsage>>, ApiError> {
    let account = AccountId::from_uuid(id);
    let days = state
        .ledger
        .usage_by_day(account, query.start, query.end)
        .await?;
    Ok(Json(days))
}
