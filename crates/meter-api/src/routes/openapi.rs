//! The engine's OpenAPI document (utoipa), served at `GET /openapi.json` for SDK codegen (EPIC 08/11).
//!
//! Coverage is built up endpoint-group by endpoint-group; the served document always reflects exactly
//! the operations annotated with `#[utoipa::path]` and listed in [`ApiDoc`].

use axum::Json;
use utoipa::OpenApi;

use crate::dto::{
    ExtendBody, GrantBody, OpenAccountBody, OpenLeaseBody, RefundBody, ReserveBody, SettleBody,
};

/// The engine's OpenAPI 3.1 description. The version tracks the crate version.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "meter engine API",
        description = "The metering engine's HTTP surface — money-truth and usage. The source of truth for engine SDK codegen.",
        license(name = "AGPL-3.0-only")
    ),
    paths(
        super::health::health,
        super::health::ready,
        super::accounts::open_account,
        super::accounts::balance,
        super::accounts::grant,
        super::accounts::credit_note,
        super::accounts::entries,
        super::reservations::reserve,
        super::reservations::settle,
        super::reservations::void,
        super::reservations::extend,
        super::leases::open_lease,
        super::leases::close_lease,
    ),
    components(schemas(
        OpenAccountBody,
        GrantBody,
        RefundBody,
        ReserveBody,
        SettleBody,
        ExtendBody,
        OpenLeaseBody
    )),
    tags(
        (name = "health", description = "Liveness and readiness probes"),
        (name = "accounts", description = "Ledger accounts: open, balance, grants, credit-notes, entries"),
        (name = "reservations", description = "The reserve -> settle/void hold lifecycle"),
        (name = "leases", description = "Per-session credit leases (hot-account mitigation)")
    )
)]
pub struct ApiDoc;

/// `GET /openapi.json` — the engine's OpenAPI document.
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
