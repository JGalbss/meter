//! The Postgres ledger backend must pass the identical shared conformance suite as the in-memory
//! reference, executed against a real Postgres started by testcontainers.

use std::sync::Arc;

use meter_core::{Credit, OrgId};
use meter_ledger::conformance::{self, Op};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveOutcome, ReserveRequest,
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
