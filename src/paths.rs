//! Path helpers for lembas-cli.

use std::path::PathBuf;

/// Get the lembas home directory (~/.lembas).
pub(crate) fn lembas_home() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".lembas")
}

/// Get the runtime prefix directory (~/.lembas/runtime).
pub(crate) fn runtime_prefix() -> PathBuf {
    lembas_home().join("runtime")
}
