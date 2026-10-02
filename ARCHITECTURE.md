# Unarc Architecture & Phase 1 Decisions

This document outlines the architectural boundaries, system design, and technical decisions implemented for **Phase 1** of Unarc.

---

## 1. System Architecture & Module Boundaries

Unarc follows strict unidirectional dependencies to preserve modularity, testability, and safety:

```
                  +---------------------------+
                  |         src/main.rs       |
                  +-------------+-------------+
                                |
                                v
                  +---------------------------+
                  |         src/cli/          |
                  |  (args, output, runner)   |
                  +-------------+-------------+
                                |
                                v
                  +---------------------------+
                  |        src/core/          |
                  |     (Application)         |
                  +------+-------------+------+
                         |             |
           +-------------+             +-------------+
           v                                         v
+---------------------+                   +---------------------+
|    src/archive/     |                   |    src/security/    |
| (format, metadata,  |                   | (path sanitization, |
| backend abstraction)|                   |  containment policy)|
+---------------------+                   +---------------------+
           |                                         |
           +--------------------+--------------------+
                                |
                                v
                  +---------------------------+
                  |        src/error.rs       |
                  |   (typed domain errors)   |
                  +-------------+-------------+
                                |
                                v
                  +---------------------------+
                  |       src/platform/       |
                  |  (macOS Apple Silicon &   |
                  |    Linux capabilities)    |
                  +---------------------------+
```

### Module Responsibilities

1. **`src/cli/` (Presentation Layer)**:
   - Command-line parsing via `clap` (derive mode).
   - Global parameters (`--json`, `--verbose`, `--quiet`).
   - Human-readable and structured JSON renderers in `output.rs`.
   - Never contains archive unpacking or raw file I/O logic.

2. **`src/core/` (Core Application Logic)**:
   - Houses the `Application` struct which orchestrates security rules and archive inspection.
   - Decoupled from CLI argument parsing so it can be called as an embedded library or test runner.

3. **`src/archive/` (Archive Subsystem)**:
   - Identifies archive formats (`Zip`, `SevenZip`, `Tar`, `TarGz`, `TarBz2`, `TarXz`, `Rar`) via file extension and initial header magic bytes.
   - Defines `ArchiveMetadata` and `ArchiveEntry` structures.
   - Defines the `ArchiveBackend` trait as a zero-cost extension point for Phase 2 decompressors (e.g., `7zz` CLI or native libraries).

4. **`src/security/` (Security & Policy Layer)**:
   - `sanitize_relative_path`: Lexically examines path components, strictly rejecting `..` traversal, leading slashes, Windows drive prefixes, and null bytes.
   - `verify_boundary_containment`: Ensures joined target paths do not escape base destination directories.
   - `SecurityPolicy`: Encapsulates configurable depth, length, and symlink rules with zero-trust defaults.

5. **`src/error.rs` (Errors & Exit Codes)**:
   - Hierarchical error types via `thiserror`: `SecurityError`, `ArchiveError`, `PlatformError`, `Cli`, `Io`.
   - Maps errors directly to standard UNIX process exit codes:
     - `1`: CLI usage / argument errors
     - `2`: Security & traversal violations
     - `3`: Archive format & inspection failures
     - `4`: Platform & capability failures
     - `5`: I/O failures

6. **`src/platform/` (Platform-Specific Code)**:
   - Target prioritization: macOS Apple Silicon (`aarch64-apple-darwin`) runtime detection, plus Linux container detection.
   - Exposes platform attributes such as quarantine flags (`com.apple.quarantine`) and capability queries.

---

## 2. Key Phase 1 Decisions

### Decision 1: Pure Docker-Based Toolchain
- **Context**: The host machine must remain completely unpolluted (no global Rust, Cargo, Homebrew, or system package installs).
- **Decision**: All compilation, testing, clippy linting, and formatting run inside a Docker image based on `rust:1.85.0-slim` with `rustfmt` and `clippy`.
- **Reproducibility**: The toolchain is pinned in `rust-toolchain.toml` and the `Dockerfile` to guarantee parity between local developer environments and CI.
- **Portability**: The development script `scripts/dev.sh` handles Docker daemon bind-mount variations (including macOS `/Volumes` sandbox isolation) with automatic fallback.

### Decision 2: Minimal & Justified Dependencies
- **Included Dependencies**:
  - `clap`: Industry standard for declarative, type-safe CLI parsing.
  - `thiserror`: Zero-runtime-overhead error derives.
  - `serde` & `serde_json`: Necessary for reproducible machine-readable JSON output.
- **Excluded**:
  - No asynchronous runtimes (`tokio`, `async-std`): Archive operations in Phase 1 and CLI workflows are synchronous and CPU/disk bound.
  - No logging crates writing to local disk or daemon logs.
  - No network or HTTP crates (`reqwest`, `hyper`): Zero external network surface.

### Decision 3: Security & Zero-Trust by Default
- Archives are treated as untrusted inputs.
- Symlinks and absolute path components are rejected by default.
- Parent traversal components (`..`) are rejected during lexical component inspection before any filesystem interaction takes place.

### Decision 4: Phase 2 Extension Points Without Premature Implementations
- Extraction routines, volume handling, 7zz binary execution, and sandboxing are deferred to subsequent phases as required.
- The `ArchiveBackend` trait and `SecurityPolicy` provide clean, type-checked contracts to plug in extraction engines without modifying CLI or core logic.
