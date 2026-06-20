//! gRPC `IngestService` backed by the ClickHouse event store — the same ingest path as HTTP.
//!
//! `Result<_, tonic::Status>` is imposed by the generated service trait; see the module docs.
#![allow(clippy::result_large_err)]

use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tonic::{Request, Response, Status};

use meter_core::{AccountId, EventId, OrgId, RunId};
use meter_event::{AmendEvent, EventStore, RecordEvent};
use meter_proto::v1;
use meter_store_ch::ChStore;

use super::{parse_uuid, status_from_event};

/// The gRPC ingest service over a ClickHouse event store.
pub struct IngestGrpc {
    events: ChStore,
}

impl IngestGrpc {
    /// Build the service over an event store.
    #[must_use]
    pub const fn new(events: ChStore) -> Self {
        Self { events }
    }
}

/// Parse the JSON `properties` string (empty means null).
fn properties(raw: &str) -> Result<Value, Status> {
    match raw.is_empty() {
        true => Ok(Value::Null),
        false => serde_json::from_str(raw)
            .map_err(|error| Status::invalid_argument(format!("invalid properties JSON: {error}"))),
    }
}

/// Convert a proto record request into a store [`RecordEvent`] (empty `event_time` means now).
fn record_event(req: v1::RecordEventRequest) -> Result<RecordEvent, Status> {
    let event_time = match req.event_time.is_empty() {
        true => OffsetDateTime::now_utc(),
        false => OffsetDateTime::parse(&req.event_time, &Rfc3339).map_err(|_| {
            Status::invalid_argument(format!("invalid event_time: {}", req.event_time))
        })?,
    };
    let run_id = match req.run_id.is_empty() {
        true => None,
        false => Some(RunId::from_uuid(parse_uuid(&req.run_id, "run_id")?)),
    };
    Ok(RecordEvent {
        org_id: OrgId::from_uuid(parse_uuid(&req.org_id, "org_id")?),
        idempotency_key: req.idempotency_key,
        event_time,
        meter: req.meter,
        account_id: AccountId::from_uuid(parse_uuid(&req.account_id, "account_id")?),
        run_id,
        properties: properties(&req.properties)?,
    })
}

#[tonic::async_trait]
impl v1::ingest_service_server::IngestService for IngestGrpc {
    async fn record_event(
        &self,
        request: Request<v1::RecordEventRequest>,
    ) -> Result<Response<v1::RecordEventResponse>, Status> {
        let event = self
            .events
            .record(record_event(request.into_inner())?)
            .await
            .map_err(|error| status_from_event(&error))?;
        Ok(Response::new(v1::RecordEventResponse {
            event_id: event.id.to_string(),
        }))
    }

    async fn record_batch(
        &self,
        request: Request<v1::RecordBatchRequest>,
    ) -> Result<Response<v1::RecordBatchResponse>, Status> {
        let reqs = request
            .into_inner()
            .events
            .into_iter()
            .map(record_event)
            .collect::<Result<Vec<_>, _>>()?;
        let recorded = self
            .events
            .record_batch(reqs)
            .await
            .map_err(|error| status_from_event(&error))?;
        Ok(Response::new(v1::RecordBatchResponse {
            accepted: recorded.len() as u64,
        }))
    }

    async fn amend_event(
        &self,
        request: Request<v1::AmendEventRequest>,
    ) -> Result<Response<v1::AmendEventResponse>, Status> {
        let req = request.into_inner();
        let amended = self
            .events
            .amend(AmendEvent {
                event_id: EventId::from_uuid(parse_uuid(&req.event_id, "event_id")?),
                properties: properties(&req.properties)?,
            })
            .await
            .map_err(|error| status_from_event(&error))?;
        Ok(Response::new(v1::AmendEventResponse {
            event_id: amended.id.to_string(),
        }))
    }

    async fn void_run(
        &self,
        request: Request<v1::VoidRunRequest>,
    ) -> Result<Response<v1::VoidRunResponse>, Status> {
        let req = request.into_inner();
        let voided = self
            .events
            .void_run(RunId::from_uuid(parse_uuid(&req.run_id, "run_id")?))
            .await
            .map_err(|error| status_from_event(&error))?;
        Ok(Response::new(v1::VoidRunResponse { voided }))
    }
}
