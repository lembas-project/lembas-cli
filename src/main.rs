//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.
//!
//! This is a thin Rust wrapper that:
//! 1. Bootstraps a conda environment with lembas-core and pixi on first run
//! 2. Delegates all commands to `python -m lembas` in that environment

use std::env;
use std::process::ExitCode;

use miette::{Context, IntoDiagnostic, Result};
use tracing_subscriber;

mod install;
mod paths;
mod runtime;

const LEMBAS_LOCK: &str = include_str!("../locks/pixi.lock");

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    match run().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let prefix = paths::runtime_prefix();
    let current_hash = runtime::lock_sha256(LEMBAS_LOCK);

    // Check if runtime needs installation or update
    let needs_install = if !install::is_installed(&prefix) {
        eprintln!("Installing lembas runtime (first run)...");
        true
    } else if !runtime::hash_matches(&prefix, &current_hash)? {
        eprintln!("Updating lembas runtime...");
        true
    } else {
        false
    };

    if needs_install {
        install::install_from_lockfile(LEMBAS_LOCK, &prefix).await?;
        runtime::write_hash(&prefix, &current_hash)?;
        let version = runtime::version_from_lock(LEMBAS_LOCK, "lembas")?;
        eprintln!("Installed lembas v{} to {}", version, prefix.display());
    }

    // Get command line args (skip the program name)
    let args: Vec<String> = env::args().skip(1).collect();

    // Build command with activated environment
    let env_vars = install::activation_env(&prefix)?;
    let python = prefix.join("bin").join("python");

    let mut cmd = std::process::Command::new(&python);
    cmd.envs(&env_vars);
    cmd.args(["-m", "lembas"]);
    cmd.args(&args);

    let status = cmd
        .status()
        .into_diagnostic()
        .context("failed to execute lembas")?;

    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(status.code().unwrap_or(1) as u8)
    })
}
