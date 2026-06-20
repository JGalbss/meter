//! `meterctl` — admin CLI for the meter engine.
//!
//! Runs ledger + event migrations and other operational tasks against the engine database without
//! booting the HTTP server.

#![forbid(unsafe_code)]

use anyhow::Context;
use clap::{Parser, Subcommand};
use meter_core::{Credit, OrgId};
use meter_ledger::{AccountScope, CreditSource, GrantRequest, LedgerBackend, NewAccount};
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
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
    },
    /// Seed a funded org account for local development (runs migrations first).
    Seed {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// How many credits to grant the new account.
        #[arg(long, default_value_t = 1_000_000)]
        credits: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Migrate { database_url } => migrate(&database_url).await,
        Command::Seed {
            database_url,
            credits,
        } => seed(&database_url, credits).await,
    }
}

async fn connect(database_url: &str) -> anyhow::Result<PgLedger> {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(database_url)
        .await
        .context("connecting to Postgres")?;
    Ok(PgLedger::new(pool))
}

async fn migrate(database_url: &str) -> anyhow::Result<()> {
    connect(database_url)
        .await?
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running migrations: {error}"))?;
    println!("migrations applied");
    Ok(())
}

async fn seed(database_url: &str, credits: u64) -> anyhow::Result<()> {
    let ledger = connect(database_url).await?;
    ledger
        .migrate()
        .await
        .map_err(|error| anyhow::anyhow!("running migrations: {error}"))?;

    let org = OrgId::new();
    let account = ledger
        .open_account(NewAccount {
            org_id: org,
            scope: AccountScope::Org,
            no_overdraft: true,
            parent_id: None,
        })
        .await
        .map_err(|error| anyhow::anyhow!("opening account: {error}"))?;
    ledger
        .grant(GrantRequest {
            account: account.id,
            amount: Credit::from(credits),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .map_err(|error| anyhow::anyhow!("granting credits: {error}"))?;
    let balance = ledger
        .balance(account.id)
        .await
        .map_err(|error| anyhow::anyhow!("reading balance: {error}"))?;

    println!("seeded org {org}");
    println!("  account {}", account.id);
    println!("  balance {} credits", balance.settled.value());
    Ok(())
}
