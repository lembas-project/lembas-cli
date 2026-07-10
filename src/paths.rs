//! Path helpers for lembas-cli.

use std::path::PathBuf;

/// Get the lembas home directory (~/.lembas).
fn lembas_home() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".lembas")
}

/// Get the runtime prefix directory (~/.lembas/runtime).
pub fn runtime_prefix() -> PathBuf {
    lembas_home().join("runtime")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_prefix_ends_with_runtime() {
        let prefix = runtime_prefix();
        assert!(prefix.ends_with("runtime"));
    }

    #[test]
    fn test_runtime_prefix_contains_lembas() {
        let prefix = runtime_prefix();
        assert!(prefix.to_string_lossy().contains(".lembas"));
    }
}
