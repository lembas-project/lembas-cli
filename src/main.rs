//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.

use std::process::ExitCode;

fn run() -> i32 {
    // Placeholder - will delegate to python -m lembas
    println!("lembas {}", env!("CARGO_PKG_VERSION"));
    0
}

fn main() -> ExitCode {
    match run() {
        0 => ExitCode::SUCCESS,
        code => ExitCode::from(code as u8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_returns_success() {
        assert_eq!(run(), 0);
    }
}
