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
        let cli_ver = env!("CARGO_PKG_VERSION");
        tracing::info!("lembas {} (cli build {})", lembas_ver, cli_ver);
        return ExitCode::SUCCESS;
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
