//! End-to-end HTTP tests: drive the engine API over the wire against a real Postgres.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use meter_api::{router, AppState};
use meter_core::{Currency, Money};
use meter_store_pg::{PgEventStore, PgLedger};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::ContainerAsync;
use tower::ServiceExt;

async fn app() -> (ContainerAsync<Postgres>, Router) {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container
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
    ledger.migrate().await.expect("migrate");
    let events = PgEventStore::new(pool);
    // 1 credit = 1 micro-USD, so credits == COGS in micro-dollars.
    let credit_value = Money::new(Decimal::new(1, 6), Currency::new("USD").expect("usd"));
    (
        container,
        router(AppState::new(ledger, events, credit_value)),
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
