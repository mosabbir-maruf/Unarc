# Contributing to Unarc

Thank you for your interest in contributing to Unarc!

Unarc is an open-source, security-focused archive utility written in Rust. We welcome bug reports, documentation improvements, and pull requests that adhere to our security architecture and quality standards.

---

## Architectural Principles

Before contributing code, please keep the following non-negotiable principles in mind:

1. **Zero Host Pollution**: All build tooling, format checks, lints, and test execution run inside reproducible, pinned Docker containers (`rust:1.85.0-slim`).
2. **Hermetic Archive Engine**: Unarc never queries system PATH for engine binaries and never downloads untrusted binaries at runtime.
3. **Zero-Trust Extraction**: Absolute paths are forbidden, parent directory traversal (`..`) is strictly blocked, and all file operations are confined within the designated destination root.
4. **Deterministic Exit Codes**: All failures map to documented, stable exit codes.
5. **No Telemetry or Background Services**: Unarc contains no analytics, phone-home mechanisms, or background daemons.

---

## Local Development Workflow

All development commands are containerized through `make` or `scripts/dev.sh`:

```bash
# Run complete verification (fmt-check, clippy, tests, build)
make check

# Format Rust source code
make fmt

# Check formatting without modifying files
make fmt-check

# Run Clippy lints (warnings treated as errors)
make clippy

# Run the complete test suite
make test

# Build debug binary
make build

# Build optimized release binary
make release
```

---

## Pull Request Guidelines

1. **Format & Lint Cleanliness**: Ensure `make fmt-check` and `make clippy` pass with zero warnings.
2. **Comprehensive Tests**: Add unit tests in `src/` or integration tests in `tests/integration_tests.rs` for any new behavior or bug fix.
3. **Security Invariant Preservation**: PRs must not degrade path traversal checks, sandbox constraints, or volume sequencing rules.
4. **Concise Commits**: Write clear, descriptive commit messages describing the rationale for changes.
5. **Licensing**: All contributions to Unarc are submitted under the terms of the MIT License.
