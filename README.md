# lembas-cli

Self-bootstrapping CLI for [lembas](https://github.com/lembas-project/lembas-core) lifecycle engineering analysis.

## Overview

This is a thin Rust binary that:

1. **Bootstraps** a conda environment with `lembas-core`, `pixi`, and Python on first run
2. **Delegates** all commands transparently to `python -m lembas` in that environment

The result is a single `lembas` executable that works out of the box without requiring users to install Python, conda, or pixi separately.

## Architecture

```
lembas (Rust binary)
  └── ~/.lembas/runtime/lembas/ (conda environment)
        ├── python
        ├── lembas-core
        └── pixi
```

The Rust binary uses [conda-ship](https://github.com/jezdez/conda-ship)'s Fleet API to manage the embedded conda environment. The environment is:

- **Self-updating**: automatically updates when the embedded lockfile changes
- **Cached**: packages are cached and shared across updates
- **Locked**: exact package versions are pinned via `locks/lembas.lock`

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
make lock       # regenerate locks/lembas.lock
make release    # rebuild with new lockfile
```

## License

MIT
