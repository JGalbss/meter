//! The meter engine binary.
//!
//! Connects to Postgres (money-truth) + `ClickHouse` (events, ADR 0003), applies migrations, and serves
//! the HTTP API. Configuration is via environment: `METER_DATABASE_URL` and `METER_CLICKHOUSE_URL`
//! (both required), `METER_LISTEN_ADDR` (default `0.0.0.0:8080`), and `METER_INGEST_MODE`
//! (`exactly_once` default | `append` for max throughput with upstream exactly-once, ADR 0005).

#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::Context;
use meter_api::{router, AppState};
use meter_core::{Currency, Money};
use meter_store_ch::{ChStore, IngestMode};
use meter_store_pg::PgLedger;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("METER_DATABASE_URL").context("METER_DATABASE_URL must be set")?;
    let clickhouse_url =
        std::env::var("METER_CLICKHOUSE_URL").context("METER_CLICKHOUSE_URL must be set")?;
    let addr: SocketAddr = std::env::var("METER_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse()
        .context("METER_LISTEN_ADDR must be a valid socket address")?;

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .context("connecting to Postgres")?;
    let ledger = PgLedger::new(pool);
    ledger
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running migrations: {error}"))?;
    // ClickHouse holds events + the audit log (both high-velocity firehoses, ADR 0003/0004).
    // METER_INGEST_MODE=append trades the cross-call dedup read for maximum throughput when ingest is
    // made exactly-once upstream (Kafka EOS, ADR 0005); default is the safe ExactlyOnce mode.
    let events = ChStore::new(&clickhouse_url).with_ingest_mode(ingest_mode_from_env());
    events
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running ClickHouse migrations: {error}"))?;

    // The cash value of one credit (USD), used to price usage into credits.
    let credit_value_usd: Decimal = std::env::var("METER_CREDIT_VALUE")
        .unwrap_or_else(|_| "0.000001".to_owned())
        .parse()
        .context("METER_CREDIT_VALUE must be a decimal")?;
    let usd = Currency::new("USD").map_err(|error| anyhow::anyhow!("currency: {error}"))?;
    let credit_value = Money::new(credit_value_usd, usd);

    let state = AppState::new(ledger, events.clone(), events, credit_value);

    // The gRPC surface (control-plane RPC) is served on its own port alongside HTTP.
    let grpc_addr: SocketAddr = std::env::var("METER_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".to_owned())
        .parse()
        .context("METER_GRPC_ADDR must be a valid socket address")?;
    let grpc = meter_api::grpc::router(state.clone());
    tokio::spawn(async move {
        tracing::info!(%grpc_addr, "meter engine gRPC listening");
        if let Err(error) = grpc.serve(grpc_addr).await {
            tracing::error!(%error, "gRPC server stopped");
        }
    });

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "meter engine HTTP listening");
    axum::serve(listener, app).await.context("serving HTTP")?;
    Ok(())
}

/// The event-ingest idempotency mode from `METER_INGEST_MODE` (`exactly_once` | `append`); defaults to
/// the safe `exactly_once`. See [`IngestMode`] / ADR 0005.
fn ingest_mode_from_env() -> IngestMode {
    match std::env::var("METER_INGEST_MODE").as_deref() {
        Ok("append") => IngestMode::Append,
        _ => IngestMode::ExactlyOnce,
    }
}
