//! The engine's OpenAPI document (utoipa), served at `GET /openapi.json` for SDK codegen (EPIC 08/11).
//!
//! Coverage is built up endpoint-group by endpoint-group; the served document always reflects exactly
//! the operations annotated with `#[utoipa::path]` and listed in [`ApiDoc`].

use axum::Json;
use utoipa::OpenApi;

use meter_core::{Currency, Money};
use meter_event::{Event, EventStatus};
use meter_ledger::{
    AccountScope, Balance, CreditSource, EntryType, LedgerAccount, LedgerEntry, ReserveOutcome,
};
use meter_pricing::{
    ChargeModel, ContextTier, Margin, Modality, PriceComponent, PriceTier, PricingDimension,
    RateCard, RateCardKind, Unit,
};
use meter_store_ch::{DayUsage as EventDayUsage, ModelUsage};
use meter_store_pg::DayUsage;

use crate::dto::{
    AmendBody, ExtendBody, GrantBody, MeterUsageBody, OpenAccountBody, OpenLeaseBody,
    RecordBatchBody, RecordEventBody, RefundBody, ReserveBody, ReserveUsageBody, SettleBody,
    SettleUsageBody, SimulateBody, UsageDimensions,
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
        super::events::record,
        super::events::record_batch,
        super::events::get,
        super::events::list_for_account,
        super::events::amend,
        super::events::void_run,
        super::usage::meter_usage,
        super::usage::reserve_usage,
        super::usage::settle_usage,
        super::simulate::simulate,
        super::catalog::list,
        super::catalog::get_card,
        super::rate_cards::list,
        super::rate_cards::get,
        super::analytics::usage_by_day,
        super::analytics::usage_by_model,
        super::analytics::org_usage_by_day,
        super::analytics::event_count,
        super::budgets::budget_status,
        super::invoices::invoice,
        super::audit::list,
    ),
    components(schemas(
        OpenAccountBody,
        GrantBody,
        RefundBody,
        ReserveBody,
        SettleBody,
        ExtendBody,
        OpenLeaseBody,
        RecordEventBody,
        RecordBatchBody,
        AmendBody,
        UsageDimensions,
        MeterUsageBody,
        ReserveUsageBody,
        SettleUsageBody,
        SimulateBody,
        LedgerAccount,
        Balance,
        LedgerEntry,
        EntryType,
        CreditSource,
        AccountScope,
        ReserveOutcome,
        Event,
        EventStatus,
        RateCard,
        RateCardKind,
        Margin,
        PriceComponent,
        ChargeModel,
        PriceTier,
        PricingDimension,
        Modality,
        ContextTier,
        Unit,
        Money,
        Currency,
        DayUsage,
        EventDayUsage,
        ModelUsage,
        super::invoices::InvoiceResponse
    )),
    tags(
        (name = "health", description = "Liveness and readiness probes"),
        (name = "accounts", description = "Ledger accounts: open, balance, grants, credit-notes, entries"),
        (name = "reservations", description = "The reserve -> settle/void hold lifecycle"),
        (name = "leases", description = "Per-session credit leases (hot-account mitigation)"),
        (name = "events", description = "Usage events: record, batch, amend (append-only), void-run"),
        (name = "usage", description = "Token-priced metering: charge, reserve/settle, simulate"),
        (name = "catalog", description = "Hosted model rate-card catalog (provider cost)"),
        (name = "rate-cards", description = "Synced (control-plane-configured) rate cards"),
        (name = "analytics", description = "Usage, budget, and invoice reads"),
        (name = "audit", description = "The engine's mutating-request audit log")
    )
)]
pub struct ApiDoc;

/// `GET /openapi.json` — the engine's OpenAPI document.
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
