//! Concurrent durable-throughput harness for the ledger money path: sustained reserve→settle
//! operations per second against real Postgres, across many independent accounts (the realistic
//! multi-tenant / multi-session pattern). Run with:
//!
//! ```bash
//! cargo run --release --example throughput -p meter-store-pg
//! ```
//!
//! Each worker drives its own account, so this measures the ledger's true parallel write throughput
//! (not single-row contention — that hot-account case is what per-session leasing exists to avoid).
//! A throwaway Postgres container is started automatically. Wall-clock measured, not extrapolated.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use meter_core::{Credit, OrgId};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveRequest, SettleRequest,
};
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const WORKERS: usize = 32;
const DURATION: Duration = Duration::from_secs(8);

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(WORKERS as u32 + 4)
        .connect(&url)
        .await
        .expect("connect");
    let ledger = Arc::new(PgLedger::new(pool));
    ledger.migrate().await.expect("migrate");

    // One generously funded account per worker — independent rows, so writes run in parallel.
    let mut accounts = Vec::with_capacity(WORKERS);
    for _ in 0..WORKERS {
        let account = ledger
            .open_account(NewAccount {
                org_id: OrgId::new(),
                scope: AccountScope::Org,
                no_overdraft: true,
                parent_id: None,
            })
            .await
            .expect("open account")
            .id;
        ledger
            .grant(GrantRequest {
                account,
                amount: Credit::from(1_000_000_000_000i64),
                source: CreditSource::Paid,
                idempotency_key: None,
            })
            .await
            .expect("grant");
        accounts.push(account);
    }

    let cycles = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + DURATION;

    println!("meter ledger durable throughput — concurrent reserve+settle against real Postgres");
    println!("workers (independent accounts): {WORKERS}   measure window: {DURATION:?}\n");

    let start = Instant::now();
    let mut handles = Vec::with_capacity(WORKERS);
    for account in accounts {
        let ledger = Arc::clone(&ledger);
        let cycles = Arc::clone(&cycles);
        handles.push(tokio::spawn(async move {
            while Instant::now() < deadline {
                let reservation = ReservationId::new();
                ledger
                    .reserve(ReserveRequest {
                        account,
                        reservation_id: reservation,
                        amount: Credit::from(10i64),
                        limit: LimitClass::Hard,
                        expires_at: None,
                        run_id: None,
                    })
                    .await
                    .expect("reserve");
                ledger
                    .settle(SettleRequest {
                        reservation_id: reservation,
                        actual: Credit::from(8i64),
                    })
                    .await
                    .expect("settle");
                cycles.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    for handle in handles {
        handle.await.expect("worker");
    }
    let elapsed = start.elapsed().as_secs_f64();

    let total_cycles = cycles.load(Ordering::Relaxed);
    let round_trips = total_cycles * 2; // each cycle = reserve + settle
    let cycles_per_sec = total_cycles as f64 / elapsed;
    let rt_per_sec = round_trips as f64 / elapsed;

    println!(
        "reserve+settle cycles: {total_cycles} in {elapsed:.2}s  =>  {cycles_per_sec:.0} cycles/s"
    );
    println!(
        "ledger round trips:    {round_trips} in {elapsed:.2}s  =>  {rt_per_sec:.0} ops/s  ({:.2} B/day)",
        rt_per_sec * 86_400.0 / 1_000_000_000.0
    );
    println!(
        "\nsettlements per day: {:.2} billion   (each fully durable, no-overdraft, double-entry)",
        cycles_per_sec * 86_400.0 / 1_000_000_000.0
    );

    // Tear down inside the runtime (the container's async Drop needs a live runtime).
    drop(ledger);
    container.rm().await.expect("remove container");
}
