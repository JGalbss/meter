//! The Postgres ledger backend must pass the identical shared conformance suite as the in-memory
//! reference, executed against a real Postgres started by testcontainers.

use std::sync::Arc;
use std::time::Instant;

use meter_core::{Credit, OrgId};
use meter_ledger::conformance::{self, HoldSpec, Op};
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
async fn statement_timeout_aborts_a_stuck_query() {
    // The hot money path begins transactions with `SET LOCAL statement_timeout` so a hung query can't
    // hold account-row locks indefinitely. Prove the mechanism against real Postgres: a query that
    // exceeds the timeout is aborted rather than blocking forever.
    let (_container, ledger) = start_ledger().await;
    let mut tx = ledger.pool().begin().await.expect("begin");
    sqlx::query("SET LOCAL statement_timeout = 50")
        .execute(&mut *tx)
        .await
        .expect("set statement_timeout");
    let result = sqlx::query("SELECT pg_sleep(1)").execute(&mut *tx).await;
    assert!(
        result.is_err(),
        "a query exceeding statement_timeout must be aborted"
    );
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

#[tokio::test]
async fn void_run_invariants_hold_on_postgres() {
    let (_container, ledger) = start_ledger().await;
    // Run 0: one open + one settled (refundable) + one zero-settle (not refundable).
    // Run 1: one open + one settled — must be untouched when run 0 is voided.
    let specs = vec![
        HoldSpec {
            run: 0,
            amount: 40,
            settle: None,
        },
        HoldSpec {
            run: 0,
            amount: 30,
            settle: Some(20),
        },
        HoldSpec {
            run: 0,
            amount: 15,
            settle: Some(0),
        },
        HoldSpec {
            run: 1,
            amount: 25,
            settle: None,
        },
        HoldSpec {
            run: 1,
            amount: 10,
            settle: Some(7),
        },
    ];
    conformance::void_run_property(&ledger, &specs, 0).await;

    // Also exercise voiding a run with no holds at all (everything zero, balance unchanged).
    let (_container2, ledger2) = start_ledger().await;
    conformance::void_run_property(&ledger2, &specs, 3).await;
}

/// Leasing is the hot-account mitigation: sessions lease from a shared parent pool. Many leases opened
/// concurrently from one no-overdraft parent must conserve credits and never over-lease — the parent
/// row serializes the transfers, so at most `funded / size` leases succeed and the parent plus all
/// children always sum back to the funded amount.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_leases_conserve_and_never_over_lease() {
    const FUNDED: i64 = 100;
    const LEASE: i64 = 10;
    const ATTEMPTS: usize = 30;

    let (_container, ledger) = start_ledger().await;
    let ledger = Arc::new(ledger);
    let parent = ledger
        .open_account(NewAccount {
            org_id: OrgId::new(),
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .expect("open")
        .id;
    ledger
        .grant(GrantRequest {
            account: parent,
            amount: Credit::from(FUNDED),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .expect("grant");

    let mut handles = Vec::new();
    for _ in 0..ATTEMPTS {
        let ledger = Arc::clone(&ledger);
        handles.push(tokio::spawn(async move {
            ledger
                .open_lease(meter_ledger::LeaseRequest {
                    parent,
                    amount: Credit::from(LEASE),
                })
                .await
        }));
    }

    let mut children = Vec::new();
    for handle in handles {
        if let Ok(Ok(child)) = handle.await {
            children.push(child.id);
        }
    }

    // No-overdraft parent: at most FUNDED / LEASE leases can succeed.
    assert!(
        children.len() <= (FUNDED / LEASE) as usize,
        "over-leased: {} children from {FUNDED} / {LEASE}",
        children.len()
    );

    // Conservation: the parent plus every leased child sum back to the funded amount.
    let mut total = ledger
        .balance(parent)
        .await
        .expect("parent balance")
        .settled;
    for child in &children {
        total += ledger.balance(*child).await.expect("child balance").settled;
    }
    assert_eq!(
        total,
        Credit::from(FUNDED),
        "credits not conserved across concurrent leases"
    );
}

/// Two `void_run` calls racing on the same run must not double-refund its settled charge — that would
/// create credits from nothing. The hold-row `FOR UPDATE` lock serializes them and the refund
/// idempotency key makes the second a no-op, so the account ends at exactly one refund.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_void_run_does_not_double_refund() {
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

    // A settled charge in the run: reserve 30, settle 30 -> settled 70.
    let run = meter_core::RunId::new();
    let reservation = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: Credit::from(30_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve");
    ledger
        .settle(SettleRequest {
            reservation_id: reservation,
            actual: Credit::from(30_i64),
        })
        .await
        .expect("settle");

    // Two concurrent voids of the same run.
    let a = {
        let ledger = Arc::clone(&ledger);
        tokio::spawn(async move { ledger.void_run(run).await })
    };
    let b = {
        let ledger = Arc::clone(&ledger);
        tokio::spawn(async move { ledger.void_run(run).await })
    };
    let first = a.await.expect("task a").expect("void_run a");
    let second = b.await.expect("task b").expect("void_run b");

    // Exactly one of the two refunded the settled charge; the other was a no-op.
    assert_eq!(
        first.charges_refunded + second.charges_refunded,
        1,
        "the settled charge must be refunded exactly once across concurrent voids"
    );
    // Balance reflects a single refund: 70 + 30 = 100, not 130.
    assert_eq!(
        ledger.balance(account).await.expect("balance").settled,
        Credit::from(100_i64),
        "concurrent voids must not double-refund"
    );
}

/// `void_run` racing a `settle` on the same run must never corrupt the ledger. Whichever wins the
/// `FOR UPDATE` lock, the run nets to zero: if settle wins it charges, then void_run refunds it; if
/// void_run wins the hold is released and settle is refused. Either way the account returns to the
/// granted balance with nothing held — a deterministic invariant, so the test is not flaky.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn void_run_racing_settle_conserves_credits() {
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

    let run = meter_core::RunId::new();
    let reservation = ReservationId::new();
    ledger
        .reserve(ReserveRequest {
            account,
            reservation_id: reservation,
            amount: Credit::from(30_i64),
            limit: LimitClass::Hard,
            expires_at: None,
            run_id: Some(run),
        })
        .await
        .expect("reserve");

    // Fire settle and void_run concurrently; they contend on the hold's row lock.
    let settle_ledger = Arc::clone(&ledger);
    let settle = tokio::spawn(async move {
        settle_ledger
            .settle(SettleRequest {
                reservation_id: reservation,
                actual: Credit::from(30_i64),
            })
            .await
    });
    let void_ledger = Arc::clone(&ledger);
    let void = tokio::spawn(async move { void_ledger.void_run(run).await });

    // settle may succeed (then void_run refunds it) or be refused (void_run won first) — both fine.
    let _ = settle.await.expect("settle task");
    void.await.expect("void task").expect("void_run");

    // Deterministic regardless of who won: the voided run leaves no net charge and no open hold.
    let balance = ledger.balance(account).await.expect("balance");
    assert_eq!(
        balance.settled,
        Credit::from(100_i64),
        "voided run must net to zero"
    );
    assert_eq!(balance.held, Credit::from(0_i64), "no hold left open");
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
