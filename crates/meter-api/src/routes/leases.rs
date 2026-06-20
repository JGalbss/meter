//! Lease endpoints: open a per-session child account funded from a parent, and close it to return
//! the unused remainder. Credits are conserved end to end (ADR 0003 leasing).

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use meter_core::AccountId;
use meter_ledger::{LeaseRequest, LedgerAccount, LedgerBackend};

use crate::dto::OpenLeaseBody;
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/leases`
pub async fn open_lease(
    State(state): State<AppState>,
    Json(body): Json<OpenLeaseBody>,
) -> Result<Json<LedgerAccount>, ApiError> {
    let lease = state
        .ledger
        .open_lease(LeaseRequest {
            parent: body.parent,
            amount: body.amount,
        })
        .await?;
    Ok(Json(lease))
}

/// `POST /v1/leases/{id}/close`
pub async fn close_lease(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let returned = state.ledger.close_lease(AccountId::from_uuid(id)).await?;
    Ok(Json(json!({
        "returned": returned.value().normalize().to_string(),
    })))
}
