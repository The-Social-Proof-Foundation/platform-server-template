use std::path::{Path, PathBuf};

use platform_core::AppResult;
use sqlx::PgPool;
use tracing::info;

const MIGRATIONS_TABLE: &str = "platform_schema_migrations";

fn normalize_migration_sql(sql: &str) -> String {
    sql.replace("\r\n", "\n")
        .trim()
        .trim_start_matches("BEGIN;")
        .trim_start_matches("begin;")
        .trim_end_matches("COMMIT;")
        .trim_end_matches("commit;")
        .trim()
        .to_string()
}

pub async fn run_migrations(pool: &PgPool, migrations_dir: impl AsRef<Path>) -> AppResult<()> {
    let migrations_dir = migrations_dir.as_ref();
    let mut conn = pool.acquire().await?;

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (
            id SERIAL PRIMARY KEY,
            filename TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"
    ))
    .execute(&mut *conn)
    .await?;

    if !migrations_dir.exists() {
        tracing::warn!("migrations directory not found: {}", migrations_dir.display());
        return Ok(());
    }

    let mut files: Vec<PathBuf> = std::fs::read_dir(migrations_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();

    for path in files {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let applied: Option<(i32,)> = sqlx::query_as(&format!(
            "SELECT 1 FROM {MIGRATIONS_TABLE} WHERE filename = $1"
        ))
        .bind(&filename)
        .fetch_optional(&mut *conn)
        .await?;

        if applied.is_some() {
            continue;
        }

        let raw = std::fs::read_to_string(&path)?;
        let sql = normalize_migration_sql(&raw);
        info!(filename, "applying migration");

        let mut tx = pool.begin().await?;
        sqlx::query(&sql).execute(&mut *tx).await?;
        sqlx::query(&format!(
            "INSERT INTO {MIGRATIONS_TABLE} (filename) VALUES ($1)"
        ))
        .bind(&filename)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }

    Ok(())
}

pub fn default_migrations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}
