# lembas-cli

Self-bootstrapping Rust CLI that embeds a conda lockfile and delegates to `python -m lembas`.

## Build & Test

```bash
cargo build              # debug build
cargo build --release    # release build
cargo test               # run tests
cargo llvm-cov           # coverage report (requires cargo-llvm-cov)
pre-commit run --all-files  # lint checks
```

## Architecture

```
lembas (Rust binary)
  └── ~/.lembas/runtime/ (conda environment, installed via rattler)
        ├── python
        └── lembas-core
```

The CLI:
1. Checks if runtime is installed and lockfile hash matches
2. Installs/updates via rattler if needed
3. Delegates all args to `python -m lembas` in the activated environment

## Code Style

### Branch naming

Use `<type>/<subject>` format to align with conventional commits:
- `feat/calver-versioning`
- `fix/install-script`
- `chore/update-deps`

### Logging

Use `tracing` macros, never `println!`/`eprintln!`. The crate enforces this with:
```rust
#![deny(clippy::print_stdout, clippy::print_stderr)]
```

Log levels:
- `tracing::error!` — fatal errors before exit
- `tracing::warn!` — unexpected but recoverable issues
- `tracing::info!` — user-facing status ("Installing...", package counts, timing)
- `tracing::debug!` — verbose troubleshooting (controlled via `RUST_LOG`)

Default output is minimal (no timestamp, no level prefix, no target) for clean CLI UX.

### Module organization

Flat structure in `src/`:
- `main.rs` — entry point, arg handling, tracing setup
- `runtime.rs` — ensure runtime installed, run lembas
- `install.rs` — rattler-based package installation
- `paths.rs` — path helpers (~/.lembas/runtime)

### Testing

Unit tests live in `#[cfg(test)] mod tests` at bottom of each module. Test pure functions (hashing, parsing, path logic). Integration tests requiring network/filesystem are not worth the flakiness.

### Dependencies

- `rattler*` crates for conda operations (keep versions aligned)
- `miette` for error handling
- `tracing` + `tracing-subscriber` for logging
- `tempfile` (dev) for test fixtures

## CI

- `Check` job aggregates all CI jobs via alls-green — branch protection requires only this
- Codecov comments on PRs but doesn't block (status checks disabled)
- Pre-commit runs in CI and auto-commits fixes

## Lockfile

`locks/pixi.lock` is embedded at compile time via `include_str!`. Update with:
```bash
make lock   # or: pixi lock --manifest-path locks/pixi.toml
```
