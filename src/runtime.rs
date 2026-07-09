//! Runtime lifecycle management.

use std::path::{Path, PathBuf};

use miette::{Context, IntoDiagnostic};
use sha2::{Digest, Sha256};

use crate::install;
use crate::paths;

const LOCKFILE: &str = include_str!("../locks/pixi.lock");
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
pub async fn ensure_runtime() -> miette::Result<PathBuf> {
    let prefix = paths::runtime_prefix();
    let current_hash = lock_sha256(LOCKFILE);

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
        install::install_from_lockfile(LOCKFILE, &prefix).await?;
        write_hash(&prefix, &current_hash)?;
        let version = version_from_lock(LOCKFILE, "lembas")?;
        eprintln!("Installed lembas v{} to {}", version, prefix.display());
    }

    Ok(prefix)
}
