//! The hosted model rate-card catalog (read-only): the curated, best-effort provider prices the
//! engine ships with, so clients can discover model pricing without maintaining it themselves.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use meter_pricing::RateCard;
use meter_ratecards::{catalog, rate_card_for, ModelCatalogEntry, CATALOG_AS_OF};

/// `GET /v1/catalog` response: the snapshot date plus every catalogued model's per-token prices.
#[derive(Serialize)]
pub struct CatalogResponse {
    pub as_of: &'static str,
    pub models: Vec<ModelCatalogEntry>,
}

/// `GET /v1/catalog` — the curated provider rate-card catalog. Prices are best-effort as of `as_of`
/// (no billing-accuracy SLA); verify against the provider before billing.
#[utoipa::path(
    get,
    path = "/v1/catalog",
    responses((status = 200, description = "Snapshot date + every catalogued model's per-token prices")),
    tag = "catalog"
)]
pub async fn list() -> Json<CatalogResponse> {
    Json(CatalogResponse {
        as_of: CATALOG_AS_OF,
        models: catalog(),
    })
}

/// `GET /v1/catalog/{model_id}` — the provider-cost rate card for a catalogued model, ready to price
/// usage against. `404` if the model is not in the catalog.
#[utoipa::path(
    get,
    path = "/v1/catalog/{model_id}",
    params(("model_id" = String, Path, description = "Catalogued model id (e.g. claude-opus-4-8)")),
    responses(
        (status = 200, description = "The provider-cost rate card", body = RateCard),
        (status = 404, description = "Model not in the catalog")
    ),
    tag = "catalog"
)]
pub async fn get_card(Path(model_id): Path<String>) -> Result<Json<RateCard>, StatusCode> {
    rate_card_for(&model_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}
