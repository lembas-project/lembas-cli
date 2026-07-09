//! Path helpers for lembas-cli.

use std::path::PathBuf;

/// Get the lembas home directory (~/.lembas).
pub fn lembas_home() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".lembas")
}
