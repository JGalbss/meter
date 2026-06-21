//! The meter engine binary.
//!
//! Connects to Postgres (money-truth) + `ClickHouse` (events, ADR 0003), applies migrations, and serves
//! the HTTP and gRPC APIs. Configuration is via environment: `METER_DATABASE_URL` and
//! `METER_CLICKHOUSE_URL` (both required), `METER_LISTEN_ADDR` (default `0.0.0.0:8080`),
//! `METER_GRPC_ADDR` (default `0.0.0.0:50051`), `METER_INGEST_MODE` (`exactly_once` default | `append`
//! for max throughput with upstream exactly-once, ADR 0005), and `METER_ROLES` — which surfaces this
//! process serves (comma-separated `http`,`grpc`; default both), so a deployment can run dedicated
//! HTTP and gRPC replicas.

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

    // Which surfaces this process serves. Default is both; a deployment can split them across dedicated
    // replicas (e.g. HTTP-only edge nodes, gRPC-only control-plane nodes) via METER_ROLES.
    let roles = roles_from_env()?;

    let grpc_handle = match roles.grpc {
        true => {
            let grpc_addr: SocketAddr = std::env::var("METER_GRPC_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:50051".to_owned())
                .parse()
                .context("METER_GRPC_ADDR must be a valid socket address")?;
            let grpc = meter_api::grpc::router(state.clone());
            tracing::info!(%grpc_addr, "meter engine gRPC listening");
            Some(tokio::spawn(async move {
                grpc.serve(grpc_addr).await.context("serving gRPC")
            }))
        }
        false => None,
    };

    let http_handle = match roles.http {
        true => {
            let app = router(state);
            let listener = tokio::net::TcpListener::bind(addr)
                .await
                .with_context(|| format!("binding {addr}"))?;
            tracing::info!(%addr, "meter engine HTTP listening");
            Some(tokio::spawn(async move {
                axum::serve(listener, app).await.context("serving HTTP")
            }))
        }
        false => None,
    };

    // Run until any selected server stops (each serving forever is the steady state).
    match (grpc_handle, http_handle) {
        (Some(grpc), Some(http)) => tokio::select! {
            res = grpc => res.context("gRPC task")?.context("gRPC server")?,
            res = http => res.context("HTTP task")?.context("HTTP server")?,
        },
        (Some(grpc), None) => grpc.await.context("gRPC task")?.context("gRPC server")?,
        (None, Some(http)) => http.await.context("HTTP task")?.context("HTTP server")?,
        (None, None) => unreachable!("parse_roles guarantees at least one role"),
    }
    Ok(())
}

/// The surfaces this engine process serves.
#[derive(Debug, PartialEq, Eq)]
struct Roles {
    http: bool,
    grpc: bool,
}

/// Read `METER_ROLES` from the environment (empty/unset → both surfaces).
fn roles_from_env() -> anyhow::Result<Roles> {
    parse_roles(&std::env::var("METER_ROLES").unwrap_or_default())
}

/// Parse a comma-separated role list. Empty selects both `http` and `grpc`; an unknown role or a list
/// that selects nothing is an error (fail fast rather than silently serve nothing).
fn parse_roles(raw: &str) -> anyhow::Result<Roles> {
    if raw.trim().is_empty() {
        return Ok(Roles {
            http: true,
            grpc: true,
        });
    }
    let mut roles = Roles {
        http: false,
        grpc: false,
    };
    for part in raw.split(',') {
        match part.trim() {
            "" => {}
            "http" => roles.http = true,
            "grpc" => roles.grpc = true,
            other => anyhow::bail!("unknown role in METER_ROLES: {other:?} (valid: http, grpc)"),
        }
    }
    if !roles.http && !roles.grpc {
        anyhow::bail!("METER_ROLES selected no roles (valid: http, grpc)");
    }
    Ok(roles)
}

/// The event-ingest idempotency mode from `METER_INGEST_MODE` (`exactly_once` | `append`); defaults to
/// the safe `exactly_once`. See [`IngestMode`] / ADR 0005.
fn ingest_mode_from_env() -> IngestMode {
    match std::env::var("METER_INGEST_MODE").as_deref() {
        Ok("append") => IngestMode::Append,
        _ => IngestMode::ExactlyOnce,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_roles, Roles};

    #[test]
    fn empty_or_unset_serves_both_surfaces() {
        let both = Roles {
            http: true,
            grpc: true,
        };
        assert_eq!(parse_roles("").unwrap(), both);
        assert_eq!(parse_roles("   ").unwrap(), both);
    }

    #[test]
    fn a_single_role_serves_only_that_surface() {
        assert_eq!(
            parse_roles("http").unwrap(),
            Roles {
                http: true,
                grpc: false
            }
        );
        assert_eq!(
            parse_roles(" grpc ").unwrap(),
            Roles {
                http: false,
                grpc: true
            }
        );
    }

    #[test]
    fn both_roles_listed_explicitly_serves_both() {
        assert_eq!(
            parse_roles("grpc,http").unwrap(),
            Roles {
                http: true,
                grpc: true
            }
        );
    }

    #[test]
    fn an_unknown_role_is_rejected() {
        assert!(parse_roles("http,ftp").is_err());
        assert!(parse_roles("nonsense").is_err());
    }
}
