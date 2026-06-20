//! The engine's gRPC surface (tonic), implementing the `meter.v1` services over the same stores as
//! the HTTP API. This module holds the shared proto<->domain conversions; one submodule per service.
//!
//! `tonic::Status` is a large error type, but the generated service traits require `Result<_, Status>`,
//! so `result_large_err` is unavoidable here and allowed at the module boundary.
#![allow(clippy::result_large_err)]

pub mod ingest;
pub mod ledger;
pub mod query;

use std::str::FromStr;

use rust_decimal::Decimal;
use tonic::Status;
use uuid::Uuid;

use meter_core::Credit;
use meter_event::EventError;
use meter_ledger::LedgerError;
use meter_proto::v1;
use meter_store_ch::ChError;

/// Parse a UUID-bearing string field, mapping a bad value to `invalid_argument`.
fn parse_uuid(value: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(value)
        .map_err(|_| Status::invalid_argument(format!("invalid {field}: {value}")))
}

/// Read a required proto [`Credit`](v1::Credit) into a domain [`Credit`].
fn credit_from_proto(credit: Option<&v1::Credit>, field: &str) -> Result<Credit, Status> {
    let credit = credit.ok_or_else(|| Status::invalid_argument(format!("missing {field}")))?;
    let amount = Decimal::from_str(&credit.amount)
        .map_err(|_| Status::invalid_argument(format!("invalid {field}: {}", credit.amount)))?;
    Ok(Credit::from_decimal(amount))
}

/// Render a domain [`Credit`] as its proto form (canonical decimal string).
fn credit_to_proto(credit: Credit) -> v1::Credit {
    v1::Credit {
        amount: credit.value().normalize().to_string(),
    }
}

/// Map a [`LedgerError`] to the closest gRPC status code.
fn status_from_ledger(error: &LedgerError) -> Status {
    match error {
        LedgerError::AccountNotFound(_) | LedgerError::ReservationNotFound(_) => {
            Status::not_found(error.to_string())
        }
        LedgerError::ReservationClosed(_) | LedgerError::NotALease(_) => {
            Status::failed_precondition(error.to_string())
        }
        LedgerError::NonPositiveAmount => Status::invalid_argument(error.to_string()),
        LedgerError::InsufficientFunds { .. } => Status::resource_exhausted(error.to_string()),
        LedgerError::Backend(_) => Status::internal(error.to_string()),
    }
}

/// Map an [`EventError`] to the closest gRPC status code.
fn status_from_event(error: &EventError) -> Status {
    match error {
        EventError::NotFound(_) => Status::not_found(error.to_string()),
        EventError::Voided(_) => Status::failed_precondition(error.to_string()),
        EventError::Backend(_) => Status::internal(error.to_string()),
    }
}

/// Map a ClickHouse [`ChError`] to a gRPC status (all are infrastructure failures).
fn status_from_ch(error: &ChError) -> Status {
    Status::internal(error.to_string())
}

/// Parse a required RFC3339 timestamp field.
fn parse_time(value: &str, field: &str) -> Result<time::OffsetDateTime, Status> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map_err(|_| Status::invalid_argument(format!("invalid {field}: {value}")))
}
