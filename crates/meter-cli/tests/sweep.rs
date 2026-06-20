//! Test for `meterctl sweep`: it releases expired open holds. Seeds an expired and a live hold via
//! the ledger, then runs the real binary against the same Postgres.

use std::process::Command;

use meter_core::{Credit, RunId};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveRequest, SettleRequest,
};
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use time::OffsetDateTime;
use uuid::Uuid;

#[tokio::test]
async fn sweep_releases_expired_holds() {
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

    let account = ledger
        .open_account(NewAccount {
            org_id: meter_core::OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open")
        .id;
    ledger
        .grant(GrantRequest {
            account,
            amount: Credit::from(100_i64),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    // One already-expired hold and one non-expiring hold.
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: ReservationId::from_uuid(Uuid::now_v7()),
            amount: Credit::from(40_i64),
            limit: LimitClass::Hard,
            expires_at: Some(OffsetDateTime::UNIX_EPOCH),
            run_id: None,
        })
        .await
        .expect("reserve expired");
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: ReservationId::from_uuid(Uuid::now_v7()),
            amount: Credit::from(10_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve live");

    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args(["sweep", "--database-url", &url])
        .output()
        .expect("run meterctl sweep");
    assert!(
        output.status.success(),
        "sweep failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("released 1 expired holds"));

    // Only the live hold remains held.
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.held, Credit::from(10_i64));
}

#[tokio::test]
async fn void_releases_a_specific_hold() {
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

    let account = ledger
        .open_account(NewAccount {
            org_id: meter_core::OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open")
        .id;
    ledger
        .grant(GrantRequest {
            account,
            amount: Credit::from(100_i64),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");
    // A non-expiring hold (the common case the sweep can't catch).
    let reservation = ReservationId::from_uuid(Uuid::now_v7());
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: Credit::from(40_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: None,
        })
        .await
        .expect("reserve");

    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args([
            "void",
            "--database-url",
            &url,
            "--reservation",
            &reservation.to_string(),
        ])
        .output()
        .expect("run meterctl void");
    assert!(
        output.status.success(),
        "void failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The hold is released.
    assert_eq!(
        ledger.balance(account).await.expect("balance").held,
        Credit::from(0_i64)
    );
}

#[tokio::test]
async fn void_run_reverses_a_runs_holds_and_settles() {
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

    let account = ledger
        .open_account(NewAccount {
            org_id: meter_core::OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open")
        .id;
    ledger
        .grant(GrantRequest {
            account,
            amount: Credit::from(100_i64),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    let run = RunId::new();
    // One open hold and one settled charge, both in the run.
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: ReservationId::from_uuid(Uuid::now_v7()),
            amount: Credit::from(40_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve open");
    let settled = ReservationId::from_uuid(Uuid::now_v7());
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: settled,
            amount: Credit::from(30_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve settled");
    ledger
        .settle(SettleRequest {
            reservation_id: settled,
            actual: Credit::from(20_i64),
        })
        .await
        .expect("settle");

    let output = Command::new(env!("CARGO_BIN_EXE_meterctl"))
        .args([
            "void-run",
            "--database-url",
            &url,
            "--run",
            &run.to_string(),
        ])
        .output()
        .expect("run meterctl void-run");
    assert!(
        output.status.success(),
        "void-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("holds released   1"), "stdout: {stdout}");
    assert!(stdout.contains("charges refunded 1"), "stdout: {stdout}");

    // The open hold is released and the settled charge refunded: settled 100, nothing held.
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(balance.settled, Credit::from(100_i64));
    assert_eq!(balance.held, Credit::from(0_i64));
}
