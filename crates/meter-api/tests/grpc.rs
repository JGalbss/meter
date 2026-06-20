//! Integration test for the gRPC `LedgerService` against a real Postgres ledger. Drives the service
//! trait directly (the same code path a tonic server dispatches to), so it verifies the proto<->domain
//! mapping and the money logic end to end.

use meter_api::grpc::ledger::LedgerGrpc;
use meter_proto::v1;
use meter_proto::v1::ledger_service_server::LedgerService;
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tonic::Request;

fn credit(amount: &str) -> Option<v1::Credit> {
    Some(v1::Credit {
        amount: amount.to_owned(),
    })
}

#[tokio::test]
async fn ledger_grpc_reserve_settle_flow() {
    let postgres = Postgres::default().start().await.expect("start postgres");
    let port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("migrate");
    let service = LedgerGrpc::new(ledger);

    // Open a no-overdraft org account.
    let account = service
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

    // Grant 100 credits.
    let granted = service
        .grant(Request::new(v1::GrantRequest {
            account_id: account.clone(),
            amount: credit("100"),
            source: v1::CreditSource::Paid as i32,
            idempotency_key: String::new(),
        }))
        .await
        .expect("grant")
        .into_inner();
    assert_eq!(granted.settled.unwrap().amount, "100");

    // Reserve 40 (hard) — allowed.
    let reservation = "22222222-2222-2222-2222-222222222222".to_owned();
    let reserved = service
        .reserve(Request::new(v1::ReserveRequest {
            account_id: account.clone(),
            reservation_id: reservation.clone(),
            amount: credit("40"),
            limit: v1::LimitClass::Hard as i32,
            expires_at: String::new(),
        }))
        .await
        .expect("reserve")
        .into_inner();
    assert!(reserved.allowed);

    // Settle 30; balance becomes 70 / 0.
    let settled = service
        .settle(Request::new(v1::SettleRequest {
            reservation_id: reservation,
            actual: credit("30"),
        }))
        .await
        .expect("settle")
        .into_inner();
    assert_eq!(settled.balance_after.unwrap().amount, "70");

    let balance = service
        .balance(Request::new(v1::BalanceRequest {
            account_id: account.clone(),
        }))
        .await
        .expect("balance")
        .into_inner();
    assert_eq!(balance.settled.unwrap().amount, "70");
    assert_eq!(balance.held.unwrap().amount, "0");

    // Over-reserving is denied with the available/requested reported.
    let denied = service
        .reserve(Request::new(v1::ReserveRequest {
            account_id: account.clone(),
            reservation_id: "33333333-3333-3333-3333-333333333333".to_owned(),
            amount: credit("1000"),
            limit: v1::LimitClass::Hard as i32,
            expires_at: String::new(),
        }))
        .await
        .expect("reserve")
        .into_inner();
    assert!(!denied.allowed);
    assert_eq!(denied.available.unwrap().amount, "70");

    // Two reservations made over gRPC with past expiries.
    let kept = "44444444-4444-4444-4444-444444444444".to_owned();
    let dropped = "55555555-5555-5555-5555-555555555555".to_owned();
    for reservation_id in [kept.clone(), dropped] {
        service
            .reserve(Request::new(v1::ReserveRequest {
                account_id: account.clone(),
                reservation_id,
                amount: credit("5"),
                limit: v1::LimitClass::Hard as i32,
                expires_at: "1970-01-01T00:00:00Z".to_owned(),
            }))
            .await
            .expect("reserve with expiry");
    }

    // ExtendHold over gRPC pushes one hold's expiry to the future; the sweep then catches only the
    // other — proving both that the expiry persisted and that the heartbeat works over gRPC.
    service
        .extend_hold(Request::new(v1::ExtendHoldRequest {
            reservation_id: kept,
            expires_at: "2100-01-01T00:00:00Z".to_owned(),
        }))
        .await
        .expect("extend_hold");
    let swept = service
        .void_expired_holds(Request::new(v1::VoidExpiredHoldsRequest {}))
        .await
        .expect("sweep")
        .into_inner()
        .released;
    assert_eq!(swept, 1);
    // The extended hold survives: 5 credits still held.
    assert_eq!(
        service
            .balance(Request::new(v1::BalanceRequest {
                account_id: account
            }))
            .await
            .expect("balance")
            .into_inner()
            .held
            .unwrap()
            .amount,
        "5"
    );
}
