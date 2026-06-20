//! Integration test for the gRPC `ConfigService`: rate cards and budgets synced into the engine's
//! Postgres config store, then read back through the store to confirm persistence.

use meter_api::grpc::config::ConfigGrpc;
use meter_proto::v1;
use meter_proto::v1::config_service_server::ConfigService;
use meter_store_pg::{PgConfig, PgLedger};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tonic::Request;
use uuid::Uuid;

#[tokio::test]
async fn config_grpc_syncs_rate_cards_and_budgets() {
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
    let store = PgConfig::new(ledger.pool().clone());
    let service = ConfigGrpc::new(PgConfig::new(ledger.pool().clone()));

    // Sync a rate card with an explicit id.
    let card_id = Uuid::now_v7();
    let returned = service
        .put_rate_card(Request::new(v1::PutRateCardRequest {
            card: Some(v1::RateCard {
                id: card_id.to_string(),
                kind: v1::RateCardKind::ProviderCost as i32,
                currency: "USD".to_owned(),
                version: 1,
                margin: "1.3".to_owned(),
                components: vec![v1::PriceComponent {
                    dimension: "output".to_owned(),
                    modality: "text".to_owned(),
                    context_tier: "standard".to_owned(),
                    unit: "token".to_owned(),
                    charge_model: "standard".to_owned(),
                    unit_price: Some(v1::Money {
                        amount: "0.000075".to_owned(),
                        currency: "USD".to_owned(),
                    }),
                }],
            }),
        }))
        .await
        .expect("put_rate_card")
        .into_inner()
        .rate_card_id;
    assert_eq!(returned, card_id.to_string());

    // It persisted: read it back through the store.
    let stored = store
        .latest_rate_card(card_id)
        .await
        .expect("latest")
        .expect("present");
    assert_eq!(stored.version, 1);
    assert_eq!(stored.margin, Decimal::new(13, 1));
    assert_eq!(stored.components[0]["dimension"], "output");

    // A card with no id gets one assigned by the engine.
    let assigned = service
        .put_rate_card(Request::new(v1::PutRateCardRequest {
            card: Some(v1::RateCard {
                id: String::new(),
                kind: v1::RateCardKind::Customer as i32,
                currency: "USD".to_owned(),
                version: 1,
                margin: "1.0".to_owned(),
                components: vec![],
            }),
        }))
        .await
        .expect("put_rate_card auto-id")
        .into_inner()
        .rate_card_id;
    let assigned_id = Uuid::parse_str(&assigned).expect("assigned id is a uuid");
    assert!(store
        .latest_rate_card(assigned_id)
        .await
        .expect("latest")
        .is_some());

    // Sync a budget.
    let account = Uuid::now_v7();
    service
        .set_budget(Request::new(v1::SetBudgetRequest {
            budget: Some(v1::Budget {
                account_id: account.to_string(),
                limit: Some(v1::Credit {
                    amount: "1000".to_owned(),
                }),
                period: "monthly".to_owned(),
            }),
        }))
        .await
        .expect("set_budget");
    let budget = store
        .budget(account)
        .await
        .expect("budget")
        .expect("present");
    assert_eq!(budget.limit_credits, Decimal::from(1000));
    assert_eq!(budget.period, "monthly");

    // A malformed card (a component priced in a different currency than the card) is rejected at sync
    // time with InvalidArgument, and is not persisted.
    let bad = service
        .put_rate_card(Request::new(v1::PutRateCardRequest {
            card: Some(v1::RateCard {
                id: String::new(),
                kind: v1::RateCardKind::ProviderCost as i32,
                currency: "USD".to_owned(),
                version: 1,
                margin: "1.0".to_owned(),
                components: vec![v1::PriceComponent {
                    dimension: "output".to_owned(),
                    modality: "text".to_owned(),
                    context_tier: "standard".to_owned(),
                    unit: "token".to_owned(),
                    charge_model: "standard".to_owned(),
                    unit_price: Some(v1::Money {
                        amount: "0.01".to_owned(),
                        currency: "EUR".to_owned(),
                    }),
                }],
            }),
        }))
        .await;
    assert_eq!(bad.unwrap_err().code(), tonic::Code::InvalidArgument);
}
