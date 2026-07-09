//! Runtime helpers for lembas-cli.

use miette::{miette, Result};
use sha2::{Digest, Sha256};

/// Compute SHA256 hash of lock content.
pub fn lock_sha256(lock_content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(lock_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extract version from lockfile for a package.
pub fn version_from_lock(lock_content: &str, package_name: &str) -> Result<String> {
    // Parse YAML lockfile to find the package version
    // This is a simplified parser - just look for the package in the lockfile
    for line in lock_content.lines() {
        let line = line.trim();
        // Look for conda package URLs like:
        // - conda: https://conda.anaconda.org/lembas-project/noarch/lembas-0.3.1-pyh4616a5c_0.conda
        if line.starts_with("- conda:") && line.contains(&format!("/{}-", package_name)) {
            // Extract version from URL
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
