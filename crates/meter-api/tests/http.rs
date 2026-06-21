//! End-to-end HTTP tests: drive the engine API over the wire against a real Postgres.

use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use meter_api::{router, AppState};
use meter_core::{Currency, Money};
use meter_store_ch::ChStore;
use meter_store_pg::{BudgetRecord, PgConfig, PgLedger, RateCardRecord};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::clickhouse::ClickHouse;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ContainerAsync;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tower::ServiceExt;

// Bound concurrent container sets so the suite is reliable regardless of the test-thread count
// (spinning a container set per test in full parallel overwhelms the Docker daemon).
static CONTAINER_LIMIT: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(2)));

/// Keeps a test's containers (and its concurrency permit) alive for the test's duration. Money-truth
/// is Postgres; events live in `ClickHouse` (ADR 0003), so the engine needs both.
struct TestApp {
    _permit: OwnedSemaphorePermit,
    _postgres: ContainerAsync<Postgres>,
    _clickhouse: ContainerAsync<ClickHouse>,
}

async fn app() -> (TestApp, Router) {
    let (guard, router, _ledger, _events) = app_with_stores().await;
    (guard, router)
}

/// Like [`app`], but also hands back the stores so a test can seed config / inspect state directly.
async fn app_with_stores() -> (TestApp, Router, PgLedger, ChStore) {
    let permit = CONTAINER_LIMIT
        .clone()
        .acquire_owned()
        .await
        .expect("container semaphore");

    let postgres = Postgres::default().start().await.expect("start postgres");
    let pg_port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{pg_port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("migrate");

    let clickhouse = ClickHouse::default()
        .start()
        .await
        .expect("start clickhouse");
    let ch_port = clickhouse
        .get_host_port_ipv4(8123)
        .await
        .expect("clickhouse port");
    let events = ChStore::new(&format!("http://127.0.0.1:{ch_port}"));
    events.migrate().await.expect("clickhouse migrate");

    // 1 credit = 1 micro-USD, so credits == COGS in micro-dollars.
    let credit_value = Money::new(Decimal::new(1, 6), Currency::new("USD").expect("usd"));
    let guard = TestApp {
        _permit: permit,
        _postgres: postgres,
        _clickhouse: clickhouse,
    };
    let app = router(AppState::new(
        ledger.clone(),
        events.clone(),
        events.clone(),
        credit_value,
    ));
    (guard, app, ledger, events)
}

async fn call(app: &Router, method: &str, uri: &str, body: &Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("build request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, value)
}

#[tokio::test]
async fn full_ledger_flow_over_http() {
    let (_container, app) = app().await;

    // Liveness.
    let (status, body) = call(&app, "GET", "/health", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    // Open an account.
    let (status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({
            "org_id": "11111111-1111-1111-1111-111111111111",
            "scope": "org",
            "no_overdraft": true
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let account_id = account["id"].as_str().expect("account id").to_owned();

    // Grant 100 credits.
    let (status, _entry) = call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "100", "source": "paid" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Balance is 100 / 100.
    let (status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(balance["settled"], "100");
    assert_eq!(balance["held"], "0");

    // Reserve 40.
    let reservation = "22222222-2222-2222-2222-222222222222";
    let (status, outcome) = call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({
            "account": account_id,
            "reservation_id": reservation,
            "amount": "40",
            "limit": "hard"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["outcome"], "allowed");

    // Settle 30; balance becomes 70 / 0.
    let (status, _settle) = call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/settle"),
        &json!({ "actual": "30" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "70");
    assert_eq!(balance["held"], "0");

    // The invoice for the period sums the ledger's settles: enforced == billed.
    let (status, invoice) = call(
        &app,
        "GET",
        &format!(
            "/v1/accounts/{account_id}/invoice?start=2000-01-01T00:00:00Z&end=2100-01-01T00:00:00Z"
        ),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invoice["total_credits"], "30");
    assert_eq!(invoice["entries"], json!(1));

    // Usage-by-day analytics: one day with the 30 credits settled.
    let (status, usage) = call(
        &app,
        "GET",
        &format!(
            "/v1/accounts/{account_id}/usage-by-day?start=2000-01-01T00:00:00Z&end=2100-01-01T00:00:00Z"
        ),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let days = usage.as_array().expect("usage array");
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["total_credits"], "30");
    assert_eq!(days[0]["entry_count"], json!(1));

    // Over-reserving is denied.
    let (status, outcome) = call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({
            "account": account_id,
            "reservation_id": "33333333-3333-3333-3333-333333333333",
            "amount": "1000",
            "limit": "hard"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["outcome"], "denied");
}

#[tokio::test]
async fn event_flow_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let account = "44444444-4444-4444-4444-444444444444";
    let run = "55555555-5555-5555-5555-555555555555";

    // Record an event with arbitrary custom fields.
    let (status, event) = call(
        &app,
        "POST",
        "/v1/events",
        &json!({
            "org_id": org,
            "idempotency_key": "evt-1",
            "meter": "tokens",
            "account": account,
            "run_id": run,
            "properties": { "input": 1200, "output": 340, "model": "claude-opus-4-8" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let event_id = event["id"].as_str().expect("event id").to_owned();
    assert_eq!(event["properties"]["model"], "claude-opus-4-8");
    assert_eq!(event["status"], "recorded");

    // Recording with the same key is idempotent.
    let (_status, again) = call(
        &app,
        "POST",
        "/v1/events",
        &json!({
            "org_id": org,
            "idempotency_key": "evt-1",
            "meter": "tokens",
            "account": account
        }),
    )
    .await;
    assert_eq!(again["id"], event["id"]);

    // Amend the event (append-only edit).
    let (status, amended) = call(
        &app,
        "POST",
        &format!("/v1/events/{event_id}/amend"),
        &json!({ "properties": { "input": 1500, "output": 400 } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(amended["supersedes"], event["id"]);
    assert_eq!(amended["properties"]["input"], json!(1500));

    // The account lists only the latest (recorded) version.
    let (_status, list) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(list.as_array().expect("array").len(), 1);
    assert_eq!(list[0]["id"], amended["id"]);

    // A keyed amend is idempotent: re-issuing it resolves to the same new version (a safe retry), and
    // the account still lists exactly one current event. Amend the current version (not the superseded
    // original).
    let latest_id = amended["id"].as_str().expect("amended id").to_owned();
    let keyed_amend = json!({
        "properties": { "input": 1800, "output": 400 },
        "idempotency_key": "amend-evt-1"
    });
    let (_status, keyed) = call(
        &app,
        "POST",
        &format!("/v1/events/{latest_id}/amend"),
        &keyed_amend,
    )
    .await;
    let (_status, keyed_retry) = call(
        &app,
        "POST",
        &format!("/v1/events/{latest_id}/amend"),
        &keyed_amend,
    )
    .await;
    assert_eq!(
        keyed["id"], keyed_retry["id"],
        "retried amend must be idempotent"
    );
    let (_status, list) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(list.as_array().expect("array").len(), 1);
    assert_eq!(list[0]["id"], keyed["id"]);

    // Voiding the run reverses the run's current event; the list empties. No holds were placed for
    // this run, so the ledger half reverses nothing.
    let (status, voided) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(voided["events_voided"], json!(1));
    assert_eq!(voided["holds_released"], json!(0));
    assert_eq!(voided["charges_refunded"], json!(0));
    let (_status, after) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account}/events"),
        &Value::Null,
    )
    .await;
    assert!(after.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn event_batch_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let account = "66666666-6666-6666-6666-666666666666";

    // Bulk ingest: many events in one round-trip → 202 Accepted with the count.
    let (status, accepted) = call(
        &app,
        "POST",
        "/v1/events/batch",
        &json!({
            "events": [
                { "org_id": org, "idempotency_key": "b-1", "meter": "tokens", "account": account,
                  "properties": { "model": "claude-opus-4-8", "credits": "1" } },
                { "org_id": org, "idempotency_key": "b-2", "meter": "tokens", "account": account,
                  "properties": { "model": "gpt-5", "credits": "2" } },
                { "org_id": org, "idempotency_key": "b-3", "meter": "tokens", "account": account,
                  "properties": { "model": "gemini-2.5-pro", "credits": "3" } }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(accepted["accepted"], json!(3));

    // All three are live; re-sending an overlapping key is idempotent (still three).
    let (_status, list) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(list.as_array().expect("array").len(), 3);

    let (status, accepted) = call(
        &app,
        "POST",
        "/v1/events/batch",
        &json!({
            "events": [
                { "org_id": org, "idempotency_key": "b-3", "meter": "tokens", "account": account,
                  "properties": { "model": "gemini-2.5-pro", "credits": "3" } },
                { "org_id": org, "idempotency_key": "b-4", "meter": "tokens", "account": account,
                  "properties": { "model": "gpt-5", "credits": "4" } }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(accepted["accepted"], json!(2));

    let (_status, list) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(list.as_array().expect("array").len(), 4);
}

#[tokio::test]
async fn usage_metering_over_http() {
    let (_container, app) = app().await;

    // Account funded with 1,000,000 credits ($1 at 1 micro-USD/credit).
    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": "11111111-1111-1111-1111-111111111111", "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Meter 1000 input + 500 output for Claude Opus ($15/$75 per M): COGS $0.0525 → 52500 credits.
    let body = json!({
        "org_id": "11111111-1111-1111-1111-111111111111",
        "account": account_id,
        "model": "claude-opus-4-8",
        "idempotency_key": "run-42",
        "run_id": "55555555-5555-5555-5555-555555555555",
        "usage": { "input_uncached": 1000, "output": 500 }
    });
    let (status, result) = call(&app, "POST", "/v1/usage", &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["charged"], json!(true));
    assert_eq!(result["credits"], "52500");
    assert_eq!(result["cogs_usd"], "0.0525");
    assert_eq!(result["settled"], "947500"); // 1_000_000 − 52_500

    // The usage event was recorded with its custom fields.
    let (_status, events) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(events.as_array().expect("array").len(), 1);
    assert_eq!(events[0]["properties"]["model"], "claude-opus-4-8");

    // Idempotent on the key: re-metering the same run does not double-charge.
    let (_status, again) = call(&app, "POST", "/v1/usage", &body).await;
    assert_eq!(again["settled"], "947500");
    let (_status, events_after) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/events"),
        &Value::Null,
    )
    .await;
    assert_eq!(events_after.as_array().expect("array").len(), 1);
}

#[tokio::test]
async fn lease_flow_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";

    // Parent account funded with 100 credits.
    let (_status, parent) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let parent_id = parent["id"].as_str().expect("parent id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{parent_id}/grants"),
        &json!({ "amount": "100", "source": "paid" }),
    )
    .await;

    // Open a lease for 60: a Session child funded by a conserving transfer from the parent.
    let (status, lease) = call(
        &app,
        "POST",
        "/v1/leases",
        &json!({ "parent": parent_id, "amount": "60" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lease["scope"], "session");
    assert_eq!(lease["parent_id"], parent_id);
    let lease_id = lease["id"].as_str().expect("lease id").to_owned();

    // Credits are conserved: parent 40, lease 60.
    let (_status, parent_balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{parent_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(parent_balance["settled"], "40");
    let (_status, lease_balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{lease_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(lease_balance["settled"], "60");

    // The session reserves 50 against the lease and settles 20 — lease settled becomes 40.
    let reservation = "66666666-6666-6666-6666-666666666666";
    let (_status, outcome) = call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({
            "account": lease_id,
            "reservation_id": reservation,
            "amount": "50",
            "limit": "hard"
        }),
    )
    .await;
    assert_eq!(outcome["outcome"], "allowed");
    call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/settle"),
        &json!({ "actual": "20" }),
    )
    .await;

    // Close the lease: the unused 40 returns to the parent.
    let (status, closed) = call(
        &app,
        "POST",
        &format!("/v1/leases/{lease_id}/close"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(closed["returned"], "40");

    // Conservation holds end to end: parent 80 + 20 spent == the original 100; the lease is drained.
    let (_status, parent_after) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{parent_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(parent_after["settled"], "80");
    let (_status, lease_after) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{lease_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(lease_after["settled"], "0");

    // Over-leasing beyond the parent's available balance is refused (insufficient funds).
    let (status, _err) = call(
        &app,
        "POST",
        "/v1/leases",
        &json!({ "parent": parent_id, "amount": "1000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn org_usage_analytics_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Meter two Opus usage events (1000 input + 500 output each → 52,500 credits each).
    for key in ["run-a", "run-b"] {
        call(
            &app,
            "POST",
            "/v1/usage",
            &json!({
                "org_id": org,
                "account": account_id,
                "model": "claude-opus-4-8",
                "idempotency_key": key,
                "usage": { "input_uncached": 1000, "output": 500 }
            }),
        )
        .await;
    }

    // Usage-by-model (from ClickHouse): one model, two events, summed tokens + credits.
    let (status, by_model) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/usage-by-model"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let models = by_model.as_array().expect("model array");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["model"], "claude-opus-4-8");
    assert_eq!(models[0]["events"], json!(2));
    assert_eq!(models[0]["input_tokens"], json!(2000));
    assert_eq!(models[0]["output_tokens"], json!(1000));
    assert_eq!(models[0]["credits"], json!(105_000.0));

    // Event count (live, recorded events for the org).
    let (status, count) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/event-count"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(count["count"], json!(2));

    // Daily event + credit totals: a single day with both events.
    let (status, by_day) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/usage-by-day"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let days = by_day.as_array().expect("day array");
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["events"], json!(2));
    assert_eq!(days[0]["credits"], json!(105_000.0));
}

#[tokio::test]
async fn budget_status_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let period = "start=2000-01-01T00:00:00Z&end=2100-01-01T00:00:00Z";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Charge 52,500 credits of usage (1000 input + 500 output on Opus).
    call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org,
            "account": account_id,
            "model": "claude-opus-4-8",
            "idempotency_key": "budget-run",
            "usage": { "input_uncached": 1000, "output": 500 }
        }),
    )
    .await;

    // Under 80% of a 100k limit -> ok.
    let (status, ok) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/budget?{period}&limit=100000"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ok["status"], "ok");
    assert_eq!(ok["used_credits"], "52500");
    assert_eq!(ok["remaining_credits"], "47500");

    // >= 80% of a 60k limit -> warning.
    let (_status, warning) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/budget?{period}&limit=60000"),
        &Value::Null,
    )
    .await;
    assert_eq!(warning["status"], "warning");

    // Over a 40k limit -> exceeded.
    let (_status, exceeded) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/budget?{period}&limit=40000"),
        &Value::Null,
    )
    .await;
    assert_eq!(exceeded["status"], "exceeded");
}

#[tokio::test]
async fn audit_log_over_http() {
    let (_container, app) = app().await;

    // A mutating action is audited.
    call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": "11111111-1111-1111-1111-111111111111", "scope": "org" }),
    )
    .await;

    let (status, audit) = call(&app, "GET", "/v1/audit?limit=10", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let entries = audit.as_array().expect("array");
    assert!(!entries.is_empty());
    let latest = &entries[0];
    assert_eq!(latest["method"], "POST");
    assert_eq!(latest["path"], "/v1/accounts");
    assert_eq!(latest["actor"], "system");
    assert_eq!(latest["status"], json!(200));
    // The audit entry carries the request's correlation id (generated when none was supplied).
    assert!(
        !latest["request_id"]
            .as_str()
            .expect("request_id")
            .is_empty(),
        "audit entry should record a request id"
    );

    // Filtering: by method, and by time window.
    let (_status, posts) = call(&app, "GET", "/v1/audit?method=POST", &Value::Null).await;
    assert!(!posts.as_array().expect("array").is_empty());
    let (_status, deletes) = call(&app, "GET", "/v1/audit?method=DELETE", &Value::Null).await;
    assert!(deletes.as_array().expect("array").is_empty());
    let (_status, old) = call(
        &app,
        "GET",
        "/v1/audit?until=2000-01-01T00:00:00Z",
        &Value::Null,
    )
    .await;
    assert!(old.as_array().expect("array").is_empty());
    let (_status, recent) = call(
        &app,
        "GET",
        "/v1/audit?since=2000-01-01T00:00:00Z",
        &Value::Null,
    )
    .await;
    assert!(!recent.as_array().expect("array").is_empty());
}

#[tokio::test]
async fn catalog_over_http() {
    let (_container, app) = app().await;

    let (status, body) = call(&app, "GET", "/v1/catalog", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["as_of"].is_string());
    let models = body["models"].as_array().expect("models array");
    assert!(!models.is_empty());

    // The catalog spans the major providers and prices are serialized as strings (exact decimals).
    let ids: Vec<&str> = models
        .iter()
        .filter_map(|m| m["model_id"].as_str())
        .collect();
    assert!(ids.contains(&"claude-opus-4-8"));
    assert!(ids.contains(&"gpt-5"));
    assert!(ids.contains(&"gemini-2.5-pro"));
    let opus = models
        .iter()
        .find(|m| m["model_id"] == "claude-opus-4-8")
        .expect("opus present");
    assert!(opus["input_per_token"].is_string());
    assert_eq!(opus["provider"], "anthropic");

    // A specific model resolves to a ready-to-use provider-cost rate card.
    let (status, card) = call(&app, "GET", "/v1/catalog/gpt-5", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(card["kind"], "provider_cost");
    assert_eq!(card["components"].as_array().expect("components").len(), 4);

    // An unknown model is a 404.
    let (status, _) = call(&app, "GET", "/v1/catalog/not-a-model", &Value::Null).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn simulate_over_http() {
    let (_container, app) = app().await;

    // Two identical events re-rated from gpt-5 onto the pricier claude-opus-4-8.
    // credit_value = 1 micro-USD, provider-cost cards have no margin.
    // Per event (1000 input + 500 output):
    //   gpt-5 : 1000*1.25e-6 + 500*1e-5 = 0.00625 USD -> 6250 credits
    //   opus  : 1000*1.5e-5 + 500*7.5e-5 = 0.0525 USD -> 52500 credits
    let (status, body) = call(
        &app,
        "POST",
        "/v1/simulate",
        &json!({
            "current_model": "gpt-5",
            "proposed_model": "claude-opus-4-8",
            "events": [
                { "input_uncached": 1000, "output": 500 },
                { "input_uncached": 1000, "output": 500 }
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event_count"], json!(2));
    assert_eq!(body["credits_current"], "12500"); // 2 * 6250
    assert_eq!(body["credits_proposed"], "105000"); // 2 * 52500
    assert_eq!(body["credit_delta"], "92500");

    // An unknown model is a 404.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/simulate",
        &json!({ "current_model": "nope", "proposed_model": "gpt-5", "events": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn model_priced_reserve_and_settle_over_http() {
    let (_container, app) = app().await;

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": "11111111-1111-1111-1111-111111111111", "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "100000", "source": "paid" }),
    )
    .await;

    // Reserve a worst-case estimate priced by the engine (gpt-5: 1000 in + 500 out -> 6250 credits).
    let reservation = "a1a1a1a1-a1a1-a1a1-a1a1-a1a1a1a1a1a1";
    let (status, outcome) = call(
        &app,
        "POST",
        "/v1/usage/reserve",
        &json!({
            "account": account_id,
            "reservation_id": reservation,
            "model": "gpt-5",
            "estimate": { "input_uncached": 1000, "output": 500 },
            "limit": "hard"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(outcome["outcome"], "allowed");
    assert_eq!(outcome["reserved_credits"], "6250");

    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["held"], "6250");

    // Settle the actual usage (1000 in + 300 out -> 4250 credits); the engine prices and charges it.
    let (status, settled) = call(
        &app,
        "POST",
        &format!("/v1/usage/reservations/{reservation}/settle"),
        &json!({ "model": "gpt-5", "actual": { "input_uncached": 1000, "output": 300 } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(settled["credits_charged"], "4250");

    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "95750"); // 100000 - 4250
    assert_eq!(balance["held"], "0");

    // An unknown model is a 404 on the reserve path.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/usage/reserve",
        &json!({ "account": account_id, "reservation_id": "b2b2b2b2-b2b2-b2b2-b2b2-b2b2b2b2b2b2", "model": "nope", "estimate": {}, "limit": "soft" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn readiness_reports_stores_up() {
    let (_container, app) = app().await;
    let (status, body) = call(&app, "GET", "/health/ready", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ledger"], json!(true));
    assert_eq!(body["events"], json!(true));
}

#[tokio::test]
async fn budget_uses_configured_limit() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let period = "start=2000-01-01T00:00:00Z&end=2100-01-01T00:00:00Z";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;
    // 52,500 credits of usage (1000 input + 500 output on Opus).
    call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org,
            "account": account_id,
            "model": "claude-opus-4-8",
            "idempotency_key": "budget-cfg-run",
            "usage": { "input_uncached": 1000, "output": 500 }
        }),
    )
    .await;

    // No limit param and no configured budget -> 422.
    let (status, _) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/budget?{period}"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Configure a 60k budget; the endpoint now uses it without a limit param.
    PgConfig::new(ledger.pool().clone())
        .set_budget(&BudgetRecord {
            account_id: account_id.parse().expect("account uuid"),
            limit_credits: Decimal::from(60_000),
            period: "monthly".to_owned(),
        })
        .await
        .expect("set budget");
    let (status, body) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/budget?{period}"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["limit_credits"], "60000");
    assert_eq!(body["used_credits"], "52500");
    assert_eq!(body["status"], "warning"); // 52500 / 60000 = 0.875 >= 0.8
}

#[tokio::test]
async fn credit_burndown_by_custom_field_and_model() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Synced card: output @ $0.0001/token -> 1000 output = $0.10 = 100000 credits (1 micro-USD/credit).
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // Burnable usage tagged by team: alpha twice, beta once (each 100000 credits).
    for (key, team) in [("u1", "alpha"), ("u2", "beta"), ("u3", "alpha")] {
        call(
            &app,
            "POST",
            "/v1/usage",
            &json!({
                "org_id": org, "account": account_id, "model": "custom-x",
                "idempotency_key": key, "usage": { "output": 1000 },
                "rate_card_id": card_id.to_string(), "tags": { "team": team }
            }),
        )
        .await;
    }
    // A non-burnable raw event tagged alpha (recorded, but no credits).
    call(
        &app,
        "POST",
        "/v1/events",
        &json!({
            "org_id": org, "idempotency_key": "raw-1", "meter": "tokens",
            "account": account_id, "properties": { "team": "alpha" }
        }),
    )
    .await;

    // Flexible credit burndown by the custom field `team`, highest spend first.
    let (status, rows) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/usage-by-field?field=team"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().expect("array");
    assert_eq!(rows.len(), 2);
    // alpha: 2 burnable + 1 non-burnable = 3 events, 200000 credits.
    assert_eq!(rows[0]["dimension"], "alpha");
    assert_eq!(rows[0]["events"], json!(3));
    assert_eq!(rows[0]["credits"], json!(200000.0));
    // beta: 1 burnable event, 100000 credits.
    assert_eq!(rows[1]["dimension"], "beta");
    assert_eq!(rows[1]["events"], json!(1));
    assert_eq!(rows[1]["credits"], json!(100000.0));

    // The same flexible burndown works by `model`; the raw event has no model so it drops out.
    let (_status, by_model) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/usage-by-field?field=model"),
        &Value::Null,
    )
    .await;
    let by_model = by_model.as_array().expect("array");
    assert_eq!(by_model[0]["dimension"], "custom-x");
    assert_eq!(by_model[0]["events"], json!(3));
    assert_eq!(by_model[0]["credits"], json!(300000.0));
}

/// Non-burnable usage is priced and recorded (real cost visibility) but never debits the ledger, and
/// its burndown contribution is zero — so free tiers / internal traffic stay flexible without ever
/// distorting money-truth.
#[tokio::test]
async fn non_burnable_usage_records_cost_but_never_charges() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "22222222-2222-2222-2222-222222222222";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Synced card: output @ $0.0001/token -> 1000 output = 100000 credits.
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // One burnable call (charges 100000) and one non-burnable call (priced 100000, burns 0).
    let (_s1, burnable) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "burn-1", "usage": { "output": 1000 },
            "rate_card_id": card_id.to_string(), "tags": { "team": "alpha" }
        }),
    )
    .await;
    assert_eq!(burnable["credits"], "100000");
    assert_eq!(burnable["priced_credits"], "100000");
    assert_eq!(burnable["charged"], json!(true));

    let (_s2, free) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "free-1", "usage": { "output": 1000 },
            "rate_card_id": card_id.to_string(), "burnable": false,
            "tags": { "team": "alpha" }
        }),
    )
    .await;
    // Recorded with real would-be price, but zero burned and no charge.
    assert_eq!(free["credits"], "0");
    assert_eq!(free["priced_credits"], "100000");
    assert_eq!(free["charged"], json!(false));

    // Ledger truth: only the burnable call moved credits (1000000 grant - 100000 = 900000).
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "900000");
    assert_eq!(balance["held"], "0");

    // Burndown by team sums only burned credits: alpha = 100000 across 2 recorded events.
    let (status, rows) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/usage-by-field?field=team"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = rows.as_array().expect("array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["dimension"], "alpha");
    assert_eq!(rows[0]["events"], json!(2));
    assert_eq!(rows[0]["credits"], json!(100000.0));
}

/// The reconcile endpoint reports zero drift when the pre-aggregated rollup agrees with the event
/// store of record — the healthy steady state after ordinary metering.
#[tokio::test]
async fn reconcile_reports_no_drift_on_consistent_usage() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "33333333-3333-3333-3333-333333333333";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    for key in ["r1", "r2"] {
        call(
            &app,
            "POST",
            "/v1/usage",
            &json!({
                "org_id": org, "account": account_id, "model": "custom-x",
                "idempotency_key": key, "usage": { "output": 1000 },
                "rate_card_id": card_id.to_string()
            }),
        )
        .await;
    }

    let (status, drift) = call(
        &app,
        "GET",
        &format!("/v1/orgs/{org}/reconcile"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(drift.as_array().expect("array").len(), 0, "drift: {drift}");
}

#[tokio::test]
async fn usage_prices_with_a_synced_rate_card() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Sync a custom card: output @ $0.0001/token, no margin.
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // Price with the synced card (model is arbitrary — the card id overrides catalog lookup).
    // 500 output * $0.0001 = $0.05 -> 50000 credits at 1 micro-USD/credit.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "synced-card", "usage": { "output": 500 },
            "rate_card_id": card_id.to_string()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["credits"], "50000");

    // The recorded event captures the pricing provenance (EPIC 04): which synced card + version, so
    // the charge can be re-derived later.
    let event_id = body["event_id"].as_str().expect("event id");
    let (_status, event) = call(&app, "GET", &format!("/v1/events/{event_id}"), &Value::Null).await;
    assert_eq!(event["properties"]["rate_card_id"], card_id.to_string());
    assert_eq!(event["properties"]["rate_card_version"], json!(1));
    assert_eq!(event["properties"]["priced_via"], "synced");

    // An unknown rate_card_id is a 404.
    let (status, _) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "synced-card-2", "usage": { "output": 1 },
            "rate_card_id": uuid::Uuid::now_v7().to_string()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn metrics_endpoint_counts_requests_and_errors() {
    let (_container, app) = app().await;

    // One success and one deliberate 404 (a random, non-existent account — not the nil system account).
    call(&app, "GET", "/health", &Value::Null).await;
    let missing = uuid::Uuid::now_v7();
    let (status, _) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{missing}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // /metrics returns Prometheus text (not JSON), so fetch the raw body.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/metrics")
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect")
        .to_bytes();
    let body = String::from_utf8(bytes.to_vec()).expect("utf8");

    // Two requests counted before this /metrics call (health + the 404), one of them a client error.
    assert!(
        body.contains("meter_http_requests_total 2"),
        "metrics body: {body}"
    );
    assert!(
        body.contains("meter_http_request_errors_total{class=\"client\"} 1"),
        "metrics body: {body}"
    );
}

#[tokio::test]
async fn openapi_document_is_served() {
    let (_container, app) = app().await;
    let (status, doc) = call(&app, "GET", "/openapi.json", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);

    // A real OpenAPI envelope.
    assert!(doc["openapi"]
        .as_str()
        .expect("openapi version")
        .starts_with("3."));
    assert_eq!(doc["info"]["title"], "meter engine API");

    // The accounts + health group is documented with paths and a typed request body.
    assert!(doc["paths"]["/v1/accounts"]["post"].is_object());
    assert!(doc["paths"]["/v1/accounts/{id}/balance"]["get"].is_object());
    assert!(doc["paths"]["/v1/accounts/{id}/credit-notes"]["post"].is_object());
    assert!(doc["paths"]["/health"]["get"].is_object());
    assert!(doc["components"]["schemas"]["OpenAccountBody"].is_object());
    assert!(doc["components"]["schemas"]["GrantBody"].is_object());

    // Reservations + leases group.
    assert!(doc["paths"]["/v1/reservations"]["post"].is_object());
    assert!(doc["paths"]["/v1/reservations/{id}/settle"]["post"].is_object());
    assert!(doc["paths"]["/v1/reservations/{id}/void"]["post"].is_object());
    assert!(doc["paths"]["/v1/reservations/{id}/extend"]["post"].is_object());
    assert!(doc["paths"]["/v1/leases"]["post"].is_object());
    assert!(doc["paths"]["/v1/leases/{id}/close"]["post"].is_object());
    assert!(doc["components"]["schemas"]["ReserveBody"].is_object());
    assert!(doc["components"]["schemas"]["OpenLeaseBody"].is_object());

    // Events group.
    assert!(doc["paths"]["/v1/events"]["post"].is_object());
    assert!(doc["paths"]["/v1/events/batch"]["post"].is_object());
    assert!(doc["paths"]["/v1/events/{id}"]["get"].is_object());
    assert!(doc["paths"]["/v1/events/{id}/amend"]["post"].is_object());
    assert!(doc["paths"]["/v1/runs/{id}/void"]["post"].is_object());
    assert!(doc["components"]["schemas"]["RecordEventBody"].is_object());
    assert!(doc["components"]["schemas"]["AmendBody"].is_object());

    // Usage (token-priced metering) group.
    assert!(doc["paths"]["/v1/usage"]["post"].is_object());
    assert!(doc["paths"]["/v1/usage/reserve"]["post"].is_object());
    assert!(doc["paths"]["/v1/usage/reservations/{id}/settle"]["post"].is_object());
    assert!(doc["paths"]["/v1/simulate"]["post"].is_object());
    assert!(doc["components"]["schemas"]["MeterUsageBody"].is_object());
    assert!(doc["components"]["schemas"]["UsageDimensions"].is_object());

    // Read-side group: catalog, rate-cards, analytics, invoices, budget, audit.
    assert!(doc["paths"]["/v1/catalog"]["get"].is_object());
    assert!(doc["paths"]["/v1/catalog/{model_id}"]["get"].is_object());
    assert!(doc["paths"]["/v1/rate-cards"]["get"].is_object());
    assert!(doc["paths"]["/v1/orgs/{id}/usage-by-model"]["get"].is_object());
    assert!(doc["paths"]["/v1/accounts/{id}/usage-by-day"]["get"].is_object());
    assert!(doc["paths"]["/v1/accounts/{id}/invoice"]["get"].is_object());
    assert!(doc["paths"]["/v1/accounts/{id}/budget"]["get"].is_object());
    assert!(doc["paths"]["/v1/audit"]["get"].is_object());

    // Every documented path carries its HTTP method object — the doc is well-formed.
    let paths = doc["paths"].as_object().expect("paths object");
    assert!(
        paths.len() >= 25,
        "expected full path coverage, got {}",
        paths.len()
    );

    // Response bodies are typed: the ledger domain schemas are present and referenced by $ref.
    assert!(doc["components"]["schemas"]["LedgerAccount"].is_object());
    assert!(doc["components"]["schemas"]["Balance"].is_object());
    assert!(doc["components"]["schemas"]["LedgerEntry"].is_object());
    assert!(doc["components"]["schemas"]["Event"].is_object());
    assert!(doc["components"]["schemas"]["RateCard"].is_object());
    assert!(doc["components"]["schemas"]["PriceComponent"].is_object());
    assert!(doc["components"]["schemas"]["Money"].is_object());
    assert_eq!(
        doc["paths"]["/v1/accounts/{id}/balance"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"]["$ref"],
        "#/components/schemas/Balance"
    );
    assert_eq!(
        doc["paths"]["/v1/catalog/{model_id}"]["get"]["responses"]["200"]["content"]
            ["application/json"]["schema"]["$ref"],
        "#/components/schemas/RateCard"
    );
    assert_eq!(
        doc["paths"]["/v1/events/{id}"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/Event"
    );

    // Analytics view schemas (the two DayUsage types are distinct: per-account vs org event store).
    assert!(doc["components"]["schemas"]["DayUsage"].is_object());
    assert!(doc["components"]["schemas"]["EventDayUsage"].is_object());
    assert!(doc["components"]["schemas"]["ModelUsage"].is_object());

    // Completeness gate: every body-bearing 2xx response carries a typed schema — no opaque objects,
    // so the whole surface is codegen-ready.
    for (path, item) in paths {
        for (method, op) in item.as_object().expect("path item object") {
            for code in ["200", "202"] {
                let response = &op["responses"][code];
                if response.is_object() && response.get("content").is_some() {
                    assert!(
                        response["content"]["application/json"]["schema"].is_object(),
                        "{method} {path} {code} response has no typed schema"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn usage_prices_with_a_package_charge_model() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Sync a card whose output is sold in 1000-token packages at $0.01 each (round up).
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": { "package": { "size": 1000 } },
                "unit_price": { "amount": "0.01", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // 2500 output tokens -> ceil(2500/1000) = 3 packages * $0.01 = $0.03 -> 30000 credits.
    let (status, body) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-pkg",
            "idempotency_key": "pkg-1", "usage": { "output": 2500 },
            "rate_card_id": card_id.to_string()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["credits"], "30000");

    // Exactly one package boundary: 1000 tokens -> 1 package -> 10000 credits.
    let (_status, exact) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-pkg",
            "idempotency_key": "pkg-2", "usage": { "output": 1000 },
            "rate_card_id": card_id.to_string()
        }),
    )
    .await;
    assert_eq!(exact["credits"], "10000");
}

#[tokio::test]
async fn extend_hold_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "100", "source": "paid" }),
    )
    .await;

    let reservation = "77777777-7777-7777-7777-777777777777";
    let (status, _) = call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({
            "account": account_id, "reservation_id": reservation,
            "amount": "40", "limit": "hard", "expires_at": "2100-01-01T00:00:00Z"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Heartbeat: extend the hold's expiry.
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/extend"),
        &json!({ "expires_at": "2101-01-01T00:00:00Z" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // After voiding, the hold can no longer be extended -> 409.
    call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/void"),
        &Value::Null,
    )
    .await;
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/extend"),
        &json!({ "expires_at": "2102-01-01T00:00:00Z" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn responses_carry_a_request_id() {
    let (_container, app) = app().await;

    // A correlation id is generated when the caller doesn't supply one.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert!(response.headers().get("x-request-id").is_some());

    // A caller-supplied id is echoed back for end-to-end correlation.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .header("x-request-id", "corr-123")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.headers().get("x-request-id").unwrap(), "corr-123");
}

#[tokio::test]
async fn synced_rate_card_is_readable() {
    let (_container, app, ledger, _events) = app_with_stores().await;

    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "customer".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::new(13, 1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    let (status, body) = call(
        &app,
        "GET",
        &format!("/v1/rate-cards/{card_id}"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["kind"], "customer");
    assert_eq!(body["version"], json!(1));
    assert_eq!(body["components"].as_array().expect("components").len(), 1);

    // An un-synced id is a 404.
    let unknown = uuid::Uuid::now_v7();
    let (status, _) = call(
        &app,
        "GET",
        &format!("/v1/rate-cards/{unknown}"),
        &Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Sync a v2 of the same card plus a second card; the list returns one (latest) per id.
    let config = PgConfig::new(ledger.pool().clone());
    config
        .put_rate_card(&RateCardRecord {
            version: 2,
            margin: Decimal::new(15, 1),
            ..RateCardRecord {
                id: card_id,
                version: 0,
                kind: "customer".to_owned(),
                currency: "USD".to_owned(),
                margin: Decimal::ZERO,
                components: json!([]),
            }
        })
        .await
        .expect("put v2");
    config
        .put_rate_card(&RateCardRecord {
            id: uuid::Uuid::now_v7(),
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::ONE,
            components: json!([]),
        })
        .await
        .expect("put second");

    let (status, list) = call(&app, "GET", "/v1/rate-cards", &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let cards = list.as_array().expect("array");
    assert_eq!(cards.len(), 2);
    // The first card resolves to its latest version (2).
    let first = cards
        .iter()
        .find(|c| c["id"] == card_id.to_string())
        .expect("first card present");
    assert_eq!(first["version"], json!(2));
}

#[tokio::test]
async fn credit_note_refunds_credits() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "100", "source": "paid" }),
    )
    .await;
    // Reserve + settle 30 -> settled 70.
    let reservation = "abababab-abab-abab-abab-abababababab";
    call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({ "account": account_id, "reservation_id": reservation, "amount": "40", "limit": "hard" }),
    )
    .await;
    let (_status, settle) = call(
        &app,
        "POST",
        &format!("/v1/reservations/{reservation}/settle"),
        &json!({ "actual": "30" }),
    )
    .await;
    let settle_entry = settle["id"].as_str().expect("settle id").to_owned();

    // Credit the 30 back, referencing the settle entry.
    let (status, entry) = call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/credit-notes"),
        &json!({ "amount": "30", "reverses_entry_id": settle_entry }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(entry["entry_type"], "refund");
    assert_eq!(entry["reverses_entry_id"], settle_entry);

    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "100"); // 70 + 30 refunded
}

#[tokio::test]
async fn void_run_reverses_events_and_ledger_over_http() {
    let (_container, app) = app().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let run = "cccccccc-cccc-cccc-cccc-cccccccccccc";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "100", "source": "paid" }),
    )
    .await;

    // An event tagged with the run.
    call(
        &app,
        "POST",
        "/v1/events",
        &json!({
            "org_id": org,
            "idempotency_key": "run-evt-1",
            "meter": "tokens",
            "account": account_id,
            "run_id": run,
            "properties": { "input": 100 }
        }),
    )
    .await;

    // An open hold in the run.
    let open_hold = "dddddddd-dddd-dddd-dddd-dddddddddddd";
    call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({ "account": account_id, "reservation_id": open_hold, "amount": "40", "limit": "hard", "run_id": run }),
    )
    .await;
    // A settled charge in the same run: reserve 30, settle 20 -> settled 80.
    let settled_hold = "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee";
    call(
        &app,
        "POST",
        "/v1/reservations",
        &json!({ "account": account_id, "reservation_id": settled_hold, "amount": "30", "limit": "hard", "run_id": run }),
    )
    .await;
    call(
        &app,
        "POST",
        &format!("/v1/reservations/{settled_hold}/settle"),
        &json!({ "actual": "20" }),
    )
    .await;

    // Kill the run: events voided, the open hold released, the settled charge refunded.
    let (status, voided) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(voided["events_voided"], json!(1));
    assert_eq!(voided["holds_released"], json!(1));
    assert_eq!(voided["charges_refunded"], json!(1));
    assert_eq!(voided["credits_refunded"], "20");

    // Balance fully restored: settled back to 100, nothing held.
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "100");
    assert_eq!(balance["held"], "0");

    // The run's events are gone from the account view.
    let (_status, events) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/events"),
        &Value::Null,
    )
    .await;
    assert!(events.as_array().expect("array").is_empty());

    // Idempotent: voiding again reverses nothing new.
    let (_status, again) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(again["holds_released"], json!(0));
    assert_eq!(again["charges_refunded"], json!(0));
    assert_eq!(again["credits_refunded"], "0");
}

/// The agent-failure path the product cares about: a run does several **direct** `/v1/usage` charges
/// (priced + charged on the spot, no reservation), then fails. Voiding the run must negate that
/// billing — refund every direct charge and void the events — restoring the balance exactly.
#[tokio::test]
async fn void_run_reverses_direct_usage_charges_over_http() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let run = "cccccccc-cccc-cccc-cccc-cccccccccccc";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Synced card: output @ $0.0001/token -> 1000 output = 100000 credits.
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // The run meters twice (direct charges of 100000 each) — settled drops to 800000.
    for key in ["u1", "u2"] {
        let (status, _body) = call(
            &app,
            "POST",
            "/v1/usage",
            &json!({
                "org_id": org, "account": account_id, "model": "custom-x",
                "idempotency_key": key, "usage": { "output": 1000 },
                "rate_card_id": card_id.to_string(), "run_id": run
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "800000");

    // Kill the failed run: its events are voided AND its direct charges refunded.
    let (status, voided) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(voided["events_voided"], json!(2));
    assert_eq!(voided["charges_refunded"], json!(2));
    assert_eq!(voided["credits_refunded"], "200000");

    // Billing fully negated: balance back to the original grant.
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "1000000");
    assert_eq!(balance["held"], "0");

    // Idempotent: voiding again refunds nothing new.
    let (_status, again) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(again["charges_refunded"], json!(0));
    assert_eq!(again["credits_refunded"], "0");
}

/// Amending a usage event re-prices in the engine and posts the ledger delta: an up-amend charges the
/// difference, a down-amend refunds it, the amend is idempotent on its key, and voiding the run still
/// restores the balance exactly — proving the reverse-and-recharge chain stays conservation-correct.
#[tokio::test]
async fn amend_usage_reprices_posts_delta_and_void_restores() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "11111111-1111-1111-1111-111111111111";
    let run = "cccccccc-cccc-cccc-cccc-cccccccccccc";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;

    // Synced card: output @ $0.0001/token -> 1000 output = 100000 credits.
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // Meter 1000 output (charge 100000). settled 900000.
    let (_status, metered) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "u1", "usage": { "output": 1000 },
            "rate_card_id": card_id.to_string(), "run_id": run
        }),
    )
    .await;
    let e1 = metered["event_id"].as_str().expect("event id").to_owned();
    assert_eq!(metered["credits"], "100000");

    // Amend UP to 2000 output (200000): delta +100000, settled 800000.
    let up = json!({ "usage": { "output": 2000 }, "idempotency_key": "am1" });
    let (status, a1) = call(&app, "POST", &format!("/v1/usage/{e1}/amend"), &up).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(a1["credits"], "200000");
    assert_eq!(a1["delta"], "100000");
    assert_eq!(a1["settled"], "800000");
    let e2 = a1["event_id"].as_str().expect("amended id").to_owned();
    assert_ne!(e2, e1);

    // Idempotent: replaying the same amend returns the same version and the same balance.
    let (_status, a1b) = call(&app, "POST", &format!("/v1/usage/{e1}/amend"), &up).await;
    assert_eq!(a1b["event_id"], a1["event_id"]);
    assert_eq!(a1b["settled"], "800000");

    // Amend the current version DOWN to 500 output (50000): delta -150000, settled 950000.
    let (_status, a2) = call(
        &app,
        "POST",
        &format!("/v1/usage/{e2}/amend"),
        &json!({ "usage": { "output": 500 }, "idempotency_key": "am2" }),
    )
    .await;
    assert_eq!(a2["credits"], "50000");
    assert_eq!(a2["delta"], "-150000");
    assert_eq!(a2["settled"], "950000");

    // Void the run: every charge in the amend chain nets out -> balance fully restored.
    let (status, _voided) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "1000000");
    assert_eq!(balance["held"], "0");
}

/// Amending a voided run's event is refused — a void already reversed its billing; re-charging a dead
/// run must never happen.
#[tokio::test]
async fn amend_usage_is_refused_after_void() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "22222222-2222-2222-2222-222222222222";
    let run = "dddddddd-dddd-dddd-dddd-dddddddddddd";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");
    let (_status, metered) = call(
        &app,
        "POST",
        "/v1/usage",
        &json!({
            "org_id": org, "account": account_id, "model": "custom-x",
            "idempotency_key": "v1", "usage": { "output": 1000 },
            "rate_card_id": card_id.to_string(), "run_id": run
        }),
    )
    .await;
    let e1 = metered["event_id"].as_str().expect("event id").to_owned();
    call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;

    let (status, _body) = call(
        &app,
        "POST",
        &format!("/v1/usage/{e1}/amend"),
        &json!({ "usage": { "output": 5000 }, "idempotency_key": "after-void" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // The void's restoration stands; no re-charge happened.
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "1000000");
}

/// Two different events amended with the SAME caller idempotency_key on one account must NOT collide:
/// each amendment's ledger postings are keyed by the (unique) amended event id, so both take effect.
/// (Regression for the adversarial-review finding that raw-key-scoped postings aliased across events.)
#[tokio::test]
async fn amend_with_shared_key_across_events_does_not_collide() {
    let (_container, app, ledger, _events) = app_with_stores().await;
    let org = "55555555-5555-5555-5555-555555555555";

    let (_status, account) = call(
        &app,
        "POST",
        "/v1/accounts",
        &json!({ "org_id": org, "scope": "org", "no_overdraft": true }),
    )
    .await;
    let account_id = account["id"].as_str().expect("account id").to_owned();
    call(
        &app,
        "POST",
        &format!("/v1/accounts/{account_id}/grants"),
        &json!({ "amount": "1000000", "source": "paid" }),
    )
    .await;
    let card_id = uuid::Uuid::now_v7();
    PgConfig::new(ledger.pool().clone())
        .put_rate_card(&RateCardRecord {
            id: card_id,
            version: 1,
            kind: "provider_cost".to_owned(),
            currency: "USD".to_owned(),
            margin: Decimal::from(1),
            components: json!([{
                "dimension": "output", "modality": "text", "context_tier": "standard",
                "unit": "token", "charge_model": "standard",
                "unit_price": { "amount": "0.0001", "currency": "USD" }
            }]),
        })
        .await
        .expect("put card");

    // Two independent metered events (each charges 100000). settled 800000.
    let mut event_ids = Vec::new();
    for key in ["e-a", "e-b"] {
        let (_s, m) = call(
            &app,
            "POST",
            "/v1/usage",
            &json!({
                "org_id": org, "account": account_id, "model": "custom-x",
                "idempotency_key": key, "usage": { "output": 1000 },
                "rate_card_id": card_id.to_string()
            }),
        )
        .await;
        event_ids.push(m["event_id"].as_str().expect("event id").to_owned());
    }

    // Amend BOTH with the same caller idempotency_key "shared" but to different amounts.
    let (_s, a1) = call(
        &app,
        "POST",
        &format!("/v1/usage/{}/amend", event_ids[0]),
        &json!({ "usage": { "output": 2000 }, "idempotency_key": "shared" }),
    )
    .await;
    assert_eq!(a1["credits"], "200000");
    let (_s, a2) = call(
        &app,
        "POST",
        &format!("/v1/usage/{}/amend", event_ids[1]),
        &json!({ "usage": { "output": 3000 }, "idempotency_key": "shared" }),
    )
    .await;
    assert_eq!(a2["credits"], "300000");

    // Both amendments applied: 1000000 − 200000 − 300000 = 500000 (NOT 700000, which a collision would
    // leave by reusing the first amendment's postings for the second).
    let (_status, balance) = call(
        &app,
        "GET",
        &format!("/v1/accounts/{account_id}/balance"),
        &Value::Null,
    )
    .await;
    assert_eq!(balance["settled"], "500000");
}
