# lembas-cli

Self-bootstrapping Rust CLI that embeds a conda lockfile and delegates to `python -m lembas`.

> **Note for Claude**: Keep this file updated as we develop. Add notes about maintenance procedures, release processes, design patterns, and architectural decisions here so future sessions have full context.

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
- `update.rs` — self-update mechanism via GitHub releases

### Testing

Unit tests live in `#[cfg(test)] mod tests` at bottom of each module. Test pure functions (hashing, parsing, path logic). Integration tests requiring network/filesystem are not worth the flakiness.

### Dependencies

- `rattler*` crates for conda operations (keep versions aligned)
- `miette` for error handling
- `tracing` + `tracing-subscriber` for logging
- `self-replace` for atomic binary replacement during updates
- `semver` for version parsing and comparison
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

## Releasing

CLI uses CalVer (`YYYY.M.PATCH`). Version is derived from git tags via `build.rs` (like setuptools-scm):
- On tag `v2026.7.0` → `2026.7.0`
- 5 commits after tag → `2026.7.0.dev5+g1234567`

To cut a release:
```bash
# Create draft release with auto-generated notes
gh release create v2026.7.1 --generate-notes --draft

# Review in GitHub UI, then publish
# release.yml automatically builds binaries and uploads to the release
```

The `--version` output shows both lembas-core version and CLI build:
```
lembas 0.3.1 (cli build 2026.7.0)
```

## Self-Update

Users can update the CLI binary in-place:

```bash
lembas self update              # update to latest version
lembas self update v2026.7.1    # update to specific version
lembas self update check        # check if update available
lembas self update list         # list available versions
lembas self update --force      # force reinstall current version
```

Implementation uses `self-replace` crate for atomic binary replacement. Downloads pre-built binaries from GitHub releases matching the current platform (darwin-arm64, darwin-x86_64, linux-x86_64, linux-aarch64).

### Release Signing

Binaries are signed with Ed25519 to verify they come from the same publisher. The CLI verifies signatures before applying updates.

**Key rotation:**
1. Generate new keypair
2. Add new public key to `src/signing_keys/` and update `TRUSTED_PUBLIC_KEYS` in `update.rs`
3. Release new CLI version (users update to get the new key)
4. Update `RELEASE_SIGNING_KEY` GitHub secret to new private key
5. (Optional later) Remove old public key from array

The private key is stored in `RELEASE_SIGNING_KEY` GitHub secret (base64-encoded). Public keys are embedded in the binary via `include_bytes!`.
