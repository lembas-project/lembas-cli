//! Package installation from lockfiles via rattler.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use miette::{Context, IntoDiagnostic};
use rattler::{
    default_cache_dir,
    install::{IndicatifReporter, Installer},
    package_cache::PackageCache,
};
use rattler_conda_types::{Platform, PrefixRecord};
use rattler_lock::LockFile;
use rattler_shell::activation::{ActivationVariables, Activator, PathModificationBehavior};
use rattler_shell::shell::ShellEnum;
use sha2::{Digest, Sha256};

use crate::paths;

const HASH_FILE: &str = ".lockfile-hash";

/// Check if a prefix has been installed.
fn is_installed(prefix: &Path) -> bool {
    prefix.join("conda-meta").exists()
}

/// Compute SHA256 hash of lock content.
fn lock_sha256(lock_content: &str) -> String {
    let hash = Sha256::digest(lock_content.as_bytes());
    format!("{:x}", hash)
}

/// Check if the stored hash matches the current hash.
fn hash_matches(prefix: &Path, current_hash: &str) -> miette::Result<bool> {
    let hash_file = prefix.join(HASH_FILE);
    if !hash_file.exists() {
        return Ok(false);
    }
    let stored_hash = std::fs::read_to_string(&hash_file)
        .into_diagnostic()
        .context("failed to read lockfile hash")?;
    Ok(stored_hash.trim() == current_hash)
}

/// Write the hash to the prefix.
fn write_hash(prefix: &Path, hash: &str) -> miette::Result<()> {
    let hash_file = prefix.join(HASH_FILE);
    std::fs::write(&hash_file, hash)
        .into_diagnostic()
        .context("failed to write lockfile hash")?;
    Ok(())
}

/// Extract version from lockfile for a package.
fn version_from_lock(lock_content: &str, package_name: &str) -> miette::Result<String> {
    for line in lock_content.lines() {
        let line = line.trim();
        if line.starts_with("- conda:") && line.contains(&format!("/{}-", package_name)) {
            if let Some(filename) = line.rsplit('/').next() {
                let parts: Vec<&str> = filename.split('-').collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].to_string());
                }
            }
        }
    }
    Err(miette::miette!(
        "could not find {} version in lockfile",
        package_name
    ))
}

/// Ensure the lembas runtime is installed and up-to-date.
/// Returns the prefix path.
pub async fn ensure_runtime(lock_content: &str) -> miette::Result<PathBuf> {
    let prefix = paths::runtime_prefix();
    let current_hash = lock_sha256(lock_content);

    let needs_install = if !is_installed(&prefix) {
        eprintln!("Installing lembas runtime (first run)...");
        true
    } else if !hash_matches(&prefix, &current_hash)? {
        eprintln!("Updating lembas runtime...");
        true
    } else {
        false
    };

    if needs_install {
        install_from_lockfile(lock_content, &prefix).await?;
        write_hash(&prefix, &current_hash)?;
        let version = version_from_lock(lock_content, "lembas")?;
        eprintln!("Installed lembas v{} to {}", version, prefix.display());
    }

    Ok(prefix)
}

/// Install packages from a lockfile string to a prefix.
pub async fn install_from_lockfile(lock_content: &str, prefix: &Path) -> miette::Result<()> {
    let lock_file = LockFile::from_str_with_base_directory(lock_content, None)
        .into_diagnostic()
        .context("failed to parse lockfile")?;

    let env = lock_file
        .default_environment()
        .ok_or_else(|| miette::miette!("lockfile has no default environment"))?;

    let current_platform = Platform::current();
    let records_by_platform = env
        .conda_repodata_records_by_platform()
        .into_diagnostic()
        .context("failed to extract records from lockfile")?;

    let records = records_by_platform
        .into_iter()
        .find(|(p, _)| p.subdir() == current_platform)
        .map(|(_, records)| records)
        .ok_or_else(|| {
            miette::miette!("lockfile has no records for platform {}", current_platform)
        })?;

    eprintln!(
        "   Lockfile contains {} packages for {}",
        records.len(),
        current_platform
    );

    // Ensure prefix directory exists
    std::fs::create_dir_all(prefix)
        .into_diagnostic()
        .context("failed to create prefix directory")?;

    // Check what's already installed
    let installed = PrefixRecord::collect_from_prefix::<PrefixRecord>(prefix).into_diagnostic()?;

    // Build HTTP client for rattler
    let client = reqwest::Client::builder()
        .no_gzip()
        .user_agent(format!("lembas-cli/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into_diagnostic()
        .context("failed to create download client")?;

    // Ensure cache directory exists
    let cache_dir = default_cache_dir()
        .map_err(|e| miette::miette!("could not determine cache directory: {}", e))?;
    rattler_cache::ensure_cache_dir(&cache_dir)
        .map_err(|e| miette::miette!("could not create cache directory: {}", e))?;

    let package_cache = PackageCache::new(cache_dir.join(rattler_cache::PACKAGE_CACHE_DIR));

    // Run installation
    let start = Instant::now();
    let result = Installer::new()
        .with_download_client(client)
        .with_package_cache(package_cache)
        .with_target_platform(current_platform)
        .with_installed_packages(installed)
        .with_execute_link_scripts(true)
        .with_reporter(IndicatifReporter::builder().finish())
        .install(prefix, records)
        .await
        .into_diagnostic()
        .context("failed to install packages")?;

    if result.transaction.operations.is_empty() {
        eprintln!("   Already up to date");
    } else {
        eprintln!(
            "   Installed {} packages in {:.1}s",
            result.transaction.operations.len(),
            start.elapsed().as_secs_f64()
        );
    }

    Ok(())
}

/// Get environment variables for running commands in the installed prefix.
pub fn activation_env(prefix: &Path) -> miette::Result<HashMap<String, String>> {
    let shell = ShellEnum::default();
    let activator = Activator::from_path(prefix, shell, Platform::current())
        .into_diagnostic()
        .context("failed to create activator")?;

    let current_path = std::env::var("PATH")
        .ok()
        .map(|p| std::env::split_paths(&p).collect());

    let activation_vars = ActivationVariables {
        conda_prefix: None,
        path: current_path,
        path_modification_behavior: PathModificationBehavior::Prepend,
        current_env: std::env::vars().collect(),
    };

    let env = activator
        .run_activation(activation_vars, None)
        .into_diagnostic()
        .context("failed to run activation")?;

    Ok(env)
}
