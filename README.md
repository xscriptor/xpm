# Technical Architecture Report: xpg Package Manager for X Distribution

## 1. Introduction and Project Scope

The `xpg` project is a modern, high-performance package management solution engineered for 'X', an Arch Linux-based distribution. Architecturally, `xpg` represents a strategic pivot from the legacy C-based infrastructure—specifically `libalpm` and the `pacman` frontend—toward a safety-critical framework authored in Rust.

This transition leverages Rust’s ownership model and zero-cost abstractions to eliminate entire classes of memory-safety vulnerabilities inherent in C. The project is a primary beneficiary of the 2024 modernization efforts funded by the Sovereign Tech Fund (STF), which prioritized the formalization of core ALPM specifications. These specifications, including `alpm-db`, `alpm-repo`, and `alpm-package`, are distributed via the `alpm-docs` package to ensure technical transparency.

During this transition, `xpg` utilizes the `alpm.rs` crate, which provides ergonomic and safe FFI bindings to `libalpm` v15.x.x. This hybrid approach allows us to maintain compatibility with existing Arch Linux infrastructure while incrementally migrating core logic to a native Rust implementation.

## 2. Rust Project Architecture

The `xpg` codebase is organized to maximize modularity and testability, adhering to industry best practices for robust Rust CLI development.

- **main.rs**: The entry point of the binary. It orchestrates high-level logic, initializes global states, and handles top-level error reporting.
- **lib.rs**: The library crate root. It houses the core business logic, abstracting the package management operations from the interface.
- **cli.rs**: A dedicated abstraction layer for the Command Line Interface. It leverages the `clap` crate for declarative argument parsing, subcommand management, and automated documentation generation.
- **config.rs**: Manages the application lifecycle regarding configuration, handling environment variables, and parsing filesystem-based configuration files.

### Conceptual Directory Tree

```text
xpg/
├── src/
│   ├── main.rs      # Binary entry point
│   ├── lib.rs       # Library root (core logic)
│   ├── cli.rs       # CLI interface definition (clap)
│   └── config.rs    # Configuration management
├── tests/           # Integration and functional tests
├── examples/        # Crate usage examples for developers
└── Cargo.toml       # Manifest and dependency management
```

## 3. Dependency Resolution Engine

`xpg` incorporates a modern, logic-based dependency solver powered by the `resolvo` library (the Rust-native evolution of `libsolv_rs`). Unlike legacy heuristic-based solvers, `resolvo` treats dependency resolution as a mathematical optimization problem.

### Logic-Based Resolution Components

1. **Boolean Satisfiability (SAT)**: All package relationships (dependencies, conflicts, and version constraints) are transformed into a set of boolean clauses. These clauses must be in **Conjunctive Normal Form (CNF)**, utilizing exclusively `¬` (NOT) and `∨` (OR) operators.
2. **Unit Propagation**: An optimization mechanism that assigns values to variables when a clause is "forced" to a specific outcome to remain true. Our implementation follows the **watched literals** technique described in the **MiniSAT paper** to maintain high performance during propagation.
3. **Conflict-Driven Clause Learning (CDCL)**: When the solver hits a conflict (a state where no assignment satisfies all clauses), it analyzes the conflict's root cause, "learns" a new clause to prevent re-entry into that state, and backtracks to a previous valid decision level.

### Dependency Translation to CNF

The following table demonstrates how standard package requirements are represented as boolean clauses within the solver:

|   |   |   |
|---|---|---|
|Dependency Requirement|CNF Boolean Clause|Architectural Interpretation|
|**Dependency**|`¬foo ∨ bar`|If `foo` is selected, `bar` must also be selected.|
|**Root Requirement**|`foo`|The target package `foo` is a mandatory assignment.|
|**Conflict**|`¬bar_v1 ∨ ¬bar_v2`|Mutually exclusive versions of the same package.|
|**Unavailability**|`¬baz`|A package omitted from the repository or blacklisted.|

## 4. Package Formatting and Compression Standards

All software artifacts managed by `xpg` utilize the `alpm-package` format, encapsulated in `.pkg.tar.zst` archives.

### Metadata Specification

Each archive contains critical metadata files at the top level:

- **.PKGINFO**: Contains core metadata: package names, versions, and dependency declarations.
- **.BUILDINFO**: Facilitates reproducible builds by documenting the specific build environment.
- **.MTREE**: A directory of file hashes and timestamps used for post-installation integrity verification.
- **.INSTALL**: Optional scripts executed during specific transaction phases (e.g., `post_install`).

### Archive Handling and Streaming Constraints

`xpg` utilizes the `tar_minimal` library for high-performance Unix-native streaming with Zstandard (Zstd) compression. This library implements a strict **Builder/Decoder architecture**. Due to its minimalist design, the following architectural constraints are enforced:

- **No Random Access**: The engine does not support listing or reading individual files out of sequence; it is strictly a stream-in/stream-out pipeline.
- **Unix-Native Only**: The library is optimized for Unix permissions and metadata; Windows environments are not supported.
- **Immutable Archives**: The format does not support in-place updates or appending; archives must be fully reconstructed if metadata changes.

## 5. Repository Database Architecture

Repository metadata is defined by the `alpm-repo-db` format. This format organizes metadata into directories named according to the schema `package-version` (e.g., `example-package-1.0.0-1/desc`).

### Repository Database Variants

- **Default**: Includes `alpm-repo-desc` for each package. This is the minimum requirement for basic searching, dependency resolution, and download orchestration.
- **Default with Files**: Adds `alpm-repo-files`, enabling advanced queries such as identifying which package owns a specific binary on the filesystem.

### Agnostic Symlinks and Interoperability

To facilitate seamless server-side compression transitions (e.g., moving from `.gz` to `.zst`), `xpg` relies on **agnostic symlinks**. For example, the symlink `repo.db` points to the specific compressed archive (e.g., `repo.db.tar.zst`). This allows the client to request a stable filename while the backend updates compression "on the fly" without breaking existing package manager installations.

## 6. Security and Verification Framework

`xpg` implements a rigorous security model to safeguard the software supply chain against artifact tampering and unauthorized distribution.

- **Berblom Algorithm**: Utilized for advanced key management and establishing granular trust levels for package signers.
- **Web of Trust (WoT)**: The underlying trust model for validating the authenticity of the keys used to sign software artifacts.
- **Digital Signatures**: All repository databases and packages must be accompanied by **OpenPGP detached signatures** using the `.sig` suffix.
- **Fakeroot Build Environment**: During package creation, binaries are compiled and staged within a `fakeroot` environment to prevent the build process from interfering with the host system's root filesystem.

### Maintenance Quality Assurance

Maintainers employ a Rust-based **linting framework** that performs static analysis on packages. This ensures that every artifact complies with distribution policies and meets technical quality standards before being signed and pushed to the repository.

## 7. Conclusion and Future Technical Roadmap

By synthesizing Rust’s memory safety with formalized ALPM specifications, `xpg` provides a transparent and robust foundation for the 'X' distribution. The shift from legacy C-FFI toward a native Rust stack ensures that the package manager remains performant and secure under modern threat models.

**Future Development Goals:**

- **Python Interoperability**: Finalizing modular bindings to allow automation tools and external scripts to interact safely with the `xpg` engine.
- **Enhanced Internationalization (i18n)**: Implementing comprehensive support for localized messaging across all CLI subcommands.
- **Verification Expansion**: Integrating emerging cryptographic standards to further harden the Web of Trust model.