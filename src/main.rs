//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.
//!
//! This is a thin Rust wrapper that:
//! 1. Bootstraps a conda environment with lembas-core and pixi on first run
//! 2. Delegates all commands to `python -m lembas` in that environment

#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::env;
use std::process::ExitCode;

use clap::Parser;

mod cli;
mod install;
mod paths;
mod runtime;
mod update;

use cli::{Cli, SelfCommands, SelfSubcommands};

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

    // Extract the args (ignoring the program name/entrypoint)
    let args: Vec<String> = env::args().skip(1).collect();

    // Handle --version locally (fast, no bootstrap needed)
    if args.first().map(|s| s.as_str()) == Some("--version") {
        let lembas_ver = runtime::lembas_version().unwrap_or_else(|| "unknown".to_string());
        let cli_ver = cli_version();
        tracing::info!("lembas {} (cli build {})", lembas_ver, cli_ver);
        return ExitCode::SUCCESS;
    }

    // Handle `self` commands via clap (no bootstrap needed)
    if args.first().map(|s| s.as_str()) == Some("self") {
        // Re-parse with clap for the self command
        let cli = match Cli::try_parse() {
            Ok(cli) => cli,
            Err(e) => {
                e.print().ok();
                return if e.kind() == clap::error::ErrorKind::DisplayHelp
                    || e.kind() == clap::error::ErrorKind::DisplayVersion
                {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
            }
        };

        return match cli.command {
            SelfCommands::SelfCmd { command } => handle_self_command(command).await,
        };
    }

    // Delegate to the managed lembas Python runtime
    let result = match runtime::run_lembas(&args).await {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    };

    // Append CLI-only commands to help output
    if args.first().map(|s| s.as_str()) == Some("--help") || args.is_empty() {
        tracing::info!("To update, run `lembas self update`");
    }

    result
}

async fn handle_self_command(command: SelfSubcommands) -> ExitCode {
    match command {
        SelfSubcommands::Update {
            version,
            check,
            list,
            force,
        } => handle_self_update(version, check, list, force).await,
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
