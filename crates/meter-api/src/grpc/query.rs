//! gRPC `QueryService`: read-side analytics (ClickHouse) and billing (Postgres ledger).
//!
//! `Result<_, tonic::Status>` is imposed by the generated service trait; see the module docs.
#![allow(clippy::result_large_err)]

use tonic::{Request, Response, Status};

use meter_core::AccountId;
use meter_proto::v1;
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;

use super::{credit_to_proto, parse_time, parse_uuid, status_from_ch, status_from_ledger};

/// The gRPC query service over the analytics store and the ledger.
pub struct QueryGrpc {
    events: ChStore,
    ledger: PgLedger,
}

impl QueryGrpc {
    /// Build the service over the event analytics store and the ledger.
    #[must_use]
    pub const fn new(events: ChStore, ledger: PgLedger) -> Self {
        Self { events, ledger }
    }
}

#[tonic::async_trait]
impl v1::query_service_server::QueryService for QueryGrpc {
    async fn usage_by_model(
        &self,
        request: Request<v1::UsageByModelRequest>,
    ) -> Result<Response<v1::UsageByModelResponse>, Status> {
        let org = parse_uuid(&request.into_inner().org_id, "org_id")?;
        let rows = self
            .events
            .usage_by_model(org)
            .await
            .map_err(|error| status_from_ch(&error))?;
        let models = rows
            .into_iter()
            .map(|row| v1::ModelUsage {
                model: row.model,
                events: row.events,
                input_tokens: row.input_tokens,
                output_tokens: row.output_tokens,
                credits: row.credits.to_string(),
            })
            .collect();
        Ok(Response::new(v1::UsageByModelResponse { models }))
    }

    async fn usage_by_day(
        &self,
        request: Request<v1::UsageByDayRequest>,
    ) -> Result<Response<v1::UsageByDayResponse>, Status> {
        let org = parse_uuid(&request.into_inner().org_id, "org_id")?;
        let rows = self
            .events
            .usage_by_day(org)
            .await
            .map_err(|error| status_from_ch(&error))?;
        let days = rows
            .into_iter()
            .map(|row| v1::DayUsage {
                day: row.day,
                events: row.events,
                credits: row.credits.to_string(),
            })
            .collect();
        Ok(Response::new(v1::UsageByDayResponse { days }))
    }

    async fn event_count(
        &self,
        request: Request<v1::EventCountRequest>,
    ) -> Result<Response<v1::EventCountResponse>, Status> {
        let org = parse_uuid(&request.into_inner().org_id, "org_id")?;
        let count = self
            .events
            .event_count(org)
            .await
            .map_err(|error| status_from_ch(&error))?;
        Ok(Response::new(v1::EventCountResponse { count }))
    }

    async fn invoice(
        &self,
        request: Request<v1::InvoiceRequest>,
    ) -> Result<Response<v1::InvoiceResponse>, Status> {
        let req = request.into_inner();
        let account = AccountId::from_uuid(parse_uuid(&req.account_id, "account_id")?);
        let start = parse_time(&req.start, "start")?;
        let end = parse_time(&req.end, "end")?;
        let usage = self
            .ledger
            .period_usage(account, start, end)
            .await
            .map_err(|error| status_from_ledger(&error))?;
        Ok(Response::new(v1::InvoiceResponse {
            total_credits: Some(credit_to_proto(usage.total_credits)),
            entries: u64::try_from(usage.entry_count).unwrap_or(0),
        }))
    }
}
