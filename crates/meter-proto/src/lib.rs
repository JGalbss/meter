//! Generated gRPC types + service stubs for the engine contract.
//!
//! The source of truth is `proto/` (a Buf module); `build.rs` runs tonic-build to generate the
//! `meter.v1` types, client stubs, and server traits at compile time. This crate just re-exports them.

// Generated code does not follow our lints; scope the allow to the generated module only.
#[allow(clippy::all, clippy::pedantic, missing_docs)]
pub mod v1 {
    tonic::include_proto!("meter.v1");
}

#[cfg(test)]
mod tests {
    use super::v1;

    #[test]
    fn generated_messages_and_enums_are_usable() {
        let req = v1::ReserveRequest {
            account_id: "acct".to_owned(),
            reservation_id: "res".to_owned(),
            amount: Some(v1::Credit {
                amount: "40".to_owned(),
            }),
            limit: v1::LimitClass::Hard as i32,
            expires_at: String::new(),
        };
        assert_eq!(req.amount.unwrap().amount, "40");
        assert_eq!(req.limit, v1::LimitClass::Hard as i32);
        assert_eq!(v1::AccountScope::Org as i32, 1);
    }

    // The contract generates both client stubs and server traits for every service.
    #[allow(dead_code)]
    fn service_types_exist(
        _ledger_client: Option<v1::ledger_service_client::LedgerServiceClient<()>>,
        _ingest_server: Option<v1::ingest_service_server::IngestServiceServer<DummyIngest>>,
        _query_client: Option<v1::query_service_client::QueryServiceClient<()>>,
        _config_client: Option<v1::config_service_client::ConfigServiceClient<()>>,
    ) {
    }

    struct DummyIngest;
    #[tonic::async_trait]
    impl v1::ingest_service_server::IngestService for DummyIngest {
        async fn record_event(
            &self,
            _: tonic::Request<v1::RecordEventRequest>,
        ) -> Result<tonic::Response<v1::RecordEventResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("test stub"))
        }
        async fn record_batch(
            &self,
            _: tonic::Request<v1::RecordBatchRequest>,
        ) -> Result<tonic::Response<v1::RecordBatchResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("test stub"))
        }
        async fn amend_event(
            &self,
            _: tonic::Request<v1::AmendEventRequest>,
        ) -> Result<tonic::Response<v1::AmendEventResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("test stub"))
        }
        async fn void_run(
            &self,
            _: tonic::Request<v1::VoidRunRequest>,
        ) -> Result<tonic::Response<v1::VoidRunResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented("test stub"))
        }
    }
}
