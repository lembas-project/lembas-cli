//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.
//!
//! This is a thin Rust wrapper that:
//! 1. Bootstraps a conda environment with lembas-core and pixi on first run
//! 2. Delegates all commands to `python -m lembas` in that environment

#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::process::ExitCode;

use clap::Parser;

mod cli;
mod install;
mod paths;
mod runtime;
mod update;

use cli::{Cli, Commands, SelfSubcommands};

/// Get CLI version from build-time git describe (like setuptools-scm).
fn cli_version() -> &'static str {
    env!("LEMBAS_CLI_VERSION")
}

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging (INFO by default, overridable via RUST_LOG)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .with_level(false)
        .init();

    let cli = Cli::parse();

    // Handle --version locally (fast, no bootstrap needed)
    if cli.version {
        let lembas_ver = runtime::lembas_version().unwrap_or_else(|| "unknown".to_string());
        let cli_ver = cli_version();
        tracing::info!("lembas {} (cli build {})", lembas_ver, cli_ver);
        return ExitCode::SUCCESS;
    }

    match cli.command {
        Some(Commands::SelfCmd { command }) => handle_self_command(command).await,
        Some(Commands::External(args)) => run_python_lembas(&args).await,
        None => {
            // No command: delegate to Python (handles --help and bare invocation)
            let args: Vec<String> = std::env::args().skip(1).collect();
            let result = run_python_lembas(&args).await;
            // Append update hint to help output
            if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
                tracing::info!("To update, run `lembas self update`");
            }
            result
        }
    }
}

async fn run_python_lembas(args: &[String]) -> ExitCode {
    match runtime::run_lembas(args).await {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

async fn handle_self_command(command: SelfSubcommands) -> ExitCode {
    match command {
        SelfSubcommands::Update {
            version,
            check,
            list,
            force,
            help,
        } => {
            if help {
                tracing::info!("Update the lembas CLI to the latest version");
                tracing::info!("");
                tracing::info!("Usage: lembas self update [OPTIONS] [VERSION]");
                tracing::info!("");
                tracing::info!("Arguments:");
                tracing::info!("  [VERSION]  Version to install (e.g., v2026.7.1)");
                tracing::info!("");
                tracing::info!("Options:");
                tracing::info!("      --check  Check if an update is available");
                tracing::info!("      --list   List available versions");
                tracing::info!("      --force  Force reinstall even if already on target version");
                tracing::info!("  -h, --help   Print help");
                return ExitCode::SUCCESS;
            }
            handle_self_update(version, check, list, force).await
        }
    }
}

async fn handle_self_update(
    version: Option<String>,
    check: bool,
    list: bool,
    force: bool,
) -> ExitCode {
    let client = reqwest::Client::new();

    if list {
        match update::list_versions(&client).await {
            Ok(releases) => {
                let current = update::current_version_string();
                tracing::info!("Available versions:");
                for r in releases.iter().take(10) {
                    let marker = if r.version.to_string() == current {
                        " (installed)"
                    } else {
                        ""
                    };
                    tracing::info!("  v{}{}", r.version, marker);
                }
                if releases.len() > 10 {
                    tracing::info!("  ... and {} more", releases.len() - 10);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                tracing::error!("{e:?}");
                ExitCode::FAILURE
            }
        }
    } else if check {
        match update::check_for_update(&client).await {
            Ok(update::UpdateCheck::Available(release)) => {
                tracing::info!(
                    "Update available: v{} -> v{}",
                    update::current_version_string(),
                    release.version
                );
                tracing::info!("Run `lembas self update` to install");
                ExitCode::SUCCESS
            }
            Ok(update::UpdateCheck::AlreadyUpToDate) => {
                tracing::info!("Already up to date (v{})", update::current_version_string());
                ExitCode::SUCCESS
            }
            Err(e) => {
                tracing::error!("{e:?}");
                ExitCode::FAILURE
            }
        }
    } else if let Some(version) = version {
        match update::find_version(&client, &version).await {
            Ok(release) => match update::perform_update(&client, &release, force).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    tracing::error!("{e:?}");
                    ExitCode::FAILURE
                }
            },
            Err(e) => {
                tracing::error!("{e:?}");
                ExitCode::FAILURE
            }
        }
    } else {
        match update::check_for_update(&client).await {
            Ok(update::UpdateCheck::Available(release)) => {
                match update::perform_update(&client, &release, force).await {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(e) => {
                        tracing::error!("{e:?}");
                        ExitCode::FAILURE
                    }
                }
            }
            Ok(update::UpdateCheck::AlreadyUpToDate) => {
                tracing::info!("Already up to date (v{})", update::current_version_string());
                ExitCode::SUCCESS
            }
            Err(e) => {
                tracing::error!("{e:?}");
                ExitCode::FAILURE
            }
        }
    }
}
