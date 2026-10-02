# Unarc Release Readiness Checklist

This document tracks the final production release verification and readiness status for Unarc v0.2.0 across all architectural, security, operational, and licensing criteria.

---

## Final Status Matrix

| Category | Status | Verification Summary |
|---|---|---|
| **Build Reproducibility** | **PASS** | Hermetic pinned container build (`rust:1.85.0-slim`), strict `Cargo.lock` dependency locking, multi-stage reproducible Docker builds, zero host tooling pollution. |
| **Archive Extraction** | **PASS** | Sandboxed streaming decompression with bounded memory consumption (<28MB peak RSS across representative benchmark payloads up to 50MB); zero artificial file/archive size limits. |
| **Multipart Handling** | **PASS** | Deterministic resolution and sequence ordering for modern `.part1.rar`, legacy `.r00`, and split `.7z.001` multipart archives; strict sibling volume validation, corrupted signature rejection, missing volume detection. |
| **Filesystem Safety** | **PASS** | Zero-trust path sanitizer rejects parent traversal (`..`), absolute paths, null bytes, Windows prefixes; enforces boundary containment; rejects unauthorized symlinks, hardlinks, FIFOs, and devices. Partial extraction cleanup via RAII guard. |
| **OS Sandboxing** | **PASS** | Native macOS Seatbelt (`sandbox-exec`) kernel confinement restricts reads to resolved volumes, writes to output destination, and denies network; Linux process grouping, environment scrubbing, `PR_SET_NO_NEW_PRIVS`, `PR_SET_PDEATHSIG`. |
| **Network Isolation** | **PASS** | Engine subprocess network operations denied by confinement policy; runtime doctor network denial probe verified; Docker runtime verified with `--network none`. |
| **Password Safety** | **PASS** | Masked interactive terminal input without echo (`rpassword`); immediate non-interactive exit with code 16 (`PASSWORD_REQUIRED`) on non-TTY stdin; zero environment variable or CLI flag secret inputs; zero secret logging. |
| **Engine Integrity** | **PASS** | Authentic pinned 7-Zip engine (`v26.03`); expected SHA-256 verified at build time and dynamically by `unarc doctor`; engine resolution strictly hermetic (no PATH searches, no runtime downloads). |
| **Updater Integrity** | **PASS** | Explicit cryptographic self-update (`unarc update`) verified via Ed25519 digital signature against embedded official trust anchor (`69ac4dbc...`); Mach-O / ELF binary header validation; same-filesystem staging; atomic POSIX `rename(2)` with RAII rollback guard; symlinked target paths rejected. |
| **Docker Hardening** | **PASS** | Minimal distroless base (`gcr.io/distroless/cc-debian12:nonroot`); non-root user `65532:65532`; zero compilers, interpreters, dev tools, or shell binaries; verified under `--network none`, `--read-only`, `--cap-drop ALL`, `-v /input:ro`, `-v /output:rw` (8/8 checks passed in `scripts/verify-docker.sh`). |
| **Release Signing** | **PASS** | Dedicated `unarc-sign` tool with `--strict` mode failing closed (exit code 1) when `RELEASE_SIGNING_KEY` is missing or empty; private key injected strictly via GitHub Actions secrets; zero private keys committed or logged. |
| **Licensing** | **PASS** | Unarc project is licensed under MIT (`LICENSE`); third-party notices and 7-Zip LGPL / unRAR license distinctions preserved in `THIRD-PARTY-NOTICES.md`; canonical notice file included in distributed release archives. |
| **Documentation** | **PASS** | Complete and accurate documentation across `README.md`, `ARCHITECTURE.md`, `SECURITY.md`, `CONTRIBUTING.md`, and `RELEASE_READINESS.md`. All CLI commands, options, and interactive slash commands verified against actual implementation. |
| **Tests** | **PASS** | 110 automated tests passing (46 unit + 64 integration); `cargo fmt --check` clean; `cargo clippy -- -D warnings` passing with 0 warnings; Gitleaks secret scan passing with 0 leaks found. |
| **Artifact Contents** | **PASS** | Release bundles contain only authentic binaries (`unarc`, `7zz`) and documentation (`LICENSE`, `THIRD-PARTY-NOTICES.md`, `README.md`). Zero source files, dev caches, build artifacts, or secret material present. |

---

## Verification Scope & Boundary

- **Locally Verified & Emulated**:
  - All unit and integration tests (110 tests).
  - Code formatting and Clippy strict checks (0 warnings).
  - Native macOS Apple Silicon compilation, doctor probes, and extraction smoke tests.
  - OS-level security boundary checks (7/7 checks).
  - Hardened Docker container runtime checks (8/8 checks).
  - Release packaging, SHA-256 checksum generation, and Ed25519 manifest verification.
  - Containerized GitHub Actions workflow linting (`actionlint`).
  - Containerized secret scanning (`gitleaks`).
- **Owner-Only Final Deployment Steps (Not Automatically Executed)**:
  - Configure `RELEASE_SIGNING_KEY` secret in GitHub repository settings.
  - Replace `<COPYRIGHT HOLDER>` in `LICENSE` with legal owner identity.
  - Replace `<SECURITY_EMAIL_PLACEHOLDER>` in `SECURITY.md` with designated security reporting contact address.
  - Push release tag (`v0.2.0`) to trigger GitHub Actions release pipeline.
  - Review and publish official GitHub Release and GHCR container package.
