use chrono::Utc;
use std::{env, fs, path::Path};
use exavault::{
    config::AppConfig,
    database::postgres::{
        create_postgres_pool, revert_last_migration, revert_migration, run_migrations,
    },
    pkg::logging::init_logger,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env();
    init_logger(&config, None);

    let args: Vec<String> = env::args().collect();
    let action = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    match action {
        "up" => {
            println!("🚀 Running all pending database migrations...");
            let pool = create_postgres_pool(&config.database_url).await?;
            run_migrations(&pool).await?;
            println!("✅ Database migrations applied successfully!");
        }
        "down" | "rollback" | "revert" => {
            println!("⏪ Rolling back the latest database migration...");
            let pool = create_postgres_pool(&config.database_url).await?;
            revert_last_migration(&pool).await?;
            println!("✅ Rollback executed successfully!");
        }
        "revert-to" => {
            let target_version = args.get(2).and_then(|v| v.parse::<i64>().ok()).expect(
                "Please provide target version: cargo run --bin migrate -- revert-to <version>",
            );

            println!("⏪ Rolling back to target version {}...", target_version);
            let pool = create_postgres_pool(&config.database_url).await?;
            revert_migration(&pool, target_version).await?;
            println!(
                "✅ Rollback to version {} executed successfully!",
                target_version
            );
        }
        "create" => {
            let name = args
                .get(2)
                .expect("Please specify migration name: cargo run --bin migrate -- create <name>");

            let timestamp = Utc::now().format("%Y%m%d%H%M%S");
            let clean_name = name.replace(' ', "_").to_lowercase();

            let up_filename = format!("{}_{}.up.sql", timestamp, clean_name);
            let down_filename = format!("{}_{}.down.sql", timestamp, clean_name);

            let migrations_dir = Path::new("migrations");
            fs::create_dir_all(migrations_dir)?;

            let up_path = migrations_dir.join(&up_filename);
            let down_path = migrations_dir.join(&down_filename);

            fs::write(
                &up_path,
                format!(
                    "-- Migration UP: {}\n-- Write your SQL migration here\n",
                    clean_name
                ),
            )?;
            fs::write(
                &down_path,
                format!(
                    "-- Migration DOWN (Rollback): {}\n-- Write your SQL rollback here\n",
                    clean_name
                ),
            )?;

            println!("✅ Created reversible migration files:");
            println!("   📄 migrations/{}", up_filename);
            println!("   📄 migrations/{}", down_filename);
        }
        "status" => {
            println!("📊 Checking migration status...");
            let pool = create_postgres_pool(&config.database_url).await?;

            let applied_rows: Vec<(i64, String, chrono::DateTime<Utc>)> = sqlx::query_as(
                "SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY version ASC",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

            if applied_rows.is_empty() {
                println!("No applied migrations recorded yet in _sqlx_migrations table.");
            } else {
                println!("Applied Migrations:");
                for (ver, desc, installed_on) in applied_rows {
                    println!("  ✓ [{}] {} (installed on {})", ver, desc, installed_on);
                }
            }
        }
        _ => {
            println!("Database Migration Tool");
            println!();
            println!("Usage:");
            println!("  cargo run --bin migrate -- up              Run all pending migrations");
            println!("  cargo run --bin migrate -- down            Rollback the last migration");
            println!(
                "  cargo run --bin migrate -- revert-to <ver> Rollback down to specified version"
            );
            println!("  cargo run --bin migrate -- create <name>   Create new reversible migration pair (.up.sql / .down.sql)");
            println!("  cargo run --bin migrate -- status          View applied migrations status");
        }
    }

    Ok(())
}
