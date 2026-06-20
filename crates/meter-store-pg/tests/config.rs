//! Integration test for the engine-side config store (rate cards + budgets) against real Postgres.

use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

use meter_store_pg::{BudgetRecord, PgConfig, PgLedger, RateCardRecord};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn rate_cards_and_budgets_round_trip() {
    let postgres = Postgres::default().start().await.expect("start postgres");
    let port = postgres
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    let ledger = PgLedger::new(pool);
    ledger.migrate().await.expect("migrate");
    let config = PgConfig::new(ledger.pool().clone());

    let card_id = Uuid::now_v7();
    let v1 = RateCardRecord {
        id: card_id,
        version: 1,
        kind: "provider_cost".to_owned(),
        currency: "USD".to_owned(),
        margin: Decimal::new(100, 2),
        components: json!([{ "dimension": "output", "unit_price": { "amount": "0.000075" } }]),
    };
    config.put_rate_card(&v1).await.expect("put v1");
    assert_eq!(
        config.latest_rate_card(card_id).await.expect("latest"),
        Some(v1.clone())
    );

    // A higher version supersedes; the latest read returns it.
    let v2 = RateCardRecord {
        version: 2,
        margin: Decimal::new(130, 2),
        ..v1.clone()
    };
    config.put_rate_card(&v2).await.expect("put v2");
    let latest = config
        .latest_rate_card(card_id)
        .await
        .expect("latest")
        .unwrap();
    assert_eq!(latest.version, 2);
    assert_eq!(latest.margin, Decimal::new(130, 2));

    // Re-pushing v1 is idempotent and does not change the latest.
    config.put_rate_card(&v1).await.expect("put v1 again");
    assert_eq!(
        config
            .latest_rate_card(card_id)
            .await
            .expect("latest")
            .unwrap()
            .version,
        2
    );

    // An unknown card has no latest.
    assert_eq!(
        config
            .latest_rate_card(Uuid::now_v7())
            .await
            .expect("latest"),
        None
    );

    // Budgets upsert per account.
    let account = Uuid::now_v7();
    config
        .set_budget(&BudgetRecord {
            account_id: account,
            limit_credits: Decimal::from(1000),
            period: "monthly".to_owned(),
        })
        .await
        .expect("set budget");
    config
        .set_budget(&BudgetRecord {
            account_id: account,
            limit_credits: Decimal::from(2000),
            period: "monthly".to_owned(),
        })
        .await
        .expect("update budget");
    let budget = config.budget(account).await.expect("budget").unwrap();
    assert_eq!(budget.limit_credits, Decimal::from(2000));
    assert_eq!(budget.period, "monthly");
    assert_eq!(config.budget(Uuid::now_v7()).await.expect("budget"), None);
}
