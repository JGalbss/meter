//! `meterctl` — admin CLI for the meter engine.
//!
//! Runs ledger + event migrations and other operational tasks against the engine database without
//! booting the HTTP server.

#![forbid(unsafe_code)]

use anyhow::Context;
use clap::{Parser, Subcommand};
use meter_store_pg::PgLedger;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser)]
#[command(name = "meterctl", about = "Admin CLI for the meter engine", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Apply the engine database migrations (idempotent).
    Migrate {
        /// Postgres connection string (defaults to $METER_DATABASE_URL).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Migrate { database_url } => migrate(&database_url).await,
    }
}

async fn migrate(database_url: &str) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await
        .context("connecting to Postgres")?;
    PgLedger::new(pool)
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running migrations: {error}"))?;
    println!("migrations applied");
    Ok(())
}
