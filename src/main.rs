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

use cli::{Cli, Commands, SelfSubcommands, UpdateAction, UpdateArgs};

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
        SelfSubcommands::Update(args) => handle_self_update(args).await,
    }
}

async fn handle_self_update(args: UpdateArgs) -> ExitCode {
    if args.help {
        UpdateArgs::print_help();
        return ExitCode::SUCCESS;
    }

    let client = reqwest::Client::new();

    match args.action {
        Some(UpdateAction::List) => list_versions(&client).await,
        Some(UpdateAction::Check) => check_for_update(&client).await,
        None => perform_update(&client, args.version, args.force, args.skip_verify).await,
    }
}

async fn list_versions(client: &reqwest::Client) -> ExitCode {
    match update::list_versions(client).await {
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
}

async fn check_for_update(client: &reqwest::Client) -> ExitCode {
    match update::check_for_update(client).await {
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
}

async fn perform_update(
    client: &reqwest::Client,
    version: Option<String>,
    force: bool,
    skip_verify: bool,
) -> ExitCode {
    let release = if let Some(v) = version {
        match update::find_version(client, &v).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("{e:?}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        match update::check_for_update(client).await {
            Ok(update::UpdateCheck::Available(r)) => r,
            Ok(update::UpdateCheck::AlreadyUpToDate) => {
                tracing::info!("Already up to date (v{})", update::current_version_string());
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                tracing::error!("{e:?}");
                return ExitCode::FAILURE;
            }
        }
    };

    match update::perform_update(client, &release, force, skip_verify).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e:?}");
            ExitCode::FAILURE
        }
    }
}
