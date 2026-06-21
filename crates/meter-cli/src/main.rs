//! `meterctl` — admin CLI for the meter engine.
//!
//! Runs ledger + event migrations and other operational tasks against the engine database without
//! booting the HTTP server.

#![forbid(unsafe_code)]

use anyhow::Context;
use clap::{Parser, Subcommand};
use meter_core::{AccountId, Credit, Currency, Money, OrgId, RunId};
use meter_ledger::{
    AccountScope, CreditSource, GrantRequest, LedgerBackend, NewAccount, ReservationId,
};
use meter_pricing::{price_usage, ContextTier, Modality, PricingDimension, Usage};
use meter_ratecards::rate_card_for;
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use uuid::Uuid;

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
    /// Print an account's balance (settled / held / available).
    Balance {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// The account id (UUID).
        #[arg(long)]
        account: Uuid,
    },
    /// List an account's ledger entries (the immutable audit trail), oldest first.
    Entries {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// The account id (UUID).
        #[arg(long)]
        account: Uuid,
    },
    /// Grant credits to an existing account.
    Grant {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// The account id (UUID).
        #[arg(long)]
        account: Uuid,
        /// How many credits to grant.
        #[arg(long)]
        credits: u64,
    },
    /// Price token usage for a catalog model (no database needed) — cost in USD and credits.
    Price {
        /// The catalog model id (e.g. `gpt-5`).
        #[arg(long)]
        model: String,
        #[arg(long, default_value_t = 0)]
        input: u64,
        #[arg(long, default_value_t = 0)]
        cache_read: u64,
        #[arg(long, default_value_t = 0)]
        cache_write: u64,
        #[arg(long, default_value_t = 0)]
        output: u64,
        /// The cash value of one credit, in USD.
        #[arg(long, default_value = "0.000001")]
        credit_value_usd: Decimal,
    },
    /// Release expired open holds (auto-void sweep for stranded reservations).
    Sweep {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
    },
    /// Release a specific open reservation (e.g. a stuck hold from a crashed run).
    Void {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// The reservation id (UUID).
        #[arg(long)]
        reservation: Uuid,
    },
    /// Reverse a whole run's ledger impact: release its open holds and refund its settled charges
    /// (the ledger half of killing a failed/abandoned run). Idempotent.
    VoidRun {
        /// Postgres connection string (defaults to $`METER_DATABASE_URL`).
        #[arg(long, env = "METER_DATABASE_URL")]
        database_url: String,
        /// The run id (UUID).
        #[arg(long)]
        run: Uuid,
    },
    /// Reconcile the pre-aggregated usage rollups against the event store of record (ClickHouse), by
    /// model and by promoted field. Prints any drift; exits non-zero if a rollup has diverged so it can
    /// gate a cron/alert.
    Reconcile {
        /// ClickHouse URL (defaults to $`METER_CLICKHOUSE_URL`, e.g. `http://127.0.0.1:8123`).
        #[arg(long, env = "METER_CLICKHOUSE_URL")]
        clickhouse_url: String,
        /// The org id (UUID) to reconcile.
        #[arg(long)]
        org: Uuid,
    },
    /// Rebuild an org's pre-aggregated rollups from the event source of record — the repair for drift
    /// that `reconcile` reports. Clears and repopulates them from the live aggregate.
    RebuildRollups {
        /// ClickHouse URL (defaults to $`METER_CLICKHOUSE_URL`, e.g. `http://127.0.0.1:8123`).
        #[arg(long, env = "METER_CLICKHOUSE_URL")]
        clickhouse_url: String,
        /// The org id (UUID) to rebuild.
        #[arg(long)]
        org: Uuid,
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
        Command::Balance {
            database_url,
            account,
        } => balance(&database_url, AccountId::from_uuid(account)).await,
        Command::Entries {
            database_url,
            account,
        } => entries(&database_url, AccountId::from_uuid(account)).await,
        Command::Grant {
            database_url,
            account,
            credits,
        } => grant(&database_url, AccountId::from_uuid(account), credits).await,
        Command::Price {
            model,
            input,
            cache_read,
            cache_write,
            output,
            credit_value_usd,
        } => price(
            &model,
            input,
            cache_read,
            cache_write,
            output,
            credit_value_usd,
        ),
        Command::Sweep { database_url } => sweep(&database_url).await,
        Command::Void {
            database_url,
            reservation,
        } => void(&database_url, ReservationId::from_uuid(reservation)).await,
        Command::VoidRun { database_url, run } => {
            void_run(&database_url, RunId::from_uuid(run)).await
        }
        Command::Reconcile {
            clickhouse_url,
            org,
        } => reconcile(&clickhouse_url, org).await,
        Command::RebuildRollups {
            clickhouse_url,
            org,
        } => rebuild_rollups(&clickhouse_url, org).await,
    }
}

async fn sweep(database_url: &str) -> anyhow::Result<()> {
    let released = connect(database_url)
        .await?
        .void_expired_holds(OffsetDateTime::now_utc())
        .await
        .map_err(|error| anyhow::anyhow!("sweeping expired holds: {error}"))?;
    println!("released {released} expired holds");
    Ok(())
}

