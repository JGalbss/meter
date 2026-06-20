//! Invoicing: a deterministic statement summed from the ledger for a period.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use meter_core::{AccountId, Credit};

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

/// A period invoice: total credits and the entry count, summed from the ledger (enforced == billed).
#[derive(Debug, Serialize, ToSchema)]
pub struct InvoiceResponse {
    #[schema(value_type = String, format = "uuid")]
    pub account_id: AccountId,
    /// Total credits consumed over the period, as an exact decimal string.
    #[schema(value_type = String)]
    pub total_credits: Credit,
    /// Number of spend postings in the period.
    pub entries: i64,
}

/// `GET /v1/accounts/{id}/invoice?start=..&end=..`
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/invoice",
    params(
        ("id" = String, Path, description = "Account id (UUID)"),
        ("start" = String, Query, description = "Period start (RFC3339)"),
        ("end" = String, Query, description = "Period end (RFC3339)")
    ),
    responses((status = 200, description = "Invoice summed from the ledger over the period", body = InvoiceResponse)),
    tag = "analytics"
)]
pub async fn invoice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(period): Query<Period>,
) -> Result<Json<InvoiceResponse>, ApiError> {
    let account = AccountId::from_uuid(id);
    let usage = state
        .ledger
        .period_usage(account, period.start, period.end)
        .await?;
    Ok(Json(InvoiceResponse {
        account_id: account,
        total_credits: usage.total_credits,
        entries: usage.entry_count,
    }))
}
