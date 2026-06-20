//! End-to-end HTTP tests: drive the engine API over the wire against a real Postgres.

use std::sync::{Arc, LazyLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use meter_api::{router, AppState};
use meter_core::{Currency, Money};
use meter_store_ch::ChStore;
use meter_store_pg::PgLedger;
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
    (
        guard,
        router(AppState::new(ledger, events.clone(), events, credit_value)),
    )
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

    // Voiding the run reverses the run's current event; the list empties.
    let (status, voided) = call(&app, "POST", &format!("/v1/runs/{run}/void"), &Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(voided["voided"], json!(1));
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
