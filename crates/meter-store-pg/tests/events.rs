//! The Postgres event store must pass the shared event conformance suite against a real Postgres.

use meter_event::conformance;
use meter_store_pg::{PgEventStore, PgLedger};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ContainerAsync;

async fn start_store() -> (ContainerAsync<Postgres>, PgEventStore) {
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
        .expect("connect");
    // Migrations live with the ledger; running them prepares the events table too.
    PgLedger::new(pool.clone())
        .migrate()
        .await
        .expect("run migrations");
    (container, PgEventStore::new(pool))
}

#[tokio::test]
async fn postgres_passes_event_conformance() {
    let (_container, store) = start_store().await;
    conformance::run_all_scenarios(&store).await;
}
