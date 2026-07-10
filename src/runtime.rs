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

/// Ensure the lembas runtime is installed and up-to-date.
async fn ensure_runtime() -> miette::Result<PathBuf> {
    let prefix = paths::runtime_prefix();
    let current_hash = lock_sha256(LOCKFILE);

    let needs_install = if !is_installed(&prefix) {
        tracing::info!("Installing lembas runtime (first run)...");
        true
    } else if !hash_matches(&prefix, &current_hash)? {
        tracing::info!("Updating lembas runtime...");
        true
    } else {
        false
    };

    if needs_install {
        install::install_from_lockfile(LOCKFILE, &prefix).await?;
        write_hash(&prefix, &current_hash)?;
        tracing::info!(
            "Installed lembas v{} to {}",
            env!("CARGO_PKG_VERSION"),
            prefix.display()
        );
    }

    Ok(prefix)
}

/// Run the lembas entry point with the given arguments in the runtime environment.
pub async fn run_lembas(args: &[String]) -> miette::Result<i32> {
    let prefix = ensure_runtime().await?;
    let env_vars = install::activation_env(&prefix)?;
    let lembas_bin = prefix.join("bin").join("lembas");

    let status = std::process::Command::new(&lembas_bin)
        .envs(&env_vars)
        .args(args)
        .status()
        .into_diagnostic()
        .context("failed to execute lembas")?;

    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_lock_sha256_deterministic() {
        let content = "hello world";
        let hash1 = lock_sha256(content);
        let hash2 = lock_sha256(content);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_lock_sha256_known_value() {
        let hash = lock_sha256("hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_lock_sha256_different_content() {
        let hash1 = lock_sha256("content a");
        let hash2 = lock_sha256("content b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_is_installed_false_when_missing() {
        let dir = TempDir::new().unwrap();
        assert!(!is_installed(dir.path()));
    }

    #[test]
    fn test_is_installed_true_when_conda_meta_exists() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("conda-meta")).unwrap();
        assert!(is_installed(dir.path()));
    }

    #[test]
    fn test_hash_matches_false_when_no_file() {
        let dir = TempDir::new().unwrap();
        let result = hash_matches(dir.path(), "somehash").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_hash_matches_true_when_same() {
        let dir = TempDir::new().unwrap();
        let hash = "abc123";
        write_hash(dir.path(), hash).unwrap();
        assert!(hash_matches(dir.path(), hash).unwrap());
    }

    #[test]
    fn test_hash_matches_false_when_different() {
        let dir = TempDir::new().unwrap();
        write_hash(dir.path(), "hash1").unwrap();
        assert!(!hash_matches(dir.path(), "hash2").unwrap());
    }

    #[test]
    fn test_write_hash_creates_file() {
        let dir = TempDir::new().unwrap();
        write_hash(dir.path(), "myhash").unwrap();
        let content = std::fs::read_to_string(dir.path().join(HASH_FILE)).unwrap();
        assert_eq!(content, "myhash");
    }
}
