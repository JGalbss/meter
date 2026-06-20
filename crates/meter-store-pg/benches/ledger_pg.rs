//! Benchmark for the enforcement hot path against the **Postgres** backend — a representative
//! latency figure (indexed idempotency + settled balance on the account row; a real DB round-trip
//! per call against a local container).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use meter_core::{Credit, OrgId};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, LimitClass, NewAccount, ReservationId,
    ReserveRequest, SettleRequest,
};
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tokio::runtime::Runtime;

fn bench_reserve_settle_pg(c: &mut Criterion) {
    let rt = Runtime::new().expect("runtime");

    // Spin a Postgres container once, migrate, and fund one account generously.
    let (container, ledger, account) = rt.block_on(async {
        let container = Postgres::default().start().await.expect("start postgres");
        let port = container
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
        (container, ledger, account)
    });

    c.bench_function("pg_reserve_settle_hard", |b| {
        b.to_async(&rt).iter(|| async {
            let reservation = ReservationId::new();
            ledger
                .reserve(ReserveRequest {
                    account,
                    reservation_id: reservation,
                    amount: black_box(Credit::from(10i64)),
                    limit: LimitClass::Hard,
                    expires_at: None,
                    run_id: None,
                })
                .await
                .expect("reserve");
            ledger
                .settle(SettleRequest {
                    reservation_id: reservation,
                    actual: black_box(Credit::from(8i64)),
                })
                .await
                .expect("settle");
        });
    });

    // Tear down explicitly inside the runtime: ContainerAsync's async Drop panics if it runs
    // without a current tokio runtime (which is the case once criterion returns).
    rt.block_on(async move {
        drop(ledger);
        container.rm().await.expect("remove container");
    });
}

criterion_group!(benches, bench_reserve_settle_pg);
criterion_main!(benches);
