//! Conversions between the ledger domain model and its SQL representation.

use rust_decimal::Decimal;
use sqlx::postgres::PgRow;
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

use meter_core::{AccountId, Credit, EntryId};
use meter_ledger::{
    AccountScope, CreditSource, EntryType, LedgerEntry, LedgerError, ReservationId,
};

/// Map a sqlx error into a ledger backend error.
pub(crate) fn be(error: sqlx::Error) -> LedgerError {
    LedgerError::Backend(error.to_string())
}

fn col<'r, T>(row: &'r PgRow, name: &str) -> Result<T, LedgerError>
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    row.try_get::<T, _>(name)
        .map_err(|error| LedgerError::Backend(format!("column {name}: {error}")))
}

pub(crate) fn scope_to_str(scope: AccountScope) -> &'static str {
    match scope {
        AccountScope::Org => "org",
        AccountScope::Team => "team",
        AccountScope::User => "user",
        AccountScope::Product => "product",
        AccountScope::Session => "session",
        AccountScope::Promo => "promo",
        AccountScope::Paid => "paid",
        AccountScope::Budget => "budget",
        AccountScope::Overage => "overage",
        AccountScope::FxClearing => "fx_clearing",
        AccountScope::System => "system",
    }
}

pub(crate) fn entry_type_to_str(entry_type: EntryType) -> &'static str {
    match entry_type {
        EntryType::Grant => "grant",
        EntryType::Usage => "usage",
        EntryType::ReservationHold => "reservation_hold",
        EntryType::Settle => "settle",
        EntryType::PartialReturn => "partial_return",
        EntryType::Void => "void",
        EntryType::Refund => "refund",
        EntryType::Chargeback => "chargeback",
        EntryType::Expiration => "expiration",
        EntryType::Amendment => "amendment",
        EntryType::Fx => "fx",
        EntryType::Sealing => "sealing",
    }
}

fn entry_type_from_str(value: &str) -> Result<EntryType, LedgerError> {
    let entry_type = match value {
        "grant" => EntryType::Grant,
        "usage" => EntryType::Usage,
        "reservation_hold" => EntryType::ReservationHold,
        "settle" => EntryType::Settle,
        "partial_return" => EntryType::PartialReturn,
        "void" => EntryType::Void,
        "refund" => EntryType::Refund,
        "chargeback" => EntryType::Chargeback,
        "expiration" => EntryType::Expiration,
        "amendment" => EntryType::Amendment,
        "fx" => EntryType::Fx,
        "sealing" => EntryType::Sealing,
        other => return Err(LedgerError::Backend(format!("unknown entry_type {other}"))),
    };
    Ok(entry_type)
}

pub(crate) fn source_to_str(source: CreditSource) -> &'static str {
    match source {
        CreditSource::Paid => "paid",
        CreditSource::Promo => "promo",
        CreditSource::Grant => "grant",
    }
}

fn source_from_str(value: &str) -> Result<CreditSource, LedgerError> {
    let source = match value {
        "paid" => CreditSource::Paid,
        "promo" => CreditSource::Promo,
        "grant" => CreditSource::Grant,
        other => return Err(LedgerError::Backend(format!("unknown source {other}"))),
    };
    Ok(source)
}

/// Build a [`LedgerEntry`] from a `ledger_entries` row.
pub(crate) fn entry_from_row(row: &PgRow) -> Result<LedgerEntry, LedgerError> {
    let source: Option<String> = col(row, "source")?;
    let source = source.map(|value| source_from_str(&value)).transpose()?;
    let reverses: Option<Uuid> = col(row, "reverses_entry_id")?;
    let reservation: Option<Uuid> = col(row, "reservation_id")?;
    Ok(LedgerEntry {
        id: EntryId::from_uuid(col::<Uuid>(row, "id")?),
        account_id: AccountId::from_uuid(col::<Uuid>(row, "account_id")?),
        paired_account_id: AccountId::from_uuid(col::<Uuid>(row, "paired_account_id")?),
        entry_type: entry_type_from_str(&col::<String>(row, "entry_type")?)?,
        delta_credits: Credit::from_decimal(col::<Decimal>(row, "delta_credits")?),
        balance_after: Credit::from_decimal(col::<Decimal>(row, "balance_after")?),
        source,
        revenue_recognizable: col::<bool>(row, "revenue_recognizable")?,
        reverses_entry_id: reverses.map(EntryId::from_uuid),
        reservation_id: reservation.map(ReservationId::from_uuid),
        idempotency_key: col::<Option<String>>(row, "idempotency_key")?,
        created_at: col::<OffsetDateTime>(row, "created_at")?,
    })
}
