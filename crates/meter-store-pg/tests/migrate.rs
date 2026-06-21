//! Migration safety: applying the engine migrations must **refuse** when the database is ahead of this
//! binary (a newer deployment already ran a migration this build doesn't know about). Running an older
//! binary's migrations against a newer schema could corrupt it, so `migrate` fails fast instead.

use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn migrate_refuses_when_the_database_is_ahead() {
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
    let ledger = PgLedger::new(pool.clone());

    // A clean apply succeeds.
    ledger.migrate().await.expect("initial migrate");

    // Simulate a newer build having applied a migration this binary doesn't ship: record a phantom,
    // higher-versioned migration in sqlx's bookkeeping table.
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) \
         VALUES (99990001, 'from a newer build', now(), true, '\\x00', 0)",
    )
    .execute(&pool)
    .await
    .expect("insert phantom migration");

    // Re-running migrations must now refuse rather than proceed against an unknown-ahead schema.
    let result = ledger.migrate().await;
    assert!(
        result.is_err(),
        "migrate must refuse when the database has a migration this binary does not know about"
    );
}
