//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.
//!
//! This is a thin Rust wrapper that:
//! 1. Bootstraps a conda environment with lembas-core and pixi on first run
//! 2. Delegates all commands to `python -m lembas` in that environment

#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::env;
use std::process::ExitCode;

mod install;
mod paths;
mod runtime;
mod update;

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

    // Handle `self update` locally (no bootstrap needed)
    if args.first().map(|s| s.as_str()) == Some("self") {
        return handle_self_command(&args[1..]).await;
    }

    // Delegate to the managed lembas Python runtime
    match runtime::run_lembas(&args).await {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

async fn handle_self_command(args: &[String]) -> ExitCode {
    let subcommand = args.first().map(|s| s.as_str());

    match subcommand {
        Some("update") => handle_self_update(&args[1..]).await,
        Some(other) => {
            tracing::error!("Unknown self command: {}", other);
            tracing::info!("Available: self update");
            ExitCode::FAILURE
        }
        None => {
            tracing::info!("Usage: lembas self <command>");
            tracing::info!("Commands:");
            tracing::info!("  update    Update the lembas CLI to the latest version");
            ExitCode::SUCCESS
        }
    }
}

async fn handle_self_update(args: &[String]) -> ExitCode {
    let client = reqwest::Client::new();

    // Parse flags
    let check_only = args.iter().any(|a| a == "--check");
    let list_only = args.iter().any(|a| a == "--list");
    let force = args.iter().any(|a| a == "--force");
    let version_arg: Option<&str> = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str());

    if list_only {
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
    } else if check_only {
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
    } else if let Some(version) = version_arg {
        // Update to specific version
        match update::find_version(&client, version).await {
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
        // Update to latest
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
