//! Event-ingest **throughput harness** — the measurement half of the measure→find-limits→improve
//! loop toward provider-scale EPS (ADR 0005 / SLO.md).
//!
//! It drives the real ingest path against a real ClickHouse container and reports events/sec for the
//! single-event baseline (`record`) and the firehose path (`record_batch`) across a batch-size ×
//! concurrency sweep, then measures read latency of the analytics aggregates at the loaded scale (the
//! reads that back credit/usage dashboards must stay fast at hundreds of millions of rows).
//!
//! It is `#[ignore]`d so the normal test run stays fast; run it explicitly (Docker required):
//!
//! ```text
//! cargo test -p meter-store-ch --test throughput -- --ignored --nocapture
//! METER_BENCH_EVENTS=5000000 cargo test -p meter-store-ch --test throughput -- --ignored --nocapture
//! ```

use std::time::Instant;

use meter_core::{AccountId, OrgId};
use meter_event::{EventStore, RecordEvent};
use serde_json::json;
use time::{Duration, OffsetDateTime};

use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

const MODELS: [&str; 4] = [
    "claude-opus-4-8",
    "claude-sonnet-4-6",
    "gpt-5",
    "gemini-2.5-pro",
];

/// A realistic usage event: a rotating model, a day bucket spread over a month, and the token/credit
/// shape the metering path records (so the analytics aggregates have real work to do).
fn bench_event(org: OrgId, account: AccountId, tag: &str, i: u64) -> RecordEvent {
    let model = MODELS[(i % MODELS.len() as u64) as usize];
    let day = OffsetDateTime::UNIX_EPOCH + Duration::days((i % 30) as i64);
    RecordEvent {
        org_id: org,
        idempotency_key: format!("{tag}-{i}"),
        event_time: day,
        meter: "tokens".to_owned(),
        account_id: account,
        run_id: None,
        properties: json!({
            "model": model,
            "input_uncached": 1000u64,
            "cache_read": 0u64,
            "cache_write": 0u64,
            "output": 500u64,
            "reasoning": 0u64,
            "credits": "0.12",
        }),
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Drive `total` distinct events into `account` in batches of `batch`, fanned across `conc` workers.
/// Returns achieved events/sec.
async fn run_load(
    store: &meter_store_ch::ChStore,
    org: OrgId,
    account: AccountId,
    tag: &'static str,
    total: u64,
    batch: u64,
    conc: u64,
) -> f64 {
    let num_batches = total.div_ceil(batch);
    let start = Instant::now();
    let mut handles = Vec::new();
    for worker in 0..conc {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            let mut b = worker;
            while b < num_batches {
                let lo = b * batch;
                let hi = (lo + batch).min(total);
                let reqs: Vec<RecordEvent> = (lo..hi)
                    .map(|i| bench_event(org, account, tag, i))
                    .collect();
                store.record_batch(reqs).await.expect("record_batch");
                b += conc;
            }
        }));
    }
    for h in handles {
        h.await.expect("join");
    }
    total as f64 / start.elapsed().as_secs_f64()
}

#[tokio::test]
#[ignore = "throughput harness; run explicitly with --ignored --nocapture (needs Docker)"]
async fn ingest_throughput() {
    let container = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let port = container.get_host_port_ipv4(8123).await.expect("http port");
    let store = meter_store_ch::ChStore::new(&format!("http://127.0.0.1:{port}"));
    store.migrate().await.expect("migrate");

    let write_n = env_u64("METER_BENCH_EVENTS", 1_000_000);
    let probe_n = env_u64("METER_BENCH_PROBE", 200_000).min(write_n);

    // --- Baseline: one event per insert (the pre-batch hot path). ---
    let (org0, acct0) = (OrgId::new(), AccountId::new());
    let baseline_n = env_u64("METER_BENCH_BASELINE", 2_000);
    let t = Instant::now();
    for i in 0..baseline_n {
        store
            .record(bench_event(org0, acct0, "baseline", i))
            .await
            .expect("record");
    }
    let single_eps = baseline_n as f64 / t.elapsed().as_secs_f64();
    println!("\n=== INGEST THROUGHPUT (events/sec) ===");
    println!("single  record       : {single_eps:>12.0} eps   (n={baseline_n})");

    // --- Sweep: find the best (batch, concurrency) on a smaller probe load. ---
    println!("\nbatch_size  concurrency        eps");
    let batches = [10_000u64, 50_000, 100_000];
    let concs = [4u64, 8, 16];
    let mut best = (0u64, 0u64, 0.0f64);
    for &batch in &batches {
        for &conc in &concs {
            let (org, acct) = (OrgId::new(), AccountId::new());
            let eps = run_load(&store, org, acct, "probe", probe_n, batch, conc).await;
            println!("{batch:>10}  {conc:>11}  {eps:>12.0}");
            if eps > best.2 {
                best = (batch, conc, eps);
            }
        }
    }
    let (best_batch, best_conc, _) = best;
    println!(
        "\nbest probe config: batch={best_batch} conc={best_conc} ({:.0} eps)",
        best.2
    );

    // --- Headline: best config at full scale; this is the number we hold to an SLO. ---
    let (org, acct) = (OrgId::new(), AccountId::new());
    let headline_eps = run_load(&store, org, acct, "scale", write_n, best_batch, best_conc).await;
    println!(
        "\nHEADLINE: {headline_eps:>12.0} eps   (n={write_n}, batch={best_batch}, conc={best_conc})"
    );

    // --- Reads at scale: the aggregates that back credit/usage dashboards. ---
    println!("\n=== READ LATENCY @ {write_n} events ===");
    let t = Instant::now();
    let n = store.event_count(org.as_uuid()).await.expect("event_count");
    println!(
        "event_count       : {:>8.1} ms  -> {n} live events",
        t.elapsed().as_secs_f64() * 1e3
    );
    let t = Instant::now();
    let by_model = store
        .usage_by_model(org.as_uuid())
        .await
        .expect("usage_by_model");
    println!(
        "usage_by_model    : {:>8.1} ms  -> {} rows",
        t.elapsed().as_secs_f64() * 1e3,
        by_model.len()
    );
    let t = Instant::now();
    let by_day = store
        .usage_by_day(org.as_uuid())
        .await
        .expect("usage_by_day");
    println!(
        "usage_by_day      : {:>8.1} ms  -> {} rows",
        t.elapsed().as_secs_f64() * 1e3,
        by_day.len()
    );

    // The batch path must crush the single-event baseline, and idempotency must hold under load.
    assert!(
        headline_eps > single_eps * 10.0,
        "batch must beat single by >10x"
    );
    assert_eq!(n, write_n, "every distinct key counted exactly once");
}
