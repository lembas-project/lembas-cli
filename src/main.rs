//! Lembas CLI - Self-bootstrapping executable for lifecycle engineering analysis.

use std::process::ExitCode;

fn main() -> ExitCode {
    // Placeholder - will delegate to python -m lembas
    println!("lembas {}", env!("CARGO_PKG_VERSION"));
    ExitCode::SUCCESS
}
