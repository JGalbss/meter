//! gRPC `LedgerService` backed by the Postgres ledger — the same money-truth the HTTP API uses.
//!
//! `Result<_, tonic::Status>` is imposed by the generated service trait; see the module docs.
#![allow(clippy::result_large_err)]

use tonic::{Request, Response, Status};

use meter_core::{AccountId, OrgId};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveOutcome, ReserveRequest, SettleRequest,
};
use meter_proto::v1;
use meter_store_pg::PgLedger;

use super::{credit_from_proto, credit_to_proto, parse_uuid, status_from_ledger};

/// The gRPC ledger service over a Postgres-backed ledger.
pub struct LedgerGrpc {
    ledger: PgLedger,
}

impl LedgerGrpc {
    /// Build the service over a ledger backend.
    #[must_use]
    pub const fn new(ledger: PgLedger) -> Self {
        Self { ledger }
    }
}

fn account_scope(scope: i32) -> Result<AccountScope, Status> {
    match v1::AccountScope::try_from(scope) {
        Ok(v1::AccountScope::Org) => Ok(AccountScope::Org),
        Ok(v1::AccountScope::Team) => Ok(AccountScope::Team),
        Ok(v1::AccountScope::User) => Ok(AccountScope::User),
        Ok(v1::AccountScope::Product) => Ok(AccountScope::Product),
        Ok(v1::AccountScope::Session) => Ok(AccountScope::Session),
        Ok(v1::AccountScope::Promo) => Ok(AccountScope::Promo),
        Ok(v1::AccountScope::Paid) => Ok(AccountScope::Paid),
        Ok(v1::AccountScope::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("scope is required"))
        }
    }
}

fn credit_source(source: i32) -> Result<CreditSource, Status> {
    match v1::CreditSource::try_from(source) {
        Ok(v1::CreditSource::Paid) => Ok(CreditSource::Paid),
        Ok(v1::CreditSource::Promo) => Ok(CreditSource::Promo),
        Ok(v1::CreditSource::Grant) => Ok(CreditSource::Grant),
        Ok(v1::CreditSource::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("source is required"))
        }
    }
}

fn limit_class(limit: i32) -> Result<LimitClass, Status> {
    match v1::LimitClass::try_from(limit) {
        Ok(v1::LimitClass::Hard) => Ok(LimitClass::Hard),
        Ok(v1::LimitClass::Soft) => Ok(LimitClass::Soft),
        Ok(v1::LimitClass::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("limit is required"))
        }
    }
}

#[tonic::async_trait]
impl v1::ledger_service_server::LedgerService for LedgerGrpc {
    async fn open_account(
        &self,
        request: Request<v1::OpenAccountRequest>,
    ) -> Result<Response<v1::OpenAccountResponse>, Status> {
        let req = request.into_inner();
        let parent_id = match req.parent_id.is_empty() {
            true => None,
            false => Some(AccountId::from_uuid(parse_uuid(
                &req.parent_id,
                "parent_id",
            )?)),
        };
        let account = self
            .ledger
            .open_account(NewAccount {
                org_id: OrgId::from_uuid(parse_uuid(&req.org_id, "org_id")?),
                scope: account_scope(req.scope)?,
                no_overdraft: req.no_overdraft,
                parent_id,
            })
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::OpenAccountResponse {
            account_id: account.id.to_string(),
        }))
    }

    async fn grant(
        &self,
        request: Request<v1::GrantRequest>,
    ) -> Result<Response<v1::GrantResponse>, Status> {
        let req = request.into_inner();
        let account = AccountId::from_uuid(parse_uuid(&req.account_id, "account_id")?);
        self.ledger
            .grant(GrantRequest {
                account,
                amount: credit_from_proto(req.amount.as_ref(), "amount")?,
                source: credit_source(req.source)?,
                idempotency_key: (!req.idempotency_key.is_empty()).then_some(req.idempotency_key),
            })
            .await
            .map_err(|error| status_from_ledger(&error))?;
        let balance = self
            .ledger
            .balance(account)
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::GrantResponse {
            settled: Some(credit_to_proto(balance.settled)),
        }))
    }

    async fn reserve(
        &self,
        request: Request<v1::ReserveRequest>,
    ) -> Result<Response<v1::ReserveResponse>, Status> {
        let req = request.into_inner();
        let outcome = self
            .ledger
            .reserve(ReserveRequest {
                account: AccountId::from_uuid(parse_uuid(&req.account_id, "account_id")?),
                reservation_id: ReservationId::from_uuid(parse_uuid(
                    &req.reservation_id,
                    "reservation_id",
                )?),
                amount: credit_from_proto(req.amount.as_ref(), "amount")?,
                limit: limit_class(req.limit)?,
                expires_at: None,
            })
            .await
            .map_err(|error| status_from_ledger(&error))?;
        let response = match outcome {
            ReserveOutcome::Allowed { .. } => v1::ReserveResponse {
                allowed: true,
                available: None,
                requested: None,
            },
            ReserveOutcome::Denied {
                available,
                requested,
            } => v1::ReserveResponse {
                allowed: false,
                available: Some(credit_to_proto(available)),
                requested: Some(credit_to_proto(requested)),
            },
        };
        Ok(Response::new(response))
    }

    async fn settle(
        &self,
        request: Request<v1::SettleRequest>,
    ) -> Result<Response<v1::SettleResponse>, Status> {
        let req = request.into_inner();
        let entry = self
            .ledger
            .settle(SettleRequest {
                reservation_id: ReservationId::from_uuid(parse_uuid(
                    &req.reservation_id,
                    "reservation_id",
                )?),
                actual: credit_from_proto(req.actual.as_ref(), "actual")?,
            })
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::SettleResponse {
            balance_after: Some(credit_to_proto(entry.balance_after)),
        }))
    }

    async fn void(
        &self,
        request: Request<v1::VoidRequest>,
    ) -> Result<Response<v1::VoidResponse>, Status> {
        let req = request.into_inner();
        self.ledger
            .void(ReservationId::from_uuid(parse_uuid(
                &req.reservation_id,
                "reservation_id",
            )?))
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::VoidResponse {}))
    }

    async fn balance(
        &self,
        request: Request<v1::BalanceRequest>,
    ) -> Result<Response<v1::BalanceResponse>, Status> {
        let req = request.into_inner();
        let balance = self
            .ledger
            .balance(AccountId::from_uuid(parse_uuid(
                &req.account_id,
                "account_id",
            )?))
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::BalanceResponse {
            settled: Some(credit_to_proto(balance.settled)),
            held: Some(credit_to_proto(balance.held)),
        }))
    }
}
