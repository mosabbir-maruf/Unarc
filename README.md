# Unarc

A production-grade, security-focused archive utility CLI in Rust.

Designed targeting **macOS Apple Silicon** first, with full **Docker/Linux** development and CI support. Unarc enforces zero-trust path validation and safe extraction defaults.

---

## Key Principles & Phase 1 Scope

- **Zero Host Pollution**: All Rust compilation, formatting, linting, and testing run strictly inside pinned Docker containers. No host toolchain installations required.
- **Security-First Architecture**: Path traversal prevention (`Zip Slip`), absolute path rejection, null-byte checks, and boundary containment enforcement.
- **Zero Extraneous State**: No telemetry, analytics, updater, network services, background daemons, persistent state, or uncoordinated file logging.
- **Clean Subsystem Boundaries**: Modularity separating presentation, core orchestration, format abstractions, security policy, platform detection, and structured errors.

---

## Prerequisites

- **Docker** (Docker Desktop on macOS, or Docker Engine on Linux)
- `make` (optional, for convenience wrappers)

No local Rust or Cargo installation is needed on your machine.

---

## Development & Validation Commands

All commands run through the pinned toolchain container (`rust:1.85.0-slim`):

```bash
# Run full suite (format check, clippy, unit/integration tests, release build)
make check

# Format source code
make fmt

# Verify formatting without changes
make fmt-check

# Run Clippy (warnings treated as errors)
make clippy

# Run all unit and integration tests
make test

# Build debug binary
make build

# Build optimized release binary
make release
```

Alternatively, invoke `./scripts/dev.sh <command>` directly (e.g. `./scripts/dev.sh test`).

---

## CLI Usage

The binary name is `unarc`.

### 1. Platform & Engine Info
```bash
unarc info
# Or JSON output:
unarc --json info
```

### 2. Archive Inspection
Inspect archive format, disk size, and encryption indicators:
```bash
unarc inspect path/to/archive.zip
unarc --json inspect path/to/archive.tar.gz
```

### 3. Path Security Validation
Validate candidate relative paths and destination containment against zero-trust policy:
```bash
# Verify relative path safety
unarc validate documents/report.pdf

# Verify boundary containment against destination folder
unarc validate data/extracted.json --base-dir /destination
```

---

## Module Layout

| Subsystem | Path | Description |
|---|---|---|
| **CLI / Presentation** | `src/cli/` | Argument parsing (`clap`), user messaging, JSON serialization |
| **Core Application** | `src/core/` | Domain coordinator and operational workflow |
| **Archive Layer** | `src/archive/` | Format detection (magic bytes/extensions) and `ArchiveBackend` trait |
| **Security Layer** | `src/security/` | Path sanitization, traversal defense, boundary verification |
| **Errors** | `src/error.rs` | Strongly typed domain errors and UNIX process exit codes |
| **Platform** | `src/platform/` | macOS Apple Silicon and Linux runtime/capability interrogation |

For detailed architectural rationale, refer to [ARCHITECTURE.md](ARCHITECTURE.md).
