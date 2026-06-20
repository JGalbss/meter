//! Account endpoints: open, balance, grant, entries.

use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

use meter_core::AccountId;
use meter_ledger::{
    Balance, GrantRequest, LedgerAccount, LedgerBackend, LedgerEntry, NewAccount, RefundRequest,
};

use crate::dto::{GrantBody, OpenAccountBody, RefundBody};
use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/accounts`
#[utoipa::path(
    post,
    path = "/v1/accounts",
    request_body = OpenAccountBody,
    responses((status = 200, description = "Account opened")),
    tag = "accounts"
)]
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
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/balance",
    params(("id" = String, Path, description = "Account id (UUID)")),
    responses((status = 200, description = "Account balance (settled / held)")),
    tag = "accounts"
)]
pub async fn balance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Balance>, ApiError> {
    let balance = state.ledger.balance(AccountId::from_uuid(id)).await?;
    Ok(Json(balance))
}

/// `POST /v1/accounts/{id}/grants`
#[utoipa::path(
    post,
    path = "/v1/accounts/{id}/grants",
    params(("id" = String, Path, description = "Account id (UUID)")),
    request_body = GrantBody,
    responses((status = 200, description = "Grant posted; returns the ledger entry")),
    tag = "accounts"
)]
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

/// `POST /v1/accounts/{id}/credit-notes` — credit an account back (a refund / correction).
#[utoipa::path(
    post,
    path = "/v1/accounts/{id}/credit-notes",
    params(("id" = String, Path, description = "Account id (UUID)")),
    request_body = RefundBody,
    responses((status = 200, description = "Refund posted; returns the ledger entry")),
    tag = "accounts"
)]
pub async fn credit_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RefundBody>,
) -> Result<Json<LedgerEntry>, ApiError> {
    let entry = state
        .ledger
        .refund(RefundRequest {
            account: AccountId::from_uuid(id),
            amount: body.amount,
            reverses_entry_id: body.reverses_entry_id,
            idempotency_key: body.idempotency_key,
        })
        .await?;
    Ok(Json(entry))
}

/// `GET /v1/accounts/{id}/entries`
#[utoipa::path(
    get,
    path = "/v1/accounts/{id}/entries",
    params(("id" = String, Path, description = "Account id (UUID)")),
    responses((status = 200, description = "The account's ledger entries (audit trail)")),
    tag = "accounts"
)]
pub async fn entries(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    let entries = state.ledger.entries(AccountId::from_uuid(id)).await?;
    Ok(Json(entries))
}
