<h1 alignt="center">X Package Manager</h1>

> Modern, high-performance package manager written in pure Rust for the X distribution (Arch Linux-based).

## Overview

`xpm` is a native Rust replacement for `pacman` and `libalpm`, designed for the X distribution. It uses the `.xp` package format (X Package) natively and maintains compatibility with Arch Linux `.pkg.tar.zst` packages.

### Key features

- **Pure Rust** -- zero C dependencies at any stage
- **Native .xp format** -- X Package format (tar.zst) with `.PKGINFO` / `.BUILDINFO` / `.MTREE` metadata
- **SAT-based dependency resolver** -- powered by `resolvo` with CDCL and watched-literal propagation
- **Arch compatible** -- reads `.pkg.tar.zst` packages and `alpm-repo-db` databases
- **Flexible repository management** -- predefined and temporary repos with `xpm repo add/remove/list`
- **OpenPGP verification** -- detached signatures with Web of Trust model
- **TOML configuration** -- clean, human-readable config at `/etc/xpm.conf`

## Installation

```bash
git clone https://github.com/xscriptordev/xpm.git
cd xpm
cargo build --release
sudo cp target/release/xpm /usr/local/bin/
```

## Usage

```bash
# Sync package databases
xpm sync

# Install packages
xpm install <package> [<package>...]

# Remove packages
xpm remove <package>

# System upgrade
xpm upgrade

# Search packages
xpm search <query>

# Query installed packages
xpm query

# Package info
xpm info <package>

# List files owned by a package
xpm files <package>

# Manage repositories
xpm repo list
xpm repo add <name> <url>
xpm repo remove <name>
```

### Pacman-style aliases

| Alias | Command |
|-------|---------|
| `xpm Sy` | `xpm sync` |
| `xpm S <pkg>` | `xpm install <pkg>` |
| `xpm R <pkg>` | `xpm remove <pkg>` |
| `xpm Su` | `xpm upgrade` |
| `xpm Q` | `xpm query` |
| `xpm Ss <query>` | `xpm search <query>` |
| `xpm Si <pkg>` | `xpm info <pkg>` |
| `xpm Ql <pkg>` | `xpm files <pkg>` |

### Global flags

| Flag | Description |
|------|-------------|
| `-c, --config <PATH>` | Custom configuration file |
| `-v, --verbose` | Increase verbosity (-v, -vv, -vvv) |
| `--no-confirm` | Skip confirmation prompts |
| `--root <PATH>` | Alternative installation root |
| `--dbpath <PATH>` | Alternative database directory |
| `--cachedir <PATH>` | Alternative cache directory |
| `--no-color` | Disable colored output |

## Configuration

Configuration file: `/etc/xpm.conf` (TOML format).

See [etc/xpm.conf.example](etc/xpm.conf.example) for all available options.

```toml
[options]
root_dir = "/"
db_path = "/var/lib/xpm/"
cache_dir = "/var/cache/xpm/pkg/"
sig_level = "optional"
parallel_downloads = 5

[[repo]]
name = "core"
server = [
    "https://xscriptordev.github.io/x-repo/$repo/os/$arch",
]

[[repo]]
name = "extra"
server = [
    "https://xscriptordev.github.io/x-repo/$repo/os/$arch",
]
```

### Repository management

Predefined repositories are configured in `/etc/xpm.conf`. Temporary repositories can be added at runtime with `xpm repo add` and are stored in `/etc/xpm.d/`.

## Project structure

```text
xpm/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── xpm/                    # Binary crate (CLI frontend)
│   │   └── src/
│   │       ├── main.rs         # Entry point, logging, config, dispatch
│   │       └── cli.rs          # clap CLI definition
│   └── xpm-core/               # Library crate (core logic)
│       └── src/
│           ├── lib.rs           # Module root
│           ├── config.rs        # TOML configuration parser
│           ├── error.rs         # Error types
│           └── repo.rs          # Repository manager
├── etc/
│   └── xpm.conf.example        # Example configuration
└── ROADMAP.md                   # Development roadmap
```

## Technical architecture

### Dependency resolution

`xpm` uses a logic-based SAT solver (`resolvo`) that transforms package relationships into CNF boolean clauses:

| Requirement | CNF Clause | Meaning |
|---|---|---|
| Dependency | `!foo OR bar` | If `foo` is installed, `bar` must be too |
| Root requirement | `foo` | Target package is mandatory |
| Conflict | `!bar_v1 OR !bar_v2` | Mutually exclusive versions |

The solver implements Unit Propagation with watched literals and Conflict-Driven Clause Learning (CDCL) for efficient backtracking.

### Package format

Packages use the ALPM `.pkg.tar.zst` format with Zstandard compression:

- `.PKGINFO` -- package name, version, dependencies
- `.BUILDINFO` -- reproducible build environment
- `.MTREE` -- file integrity hashes
- `.INSTALL` -- optional pre/post install scripts

### Security

- **OpenPGP detached signatures** (`.sig`) for packages and databases
- **Web of Trust** model for key validation
- **Fakeroot** build environment for safe package creation
- **Package linting** framework for quality assurance

## Repository hosting

The default package repository is hosted on **GitHub Pages** at `xscriptordev.github.io/x-repo`. This will migrate to the `xscriptordev` organization for consistency as the project grows. `xpm` supports any HTTP-based static file server, making future migration to a VPS transparent.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full development roadmap.

| Version | Milestone |
|---|---|
| `v0.1.0` | Functional CLI with configuration |
| `v0.5.0` | Native engine (resolver + packages + repo db) |
| `v0.8.0` | Security and transaction management |
| `v1.0.0` | Benchmarked, tested, production-ready |

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).