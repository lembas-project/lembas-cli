//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.
//!
//! This is a thin Rust wrapper that:
//! 1. Bootstraps a conda environment with lembas-core and pixi on first run
//! 2. Delegates all commands to `python -m lembas` in that environment

use std::env;
use std::process::ExitCode;

use conda_ship::fleet::{Fleet, InstallOptions, RuntimeSpec};
use miette::{Context, IntoDiagnostic, Result};
use tracing_subscriber;

mod paths;
mod runtime;

const LEMBAS_LOCK: &str = include_str!("../locks/lembas.lock");

#[tokio::main]
async fn main() -> ExitCode {
    // Initialize logging
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
    // Ensure lembas runtime is installed
    let runtime = ensure_runtime().await?;

    // Get command line args (skip the program name)
    let args: Vec<String> = env::args().skip(1).collect();

    // Build command from RuntimeCommand data
    let runtime_cmd = runtime.command("python")?;
    let mut cmd = std::process::Command::new(&runtime_cmd.executable);

    // Set environment variables from activation
    for (key, value) in &runtime_cmd.env {
        cmd.env(key, value);
    }

    // Prepend runtime paths to PATH
    let mut path_dirs = runtime_cmd.path_entries.clone();
    if let Some(existing_path) = env::var_os("PATH") {
        path_dirs.extend(env::split_paths(&existing_path));
    }
    cmd.env("PATH", env::join_paths(&path_dirs).into_diagnostic()?);

    // Delegate to python -m lembas
    let status = cmd
        .args(["-m", "lembas"])
        .args(&args)
        .status()
        .into_diagnostic()
        .context("failed to execute lembas")?;

    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(status.code().unwrap_or(1) as u8)
    })
}

/// Ensure the lembas runtime is installed, installing if needed.
async fn ensure_runtime() -> Result<conda_ship::fleet::InstalledRuntime> {
    let fleet = Fleet::new(paths::lembas_home().join("runtime"));

    // Check if already installed and up-to-date
    if let Some(installed) = fleet.status("lembas")? {
        let current_hash = runtime::lock_sha256(LEMBAS_LOCK);
        if installed.lock_sha256.as_deref() == Some(current_hash.as_str()) {
            return Ok(installed);
        }
        eprintln!("Updating lembas runtime...");
    } else {
        eprintln!("Installing lembas runtime (first run)...");
    }

    // Install or update
    let spec = RuntimeSpec {
        id: "lembas".to_string(),
        version: runtime::version_from_lock(LEMBAS_LOCK, "lembas")?,
        delegate_executable: "python".to_string(),
        lock_content: LEMBAS_LOCK.to_string(),
        requested_specs: vec!["python".to_string(), "lembas".to_string(), "pixi".to_string()],
    };

    let installed = fleet
        .install(spec, InstallOptions::default())
        .await
        .context("failed to install lembas runtime")?;

    eprintln!(
        "Installed lembas v{} to {}",
        installed.version,
        installed.prefix.display()
    );

    Ok(installed)
}
