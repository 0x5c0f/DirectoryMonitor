use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dm_core::config::AppConfig;
use std::path::Path;
use tracing::info;

mod pipeline;
pub(crate) mod runner;
#[cfg(windows)]
mod windows_service;

#[derive(Parser)]
#[command(
    name = "directory-monitor",
    about = "Cross-platform filesystem monitoring tool",
    version,
    long_about = "A Rust implementation of Directory Monitor - watches directories for file system changes in real-time."
)]
struct Cli {
    /// Path to configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Log level (trace, debug, info, warn, error).
    #[arg(short, long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start monitoring (default if no subcommand).
    Run,
    /// Start monitoring with a web dashboard.
    Serve {
        /// Bind address (host:port). Overrides config file [server] settings.
        #[arg(short, long)]
        bind: Option<String>,
    },
    /// Validate the configuration file.
    Validate,
    /// Take a snapshot of a directory (for outage recovery).
    Snapshot {
        /// Directory to snapshot.
        #[arg(short, long)]
        path: PathBuf,
        /// Output file for the snapshot.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Install as a system service (Windows only).
    #[cfg(windows)]
    InstallService,
    /// Uninstall the system service (Windows only).
    #[cfg(windows)]
    UninstallService,
    /// Run as a Windows service (Windows only).
    #[cfg(windows)]
    RunService,
}

use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration first to get logging settings
    let config = AppConfig::load(&cli.config)
        .with_context(|| format!("Failed to load config from {}", cli.config.display()))?;

    // Initialize logging: CLI --log_level overrides config logging.level
    let level = if cli.log_level != "info" {
        &cli.log_level
    } else {
        &config.logging.level
    };
    init_logging(
        level,
        config.logging.file.as_deref(),
        &config.logging.rotation,
    )?;

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Validate => {
            validate_config(&config)?;
        }
        Commands::Run => {
            if config.server.enabled {
                info!("server.enabled = true, starting with web dashboard");
                runner::run_serve(config, cli.config, &None).await?;
            } else {
                runner::run_monitor(config).await?;
            }
        }
        Commands::Serve { bind } => {
            runner::run_serve(config, cli.config, &bind).await?;
        }
        Commands::Snapshot { path, output } => {
            runner::take_snapshot(&path, &output)?;
        }
        #[cfg(windows)]
        Commands::InstallService => {
            windows_service::install_service(&cli.config)?;
        }
        #[cfg(windows)]
        Commands::UninstallService => {
            windows_service::uninstall_service()?;
        }
        #[cfg(windows)]
        Commands::RunService => {
            windows_service::run_service(config, &cli.config)?;
        }
    }

    Ok(())
}

fn init_logging(level: &str, log_file: Option<&Path>, rotation: &str) -> Result<()> {
    use tracing_appender::rolling;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    if let Some(path) = log_file {
        let parent = path.parent().unwrap_or(Path::new("."));
        let file_name = path.file_name().unwrap_or_default();

        let file_appender = match rotation {
            "daily" => rolling::daily(parent, file_name),
            // tracing-appender 不支持 monthly，回退到 daily
            "monthly" => rolling::daily(parent, file_name),
            _ => rolling::never(parent, file_name),
        };
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        // Leak the guard so it lives for the program's lifetime
        std::mem::forget(guard);

        // File layer
        let file_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(true)
            .with_ansi(false)
            .with_writer(non_blocking);

        // Stdout layer
        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_thread_ids(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .with_thread_ids(true)
            .init();
    }

    Ok(())
}

fn validate_config(config: &AppConfig) -> Result<()> {
    info!("Validating configuration...");

    if config.watches.is_empty() {
        tracing::warn!("No watch directories configured");
    }

    let mut missing_paths = Vec::new();

    for watch in &config.watches {
        if !watch.path.exists() {
            tracing::error!("Watch path does not exist: {}", watch.path.display());
            missing_paths.push(watch.path.display().to_string());
        } else {
            info!("  ✓ {}", watch.path.display());
        }
    }

    if !missing_paths.is_empty() {
        anyhow::bail!("Missing watch paths: {}", missing_paths.join(", "));
    }

    if config.notifications.email.enabled {
        info!(
            "  Email notifications: enabled (SMTP: {})",
            config.notifications.email.smtp_server
        );
    }
    if config.notifications.syslog.enabled {
        info!(
            "  Syslog: enabled ({}:{})",
            config.notifications.syslog.server, config.notifications.syslog.port
        );
    }
    if config.database.enabled {
        info!("  Database: {}", config.database.path.display());
    }

    info!("Configuration valid");
    Ok(())
}
