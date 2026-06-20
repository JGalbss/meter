//! Integration test for the gRPC `QueryService`: analytics from ClickHouse and invoicing from the
//! Postgres ledger, each against a real container.

use meter_api::grpc::query::QueryGrpc;
use meter_core::{AccountId, OrgId, RunId};
use meter_event::{EventStore, RecordEvent};
use meter_ledger::{
    CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveRequest, SettleRequest,
};
use meter_proto::v1;
use meter_proto::v1::query_service_server::QueryService;
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use time::macros::datetime;
use tonic::Request;

const ORG: &str = "11111111-1111-1111-1111-111111111111";

#[tokio::test]
async fn query_grpc_analytics_and_invoice() {
    // --- ClickHouse: record two usage events under one org/model. ---
    let ch = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let ch_port = ch.get_host_port_ipv4(8123).await.expect("ch port");
    let events = ChStore::new(&format!("http://127.0.0.1:{ch_port}"));
    events.migrate().await.expect("ch migrate");
    let org = OrgId::from_uuid(ORG.parse().unwrap());
    let account = AccountId::new();
    for (i, credits) in ["5", "7"].iter().enumerate() {
        events
            .record(RecordEvent {
                org_id: org,
                idempotency_key: format!("q-{i}"),
                event_time: datetime!(2026-06-20 12:00:00 UTC),
                meter: "tokens".to_owned(),
                account_id: account,
                run_id: Some(RunId::new()),
                properties: json!({ "model": "gpt-5", "output": 100, "credits": credits }),
            })
            .await
            .expect("record");
    }

    // --- Postgres: settle 30 credits on an account so the invoice has something to total. ---
    let pg = Postgres::default().start().await.expect("start postgres");
    let pg_port = pg.get_host_port_ipv4(5432).await.expect("pg port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("pg migrate");
    let billed = ledger
        .open_account(NewAccount {
            org_id: org,
            scope: meter_ledger::AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open")
        .id;
    ledger
        .grant(GrantRequest {
            account: billed,
            amount: meter_core::Credit::from(100_i64),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    let reservation = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account: billed,
            reservation_id: reservation,
            amount: meter_core::Credit::from(40_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: meter_core::Credit::from(30_i64),
        })
        .await
        .expect("settle");

    let service = QueryGrpc::new(events, ledger);

    // event_count over the org.
    let count = service
        .event_count(Request::new(v1::EventCountRequest {
            org_id: ORG.to_owned(),
        }))
        .await
        .expect("event_count")
        .into_inner()
        .count;
    assert_eq!(count, 2);

    // usage_by_model: one model with two events.
    let models = service
        .usage_by_model(Request::new(v1::UsageByModelRequest {
            org_id: ORG.to_owned(),
        }))
        .await
        .expect("usage_by_model")
        .into_inner()
        .models;
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model, "gpt-5");
    assert_eq!(models[0].events, 2);

    // invoice over a wide window totals the settled 30 credits.
    let invoice = service
        .invoice(Request::new(v1::InvoiceRequest {
            account_id: billed.to_string(),
            start: "2000-01-01T00:00:00Z".to_owned(),
            end: "2100-01-01T00:00:00Z".to_owned(),
        }))
        .await
        .expect("invoice")
        .into_inner();
    assert_eq!(invoice.total_credits.unwrap().amount, "30");
    assert_eq!(invoice.entries, 1);
}
