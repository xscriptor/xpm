# Contributing to xpm

Thank you for your interest in contributing to xpm! This document provides
guidelines to help you get started.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/<your-user>/xpm.git
   cd xpm
   ```
3. Create a feature branch:
   ```bash
   git checkout -b feature/my-change
   ```
4. Make your changes, then build and test:
   ```bash
   cargo build --workspace
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --all -- --check
   ```
5. Commit and push your branch, then open a Pull Request

## Development Setup

- **Rust 2021 edition** (1.70+)
- **Workspace:** `crates/xpm` (binary) + `crates/xpm-core` (library)
- Business logic goes in `xpm-core`; the binary crate handles CLI only

## Code Style

- **Line width:** 100 characters max (see `rustfmt.toml`)
- **Clippy:** All warnings denied — run `cargo clippy -- -D warnings`
- **Error handling:** `thiserror` in the library, `anyhow` in the binary
- **Logging:** Use `tracing` macros, never `println!` for debug output
- **Tests:** Place tests in `#[cfg(test)] mod tests` within each file

## What to Contribute

- Bug fixes and error handling improvements
- New features from the roadmap
- Documentation improvements
- Test coverage for edge cases
- Performance optimizations
- Package format compatibility improvements

Check the [ROADMAP.md](ROADMAP.md) for planned features.

## Reporting Bugs

Open an issue on GitHub with:
- Steps to reproduce
- Expected vs actual behavior
- xpm version (`xpm --version`)
- Operating system and architecture

## Code of Conduct

Be respectful and constructive. We are building tools for the community.

## License

By contributing, you agree that your contributions will be licensed under
GPL-3.0-or-later.
