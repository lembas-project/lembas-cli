//! Runtime helpers for lembas-cli.

use std::path::Path;

use miette::{miette, Context, IntoDiagnostic, Result};
use sha2::{Digest, Sha256};

const HASH_FILE: &str = ".lockfile-hash";

/// Compute SHA256 hash of lock content.
pub(crate) fn lock_sha256(lock_content: &str) -> String {
    let hash = Sha256::digest(lock_content.as_bytes());
    format!("{:x}", hash)
}

/// Check if the stored hash matches the current hash.
pub(crate) fn hash_matches(prefix: &Path, current_hash: &str) -> Result<bool> {
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
pub(crate) fn write_hash(prefix: &Path, hash: &str) -> Result<()> {
    let hash_file = prefix.join(HASH_FILE);
    std::fs::write(&hash_file, hash)
        .into_diagnostic()
        .context("failed to write lockfile hash")?;
    Ok(())
}

/// Extract version from lockfile for a package.
pub(crate) fn version_from_lock(lock_content: &str, package_name: &str) -> Result<String> {
    for line in lock_content.lines() {
        let line = line.trim();
        // Look for conda package URLs like:
        // - conda: https://conda.anaconda.org/lembas-project/noarch/lembas-0.3.1-pyh4616a5c_0.conda
        if line.starts_with("- conda:") && line.contains(&format!("/{}-", package_name)) {
            if let Some(filename) = line.rsplit('/').next() {
                // filename is like: lembas-0.3.1-pyh4616a5c_0.conda
                let parts: Vec<&str> = filename.split('-').collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].to_string());
                }
            }
        }
    }

    Err(miette!(
        "could not find {} version in lockfile",
        package_name
    ))
}
