use clap::{Parser, Subcommand};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;

use exavault::{
    build_app,
    config::AppConfig,
    database::postgres::{create_postgres_pool, run_migrations},
    modules::{cli::run_pull_command, mcp::run_mcp_server},
    pkg::{logging::init_logger, realtime::RealtimeHub},
};

#[derive(Parser)]
#[command(name = "exavault", version, about = "ExaVault: Self-hosted Secret & .env Manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the ExaVault HTTP server & Web Dashboard (default)
    Serve,

    /// Pull secrets directly to local .env without opening the browser
    Pull {
        /// ExaVault service token (exv_...)
        #[arg(short, long, env = "EXAVAULT_TOKEN")]
        token: String,

        /// ExaVault server URL
        #[arg(short, long, env = "EXAVAULT_URL", default_value = "http://localhost:3000")]
        url: String,

        /// Format: dotenv, json, shell
        #[arg(short, long, default_value = "dotenv")]
        format: String,

        /// Output file path (defaults to .env)
        #[arg(short, long, default_value = ".env")]
        out: Option<String>,
    },

    /// Run as a Model Context Protocol (MCP) stdio server for AI assistants
    Mcp,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Pull { token, url, format, out } => {
            run_pull_command(&url, &token, &format, out.as_deref()).await?;
            Ok(())
        }

        Commands::Mcp => {
            run_mcp_server().await?;
            Ok(())
        }

        Commands::Serve => {
            let config = AppConfig::from_env();

            // 1. Initialize OpenTelemetry Tracer if enabled
            let (otel_provider, otel_err) = match exavault::pkg::telemetry::init_telemetry(&config) {
                Ok(provider) => (provider, None),
                Err(err) => (None, Some(err)),
            };

            // 2. Initialize structured logging
            init_logger(&config, otel_provider.as_ref());

            if let Some(err) = otel_err {
                tracing::warn!(error = %err, "Failed to initialize OpenTelemetry tracer provider");
            }

            info!(
                host = %config.host,
                port = %config.port,
                "Starting ExaVault server..."
            );

            // 3. Connect to PostgreSQL
            let pool = match create_postgres_pool(&config.database_url).await {
                Ok(pool) => {
                    info!("Database connection pool established successfully");
                    // Run automatic migrations
                    if let Err(e) = run_migrations(&pool).await {
                        tracing::warn!(error = %e, "Migration check failed, continuing with existing schema");
                    } else {
                        info!("Database migrations executed successfully");
                    }

                    // Auto-seed initial administrator account into database if none exists
                    let vault_repo = Arc::new(exavault::modules::vault::repository::VaultRepository::new(pool.clone()));
                    let vault_service = exavault::modules::vault::service::VaultService::new(
                        vault_repo,
                        config.master_key_bytes(),
                    );
                    match vault_service.seed_default_admin(&config.admin_username, &config.admin_password).await {
                        Ok(true) => info!(username = %config.admin_username, "Auto-seeded initial administrator account into database"),
                        Ok(false) => info!("Administrator account already exists in database, skipping auto-seed"),
                        Err(e) => tracing::warn!(error = %e, "Failed to auto-seed initial administrator account"),
                    }

                    pool
                }
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        database_url = %config.database_url,
                        "Failed to connect to database. Please ensure PostgreSQL is running."
                    );
                    return Err(Box::new(err) as Box<dyn std::error::Error>);
                }
            };

            // 4. Initialize Realtime Hub with Distributed Bridge
            let hub = Arc::new(RealtimeHub::default());
            let bridge = exavault::pkg::realtime::create_realtime_bridge(&config, hub.sender()).await;
            hub.set_bridge(bridge).await;

            // 5. Build Axum Router
            let app = build_app(config.clone(), pool, hub);

            // 6. Start listener with graceful shutdown
            let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
            let listener = TcpListener::bind(addr).await?;
            info!("⚡ ExaVault is running on http://{}", addr);

            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            info!("ExaVault server shutdown completed cleanly");
            exavault::pkg::telemetry::shutdown_telemetry();
            Ok(())
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C signal, shutting down...");
        },
        _ = terminate => {
            tracing::info!("Received terminate signal, shutting down...");
        },
    }
}
