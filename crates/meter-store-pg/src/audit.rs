//! Append-only audit log of mutating actions.

use serde::Serialize;
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

/// An audit-log backed by PostgreSQL.
#[derive(Debug, Clone)]
pub struct PgAuditLog {
    pool: PgPool,
}

/// One recorded action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub actor: String,
    pub method: String,
    pub path: String,
    pub status: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl PgAuditLog {
    /// Wrap a connection pool (shares the engine database).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append an audit entry.
    pub async fn record(
        &self,
        actor: &str,
        method: &str,
        path: &str,
        status: i32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO audit_log (id, actor, method, path, status) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(actor)
        .bind(method)
        .bind(path)
        .bind(status)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The most recent audit entries (newest first).
    pub async fn list(&self, limit: i64) -> Result<Vec<AuditEntry>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, actor, method, path, status, created_at FROM audit_log \
             ORDER BY created_at DESC, id DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_entry).collect()
    }
}

fn row_to_entry(row: &PgRow) -> Result<AuditEntry, sqlx::Error> {
    Ok(AuditEntry {
        id: row.try_get::<Uuid, _>("id")?.to_string(),
        actor: row.try_get("actor")?,
        method: row.try_get("method")?,
        path: row.try_get("path")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}
