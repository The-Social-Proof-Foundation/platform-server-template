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

/// Strip SQL comments so semicolons inside comments do not split statements.
pub fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_single_quote = false;

    while let Some(c) = chars.next() {
        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                out.push(c);
            }
            continue;
        }
        if in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_single_quote {
            out.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    out.push(chars.next().unwrap());
                } else {
                    in_single_quote = false;
                }
            }
            continue;
        }

        if c == '-' && chars.peek() == Some(&'-') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        if c == '\'' {
            in_single_quote = true;
            out.push(c);
            continue;
        }
        out.push(c);
    }
    out
}

fn split_migration_statements(sql: &str) -> Vec<String> {
    let stripped = strip_sql_comments(sql);
    stripped
        .split(';')
        .map(str::trim)
        .filter(|statement| {
            !statement.is_empty()
                && !statement
                    .lines()
                    .all(|line| line.trim().is_empty() || line.trim().starts_with("--"))
        })
        .map(|statement| statement.to_string())
        .collect()
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
        for statement in split_migration_statements(&sql) {
            sqlx::query(&statement).execute(&mut *tx).await?;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_preserves_semicolons_in_comments() {
        let sql = "-- note; with semicolon\nCREATE TABLE t (id INT);";
        let stripped = strip_sql_comments(sql);
        assert!(stripped.contains("CREATE TABLE t (id INT)"));
        assert!(!stripped.contains("note"));
        let statements = split_migration_statements(sql);
        assert_eq!(statements.len(), 1);
        assert!(statements[0].starts_with("CREATE TABLE"));
    }

    #[test]
    fn split_multiple_statements() {
        let sql = "CREATE TABLE a (id INT); CREATE TABLE b (id INT);";
        let statements = split_migration_statements(sql);
        assert_eq!(statements.len(), 2);
    }
}
