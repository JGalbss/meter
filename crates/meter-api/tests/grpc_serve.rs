//! End-to-end test that the assembled gRPC server actually serves over the wire: bind it on an
//! ephemeral port, then drive the LedgerService with a real tonic client over HTTP/2.

use meter_api::{grpc, AppState};
use meter_core::{Currency, Money};
use meter_proto::v1;
use meter_proto::v1::ledger_service_client::LedgerServiceClient;
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Request;

#[tokio::test]
async fn grpc_server_serves_ledger_over_the_wire() {
    let postgres = Postgres::default().start().await.expect("start postgres");
    let pg_port = postgres.get_host_port_ipv4(5432).await.expect("pg port");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&format!(
            "postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres"
        ))
        .await
        .expect("connect");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("pg migrate");

    let clickhouse = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let ch_port = clickhouse.get_host_port_ipv4(8123).await.expect("ch port");
    let events = ChStore::new(&format!("http://127.0.0.1:{ch_port}"));
    events.migrate().await.expect("ch migrate");

    let credit_value = Money::new(Decimal::new(1, 6), Currency::new("USD").expect("usd"));
    let state = AppState::new(ledger, events.clone(), events, credit_value);

    // Serve the gRPC router on an ephemeral port (the listener is bound before serving, so the client
    // connect below never races a missing socket).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        grpc::router(state)
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("serve");
    });

    let mut client = LedgerServiceClient::connect(format!("http://{addr}"))
        .await
        .expect("connect");

    let account = client
        .open_account(Request::new(v1::OpenAccountRequest {
            org_id: "11111111-1111-1111-1111-111111111111".to_owned(),
            scope: v1::AccountScope::Org as i32,
            no_overdraft: true,
            parent_id: String::new(),
        }))
        .await
        .expect("open_account")
        .into_inner()
        .account_id;

    client
        .grant(Request::new(v1::GrantRequest {
            account_id: account.clone(),
            amount: Some(v1::Credit {
                amount: "100".to_owned(),
            }),
            source: v1::CreditSource::Paid as i32,
            idempotency_key: String::new(),
        }))
        .await
        .expect("grant");

    let balance = client
        .balance(Request::new(v1::BalanceRequest {
            account_id: account,
        }))
        .await
        .expect("balance")
        .into_inner();
    assert_eq!(balance.settled.unwrap().amount, "100");
    assert_eq!(balance.held.unwrap().amount, "0");
}
