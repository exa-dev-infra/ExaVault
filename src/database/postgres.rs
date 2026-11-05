use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

use crate::pkg::errors::AppError;

pub async fn create_postgres_pool(database_url: &str) -> Result<PgPool, AppError> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect(database_url)
        .await
        .map_err(AppError::Database)
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| AppError::internal(format!("Database migration failed: {}", e)))?;

    Ok(())
}

pub async fn revert_migration(pool: &PgPool, target_version: i64) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .undo(pool, target_version)
        .await
        .map_err(|e| AppError::internal(format!("Database rollback failed: {}", e)))?;

    Ok(())
}

pub async fn revert_last_migration(pool: &PgPool) -> Result<(), AppError> {
    let migrator = sqlx::migrate!("./migrations");

    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT version FROM _sqlx_migrations WHERE success = TRUE ORDER BY version DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let current_version = match row {
        Some((v,)) => v,
        None => {
            tracing::info!("No applied migrations found to revert.");
            return Ok(());
        }
    };

    let mut versions: Vec<i64> = migrator.iter().map(|m| m.version).collect();
    versions.sort_unstable();

    let target_version = versions
        .iter()
        .copied()
        .rfind(|&v| v < current_version)
        .unwrap_or(0);

    tracing::info!(
        current_version = current_version,
        target_version = target_version,
        "Reverting migration..."
    );

    migrator
        .undo(pool, target_version)
        .await
        .map_err(|e| AppError::internal(format!("Database rollback failed: {}", e)))?;

    tracing::info!(
        reverted_version = current_version,
        now_at_version = target_version,
        "Successfully rolled back migration"
    );

    Ok(())
}
