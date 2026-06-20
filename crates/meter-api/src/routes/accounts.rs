//! Account endpoints: open, balance, grant, entries.

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use meter_core::AccountId;
use meter_ledger::{Balance, GrantRequest, LedgerAccount, LedgerBackend, LedgerEntry, NewAccount};

use crate::dto::{GrantBody, OpenAccountBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/accounts`
pub async fn open_account(
    State(state): State<AppState>,
    Json(body): Json<OpenAccountBody>,
) -> Result<Json<LedgerAccount>, ApiError> {
    let account = state
        .ledger
        .open_account(NewAccount {
            org_id: body.org_id,
            scope: body.scope,
            no_overdraft: body.no_overdraft,
            parent_id: body.parent_id,
        })
        .await?;
    Ok(Json(account))
}

/// `GET /v1/accounts/{id}/balance`
pub async fn balance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Balance>, ApiError> {
    let balance = state.ledger.balance(AccountId::from_uuid(id)).await?;
    Ok(Json(balance))
}

/// `POST /v1/accounts/{id}/grants`
pub async fn grant(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<GrantBody>,
) -> Result<Json<LedgerEntry>, ApiError> {
    let entry = state
        .ledger
        .grant(GrantRequest {
            account: AccountId::from_uuid(id),
            amount: body.amount,
            source: body.source,
            idempotency_key: body.idempotency_key,
        })
        .await?;
    Ok(Json(entry))
}

/// `GET /v1/accounts/{id}/entries`
pub async fn entries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    let entries = state.ledger.entries(AccountId::from_uuid(id)).await?;
    Ok(Json(entries))
}
