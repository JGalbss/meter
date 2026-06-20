//! The meter engine binary.
//!
//! Connects to Postgres, applies the ledger migrations, and serves the HTTP API. Configuration is via
//! environment: `METER_DATABASE_URL` (required) and `METER_LISTEN_ADDR` (default `0.0.0.0:8080`).

#![forbid(unsafe_code)]

use std::net::SocketAddr;

use anyhow::Context;
use meter_api::{router, AppState};
use meter_core::{Currency, Money};
use meter_store_pg::{PgEventStore, PgLedger};
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
    let addr: SocketAddr = std::env::var("METER_LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse()
        .context("METER_LISTEN_ADDR must be a valid socket address")?;

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await
        .context("connecting to Postgres")?;
    let ledger = PgLedger::new(pool.clone());
    ledger
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running migrations: {error}"))?;
    let events = PgEventStore::new(pool);

    // The cash value of one credit (USD), used to price usage into credits.
    let credit_value_usd: Decimal = std::env::var("METER_CREDIT_VALUE")
        .unwrap_or_else(|_| "0.000001".to_owned())
        .parse()
        .context("METER_CREDIT_VALUE must be a decimal")?;
    let usd = Currency::new("USD").map_err(|error| anyhow::anyhow!("currency: {error}"))?;
    let credit_value = Money::new(credit_value_usd, usd);

    let app = router(AppState::new(ledger, events, credit_value));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "meter engine listening");
    axum::serve(listener, app).await.context("serving HTTP")?;
    Ok(())
}
