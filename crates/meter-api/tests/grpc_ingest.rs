//! Integration test for the gRPC `IngestService` against a real ClickHouse event store.

use meter_api::grpc::ingest::IngestGrpc;
use meter_proto::v1;
use meter_proto::v1::ingest_service_server::IngestService;
use meter_store_ch::ChStore;
use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tonic::Request;

const ORG: &str = "11111111-1111-1111-1111-111111111111";
const ACCOUNT: &str = "44444444-4444-4444-4444-444444444444";
const RUN: &str = "55555555-5555-5555-5555-555555555555";

fn event(key: &str, props: &str) -> v1::RecordEventRequest {
    v1::RecordEventRequest {
        org_id: ORG.to_owned(),
        idempotency_key: key.to_owned(),
        event_time: String::new(),
        meter: "tokens".to_owned(),
        account_id: ACCOUNT.to_owned(),
        run_id: RUN.to_owned(),
        properties: props.to_owned(),
    }
}

#[tokio::test]
async fn ingest_grpc_record_amend_void_flow() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let store = ChStore::new(&format!("http://127.0.0.1:{port}"));
    store.migrate().await.expect("migrate");
    let service = IngestGrpc::new(store);

    // Record one event.
    let id1 = service
        .record_event(Request::new(event("g1", r#"{"input":10}"#)))
        .await
        .expect("record_event")
        .into_inner()
        .event_id;
    assert!(!id1.is_empty());

    // Bulk-record two more.
    let accepted = service
        .record_batch(Request::new(v1::RecordBatchRequest {
            events: vec![
                event("g2", r#"{"input":20}"#),
                event("g3", r#"{"input":30}"#),
            ],
        }))
        .await
        .expect("record_batch")
        .into_inner()
        .accepted;
    assert_eq!(accepted, 2);

    // Amend the first event into a new version.
    let amended = service
        .amend_event(Request::new(v1::AmendEventRequest {
            event_id: id1.clone(),
            properties: r#"{"input":11}"#.to_owned(),
            idempotency_key: String::new(),
        }))
        .await
        .expect("amend_event")
        .into_inner()
        .event_id;
    assert_ne!(amended, id1);

    // Void the run: the three current recorded events (amended g1 + g2 + g3) are voided.
    let voided = service
        .void_run(Request::new(v1::VoidRunRequest {
            run_id: RUN.to_owned(),
        }))
        .await
        .expect("void_run")
        .into_inner()
        .voided;
    assert_eq!(voided, 3);

    // Invalid JSON properties are rejected as invalid_argument.
    let bad = service
        .record_event(Request::new(event("bad", "{not json")))
        .await;
    assert_eq!(bad.unwrap_err().code(), tonic::Code::InvalidArgument);
}