async fn void(database_url: &str, reservation: ReservationId) -> anyhow::Result<()> {
    connect(database_url)
        .await?
        .void(reservation)
        .await
        .map_err(|error| anyhow::anyhow!("voiding reservation: {error}"))?;
    println!("voided reservation {reservation}");
    Ok(())
}

async fn void_run(database_url: &str, run: RunId) -> anyhow::Result<()> {
    let summary = connect(database_url)
        .await?
        .void_run(run)
        .await
        .map_err(|error| anyhow::anyhow!("voiding run: {error}"))?;
    println!("voided run {run}");
    println!("  holds released   {}", summary.holds_released);
    println!("  charges refunded {}", summary.charges_refunded);
    println!(
        "  credits refunded {}",
        summary.credits_refunded.value().normalize()
    );
    Ok(())
}

async fn reconcile(clickhouse_url: &str, org: Uuid) -> anyhow::Result<()> {
    let store = ChStore::new(clickhouse_url).with_env_credentials();
    let drift = store
        .reconcile_rollups(org)
        .await
        .map_err(|error| anyhow::anyhow!("reconciling org {org}: {error}"))?;
    if drift.is_empty() {
        println!("org {org}: rollups consistent with the event source of record");
        return Ok(());
    }
    println!("org {org}: {} group(s) drifted", drift.len());
    for row in &drift {
        println!(
            "  [{}] {:<24}  rollup {} events / {} credits  vs  scan {} events / {} credits",
            row.scope,
            row.dimension,
            row.rollup_events,
            row.rollup_credits,
            row.scan_events,
            row.scan_credits
        );
    }
    anyhow::bail!("rollup drift detected for org {org}");
}

async fn rebuild_rollups(clickhouse_url: &str, org: Uuid) -> anyhow::Result<()> {
    let store = ChStore::new(clickhouse_url).with_env_credentials();
    store
        .rebuild_rollups(org)
        .await
        .map_err(|error| anyhow::anyhow!("rebuilding rollups for org {org}: {error}"))?;
    println!("org {org}: rollups rebuilt from the event source of record");
    Ok(())
}

fn price(
    model: &str,
    input: u64,
    cache_read: u64,
    cache_write: u64,
    output: u64,
    credit_value_usd: Decimal,
) -> anyhow::Result<()> {
    let card = rate_card_for(model).with_context(|| format!("unknown model: {model}"))?;
    let usd = Currency::new("USD").map_err(|error| anyhow::anyhow!("currency: {error}"))?;
    let credit_value = Money::new(credit_value_usd, usd);

    let mut usage = Usage::new(Modality::Text, ContextTier::Standard);
    for (dimension, quantity) in [
        (PricingDimension::InputUncached, input),
        (PricingDimension::CacheRead, cache_read),
        (PricingDimension::CacheWrite, cache_write),
        (PricingDimension::Output, output),
    ] {
        if quantity > 0 {
            usage = usage.with(dimension, Decimal::from(quantity));
        }
    }

    let priced = price_usage(&usage, &card, &credit_value)
        .map_err(|error| anyhow::anyhow!("pricing: {error}"))?;
    println!("model   {model}");
    println!("  cogs    {} USD", priced.cogs.amount().normalize());
    println!(
        "  price   {} USD",
        priced.customer_price.amount().normalize()
    );
    println!("  credits {}", priced.credits.value().normalize());
    Ok(())
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

async fn balance(database_url: &str, account: AccountId) -> anyhow::Result<()> {
    let ledger = connect(database_url).await?;
    let balance = ledger
        .balance(account)
        .await
        .map_err(|error| anyhow::anyhow!("reading balance: {error}"))?;
    println!("account {account}");
    println!("  settled   {} credits", balance.settled.value());
    println!("  held      {} credits", balance.held.value());
    println!("  available {} credits", balance.available().value());
    Ok(())
}

async fn entries(database_url: &str, account: AccountId) -> anyhow::Result<()> {
    let entries = connect(database_url)
        .await?
        .entries(account)
        .await
        .map_err(|error| anyhow::anyhow!("reading entries: {error}"))?;
    println!("account {account} — {} entries", entries.len());
    for entry in &entries {
        println!(
            "  {}  {:<8?}  delta {:>12}  balance {:>12}",
            entry.created_at,
            entry.entry_type,
            entry.delta_credits.value().normalize(),
            entry.balance_after.value().normalize(),
        );
    }
    Ok(())
}

async fn grant(database_url: &str, account: AccountId, credits: u64) -> anyhow::Result<()> {
    let ledger = connect(database_url).await?;
    ledger
        .grant(GrantRequest {
            account,
            amount: Credit::from(credits),
            source: CreditSource::Paid,
            idempotency_key: None,
        })
        .await
        .map_err(|error| anyhow::anyhow!("granting credits: {error}"))?;
    let balance = ledger
        .balance(account)
        .await
        .map_err(|error| anyhow::anyhow!("reading balance: {error}"))?;
    println!("granted {credits} credits to {account}");
    println!("  balance {} credits", balance.settled.value());
    Ok(())
}
