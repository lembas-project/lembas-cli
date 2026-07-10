# lembas-cli

Self-bootstrapping CLI for [lembas](https://github.com/lembas-project/lembas-core) lifecycle engineering analysis.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/lembas-project/lembas-cli/main/scripts/install.sh | sh
```

Or with options:

```bash
# Install specific version
curl -fsSL https://raw.githubusercontent.com/lembas-project/lembas-cli/main/scripts/install.sh | sh -s -- --version v0.3.1

# Install to custom directory
curl -fsSL https://raw.githubusercontent.com/lembas-project/lembas-cli/main/scripts/install.sh | sh -s -- --install-dir /usr/local/bin
```

Run `./install.sh --help` for all options.

## Overview

This is a thin Rust binary that:

1. **Bootstraps** a conda environment with `lembas-core`, `pixi`, and Python on first run
2. **Delegates** all commands transparently to `python -m lembas` in that environment

The result is a single `lembas` executable that works out of the box without requiring users to install Python, conda, or pixi separately.

## Architecture

```
lembas (Rust binary)
  └── ~/.lembas/runtime/ (conda environment)
        ├── python
        ├── lembas-core
        └── pixi
```

The Rust binary uses [rattler](https://github.com/conda/rattler) to manage the embedded conda environment. The environment is:

- **Self-updating**: automatically updates when the embedded lockfile changes
- **Cached**: packages are cached and shared across updates
- **Locked**: exact package versions are pinned via `locks/pixi.lock`

## Building

```bash
make debug      # build debug binary (default)
make release    # build optimized release binary
make install-debug  # build debug and symlink to ~/.local/bin/lembas
make clean      # remove build artifacts
```

The release binary is optimized for size with LTO and stripping enabled.

## Updating the embedded environment

```bash
# Edit locks/pixi.toml to change dependencies, then:
make lock       # regenerate locks/pixi.lock
make release    # rebuild with new lockfile
```

## License

MIT
