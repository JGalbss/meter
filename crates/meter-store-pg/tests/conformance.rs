//! The Postgres ledger backend must pass the identical shared conformance suite as the in-memory
//! reference, executed against a real Postgres started by testcontainers.

use std::sync::Arc;
use std::time::Instant;

use meter_core::{Credit, OrgId};
use meter_ledger::conformance::{self, Op};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveOutcome, ReserveRequest, SettleRequest,
};
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ContainerAsync;

async fn start_ledger() -> (ContainerAsync<Postgres>, PgLedger) {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .expect("connect to postgres");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("run migrations");
    (container, ledger)
}

#[tokio::test]
async fn passes_the_shared_conformance_scenarios() {
    let (_container, ledger) = start_ledger().await;
    conformance::run_all_scenarios(&ledger).await;
}

#[tokio::test]
async fn matches_the_model_over_a_sequence() {
    let (_container, ledger) = start_ledger().await;
    let ops = vec![
        Op::Grant(100),
        Op::Spend {
            reserve: 40,
            actual: 30,
        },
        Op::Spend {
            reserve: 500,
            actual: 0,
        }, // denied: insufficient
        Op::Grant(50),
        Op::Spend {
            reserve: 80,
            actual: 80,
        },
        Op::Spend {
            reserve: 10,
            actual: 5,
        },
        Op::Grant(25),
        Op::Spend {
            reserve: 200,
            actual: 0,
        }, // denied: insufficient
        Op::Void { reserve: 20 },  // reserve then release: settled unchanged
        Op::Void { reserve: 500 }, // denied reservation, void is a no-op
    ];
    conformance::check_against_model(&ledger, &ops).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reserves_never_overdraft() {
    let (_container, ledger) = start_ledger().await;
    let ledger = Arc::new(ledger);
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
            amount: Credit::from(100_i64),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    // 50 racing reservations of 10 credits each against a 100-credit balance: at most 10 may succeed.
    let mut handles = Vec::new();
    for _ in 0..50 {
        let ledger = Arc::clone(&ledger);
        handles.push(tokio::spawn(async move {
            ledger
                .reserve(ReserveRequest {
                    account,
                    reservation_id: ReservationId::new(),
                    amount: Credit::from(10_i64),
                    limit: LimitClass::Hard,
                    expires_at: None,
                    run_id: None,
                })
                .await
        }));
    }

    let mut allowed = 0_i64;
    for handle in handles {
        if let Ok(Ok(ReserveOutcome::Allowed { .. })) = handle.await {
            allowed += 1;
        }
    }

    let balance = ledger.balance(account).await.expect("balance");
    assert!(
        allowed <= 10,
        "allowed {allowed} reservations exceeds capacity of 10"
    );
    assert_eq!(balance.held, Credit::from(allowed * 10));
    assert!(
        !balance.available().is_negative(),
        "overdraft under concurrency"
    );
}

/// Load harness: many workers hammer reserve→settle on one funded account in parallel. The ledger
/// must conserve credits exactly — every settle charges its actual and releases its hold, with no
/// lost or double-counted credits — so the final settled balance equals `funded - sum(actuals)` and
/// nothing stays held. Also reports sustained throughput.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reserve_settle_conserves_credits() {
    const WORKERS: i64 = 8;
    const PER_WORKER: i64 = 25;
    const RESERVE: i64 = 10;
    const ACTUAL: i64 = 3;
    const FUNDED: i64 = 100_000;

    let (_container, ledger) = start_ledger().await;
    let ledger = Arc::new(ledger);
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
            amount: Credit::from(FUNDED),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    let started = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..WORKERS {
        let ledger = Arc::clone(&ledger);
        handles.push(tokio::spawn(async move {
            for _ in 0..PER_WORKER {
                let reservation = ReservationId::new();
                let outcome = ledger
                    .reserve(ReserveRequest {
                        account,
                        reservation_id: reservation,
                        amount: Credit::from(RESERVE),
                        limit: LimitClass::Hard,
                        expires_at: None,
                        run_id: None,
                    })
                    .await
                    .expect("reserve");
                assert!(
                    matches!(outcome, ReserveOutcome::Allowed { .. }),
                    "reserve denied"
                );
                ledger
                    .settle(SettleRequest {
                        reservation_id: reservation,
                        actual: Credit::from(ACTUAL),
                    })
                    .await
                    .expect("settle");
            }
        }));
    }
    for handle in handles {
        handle.await.expect("worker panicked");
    }

    let ops = WORKERS * PER_WORKER;
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(
        balance.settled,
        Credit::from(FUNDED - ops * ACTUAL),
        "credits not conserved under concurrent settles"
    );
    assert_eq!(
        balance.held,
        Credit::from(0_i64),
        "holds left open after settle"
    );

    let elapsed = started.elapsed();
    eprintln!(
        "concurrent reserve+settle: {ops} cycles in {:.3}s ({:.0} ops/s)",
        elapsed.as_secs_f64(),
        f64::from(i32::try_from(ops).unwrap_or(i32::MAX)) / elapsed.as_secs_f64()
    );
}
