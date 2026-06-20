//! The hosted model rate-card catalog (read-only): the curated, best-effort provider prices the
//! engine ships with, so clients can discover model pricing without maintaining it themselves.

use axum::Json;
use serde::Serialize;

use meter_ratecards::{catalog, ModelCatalogEntry, CATALOG_AS_OF};

/// `GET /v1/catalog` response: the snapshot date plus every catalogued model's per-token prices.
#[derive(Serialize)]
pub struct CatalogResponse {
    pub as_of: &'static str,
    pub models: Vec<ModelCatalogEntry>,
}

/// `GET /v1/catalog` — the curated provider rate-card catalog. Prices are best-effort as of `as_of`
/// (no billing-accuracy SLA); verify against the provider before billing.
pub async fn list() -> Json<CatalogResponse> {
    Json(CatalogResponse {
        as_of: CATALOG_AS_OF,
        models: catalog(),
    })
}
