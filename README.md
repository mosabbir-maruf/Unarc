# Unarc

An open-source, production-grade, security-focused archive utility CLI in Rust.

Designed targeting **macOS Apple Silicon** (`aarch64-apple-darwin`) first, with full **Linux** (`x86_64`, `aarch64`) and **Docker** production container support. Unarc enforces zero-trust path validation, interactive terminal UX, and a hermetically bundled archive engine.

---

## Key Principles & Scope

- **Open Source & Permissive**: Unarc is open source and licensed under the MIT License.
- **Zero Host Pollution**: All Rust compilation, formatting, linting, and testing run strictly inside pinned Docker containers (`rust:1.85.0-slim`).
- **Hermetic Pinned Archive Engine**: Bundles an exact official release of 7-Zip (`7zz v26.03`). The engine is verified by SHA-256 and never downloaded at runtime or invoked from host package managers (like Homebrew). The bundled engine is separately licensed (GNU LGPL v2.1+ with unRAR restriction).
- **Interactive & Scriptable UX**: Supports both direct script commands and an interactive terminal shell with an ASCII wordmark, `/` command suggestions, arrow-key navigation, and Tab completion.
- **Zero-Trust Extraction**: Comprehensive path traversal protection (`Zip Slip`), absolute path rejection, null-byte checks, and boundary containment enforcement.
- **Production Reliability & Cleanup**: Deterministic exit codes, clean SIGINT/SIGTERM cancellation without zombie processes, and automatic partial extraction cleanup on failure.
- **Privacy & Safety**: Completely offline extraction behavior, zero telemetry, zero analytics, zero background daemons, no automatic background updaters, no persistent state, and zero uncoordinated file logging. Updates run strictly via explicit user invocation (`unarc update`).

---

## Pinned 7-Zip (7zz) Engine

| Attribute | Value |
|---|---|
| **Version** | `26.03` |
| **Release Location** | `https://github.com/ip7z/7zip/releases/tag/26.03` |
| **macOS (7z2603-mac.tar.xz)** | `5ca87677072c59f5602e5c49baa27d4694bacd2259b4e507f0094249d4281480` |
| **Linux ARM64 (7z2603-linux-arm64.tar.xz)** | `2389ba20e4d8295e8709c20b6263b69bd1ec4972fe38a04ad7a1badbf595b996` |
| **Linux x86_64 (7z2603-linux-x64.tar.xz)** | `dc99eff5008f1ab79bd7084c68513701547a808a89502bf4133683535ab3c695` |

---

## Exit Codes

Unarc defines deterministic, script-friendly exit codes for all operations:

| Code | Identifier | Description |
|---|---|---|
| `0` | `SUCCESS` | Operation completed successfully |
| `2` | `CLI_ERROR` | Invalid command line arguments or flags |
| `10` | `INPUT_NOT_FOUND` | Archive file does not exist |
| `11` | `INPUT_NOT_FILE` | Archive path points to a directory or non-regular file |
| `12` | `UNSUPPORTED_FORMAT` | Archive format is unsupported or unknown |
| `13` | `MISSING_VOLUME` | Multipart sequence is missing a required volume |
| `14` | `INVALID_VOLUME` | Sibling volume is corrupt, not a file, or invalid |
| `15` | `CORRUPT_ARCHIVE` | Archive data or header is corrupted |
| `16` | `PASSWORD_REQUIRED` | Archive is password-protected and no password was supplied |
| `17` | `INVALID_PASSWORD` | Supplied archive password is incorrect |
| `18` | `OUTPUT_INVALID` | Extraction output destination is invalid or a non-directory file |
| `20` | `PATH_TRAVERSAL` | Archive contains entries attempting Zip Slip path traversal |
| `21` | `UNSAFE_ENTRY` | Archive contains unauthorized symlinks, hardlinks, or special nodes |
| `22` | `SECURITY_POLICY_VIOLATION` | Sandbox or execution boundary constraint violated |
| `30` | `PERMISSION_DENIED` | Filesystem permission denied during read or write operations |
| `40` | `EXTRACTION_FAILED` | Archive extraction failed |
| `41` | `ENGINE_FAILED` | Internal bundled archive engine failed to execute |
| `130` | `INTERRUPTED` | Execution interrupted by SIGINT / SIGTERM signal |

---

## CLI Usage

### Direct / Script Mode

```bash
# Securely extract an archive to a destination
unarc extract path/to/archive.rar --output /path/to/output

# Test integrity of an archive without writing to disk
unarc test path/to/archive.7z

# Display platform, engine status, and capabilities
unarc info
unarc --json info

# Run system security and health diagnostics
unarc doctor
unarc --json doctor

# Check for available signed updates
unarc update --check

# Apply cryptographically verified self-update
unarc update

# Display binary version
unarc version
```

> **Note on Password-Protected Archives**: When an archive is encrypted, Unarc prompts interactively via masked terminal input (`rpassword`) without echo. Passwords are never accepted through environment variables or CLI flags; automated replacement secret transport is intentionally deferred.

### Interactive Mode

Launch `unarc` without subcommands to enter the interactive shell:
```bash
unarc
```

```
 _   _ _   _   _    ____   ____ 
| | | | \ | | / \  |  _ \ / ___|
| | | |  \| |/ _ \ | |_) | |    
| |_| | |\  / ___ \|  _ <| |___ 
 \___/|_| \_/_/   \_\_| \_\\____|
          Secure Archive Utility
```

Interactive Features:
- Type `/` to display slash command suggestions.
- Prefix filtering: e.g. `/ex` filters to `/extract` and `/exit`.
- Use **Up/Down Arrow keys** to navigate suggestions.
- Press **Tab** to autocomplete.
- Press **Enter** to execute.

Available Interactive Commands:
- `/extract`: Prompts for archive path and destination, then extracts securely.
- `/test`: Tests archive integrity.
- `/info`: Displays platform, engine, and security details.
- `/doctor`: Runs diagnostic health checks.
- `/update`: Checks for and applies cryptographically verified self-update.
- `/config`: Displays active security policy.
- `/help`: Displays command reference.
- `/exit`: Exits the interactive shell.

---

## Performance & Benchmarks

Unarc streams extraction directly through the sandboxed engine to ensure bounded memory consumption regardless of archive payload size:

```bash
# Run the automated benchmark suite
make bench
```

### Benchmark Results (Apple Silicon M-Series)

| Scenario | Archive Size | Extracted Size | Wall Time | Peak RSS | Throughput | Status |
|---|---|---|---|---|---|---|
| **Small (100KB)** | 0.3 KB | 0.10 MB | 528 ms | 3.97 MB | 0.18 MB/s | PASS |
| **Medium (10MB)** | 10.2 MB | 10.00 MB | 52 ms | 27.20 MB | 189.61 MB/s | PASS |
| **Large (50MB)** | 51.2 MB | 50.00 MB | 65 ms | 27.44 MB | 768.26 MB/s | PASS |
| **Multipart (10MB)** | 5.1 MB | 10.00 MB | 42 ms | 2.27 MB | 233.09 MB/s | PASS |

- **Bounded Memory**: Peak RSS remains strictly bounded under 28MB across all representative streaming extractions.
- **High Throughput**: 190–770 MB/s streaming decompression.
- **Architectural Streaming Guarantee**: Unarc imposes no artificial file-size, archive-size, or total-output-size limits. The current benchmark suite empirically validates bounded-memory behavior on representative test payloads up to 50MB; empirical multi-gigabyte or 100GB+ extraction is not part of the current automated test run and is constrained solely by host disk storage.

---

## Development & Automation Commands

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

# Run performance benchmark suite
make bench

# Build production hardened distroless Docker image
make docker-prod
```

---

## Native macOS Installation (Apple Silicon arm64)

Unarc provides self-contained native release packages for macOS Apple Silicon (`aarch64-apple-darwin`).
**Zero dependencies required**: End users do not need Rust, Cargo, Homebrew, Python, Docker, or system-installed 7-Zip.

### 1. Download & Verify Release Artifacts

Every GitHub Release includes three matching artifacts:
- `unarc-<version>-macos-arm64.tar.gz` (Archive bundle containing `unarc`, authentic pinned `7zz`, and docs)
- `unarc-<version>-macos-arm64.tar.gz.sha256` (SHA-256 digest)
- `unarc-<version>-macos-arm64.manifest.json` (Cryptographically signed Ed25519 metadata manifest)

```bash
# 1. Verify SHA-256 checksum
shasum -a 256 -c unarc-0.2.0-macos-arm64.tar.gz.sha256

# 2. Extract into user binary directory
mkdir -p ~/.local/bin
tar -xzf unarc-0.2.0-macos-arm64.tar.gz -C /tmp
mv /tmp/unarc-0.2.0-macos-arm64/unarc ~/.local/bin/
mv /tmp/unarc-0.2.0-macos-arm64/7zz ~/.local/bin/
rm -rf /tmp/unarc-0.2.0-macos-arm64

# 3. Verify installation integrity
unarc version
unarc doctor
```

---

## Hardened Docker Runtime (Production Linux Container)

Unarc is distributed as an ultra-minimal, hardened container image on GitHub Container Registry (GHCR): `ghcr.io/<owner>/unarc:<version>`.

### Container Hardening Specifications

- **Minimal Distroless Base**: Based on `gcr.io/distroless/cc-debian12:nonroot`.
- **Zero Build / Development Tools**: Contains NO Rust toolchain, Cargo, gcc, make, curl, git, Python, apt, or dpkg.
- **No Shell Tooling**: Contains NO `/bin/sh` or `/bin/bash` in the runtime image.
- **Strict Non-Root Execution**: Runs strictly under unprivileged user `nonroot:nonroot` (`UID:GID 65532:65532`).
- **Read-Only Root Filesystem**: Compatible with `--read-only`; temporary scratch workspace is isolated in an anonymous `/tmp` volume.
- **Full Capability Dropping**: Operates with `--cap-drop ALL`.
- **Explicit Network Denial**: Operates under `--network none`.

### Production Secure Invocation

To extract archives with maximal OS-level and container isolation:

```bash
docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "/host/path/to/input:/input:ro" \
  -v "/host/path/to/output:/output:rw" \
  ghcr.io/<owner>/unarc:0.2.0 \
  extract /input/archive.rar --output /output
```

To test archive integrity without write access:

```bash
docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  -v "/host/path/to/input:/input:ro" \
  ghcr.io/<owner>/unarc:0.2.0 \
  test /input/archive.zip
```

---

## Release Verification & Trust Anchors

### 1. SHA-256 Checksum Verification
```bash
shasum -a 256 -c unarc-0.2.0-macos-arm64.tar.gz.sha256
```

### 2. Ed25519 Cryptographic Manifest Signature
Every release manifest is cryptographically signed using Unarc's release key. The public key is permanently pinned in Unarc binary builds:
- **Official Public Key (hex)**: `69ac4dbc8ef560b61acdad8772ac647cb009c07d49543489bc635aef69e89b4c`

---

## Update Behavior & Topology

- **Native Binaries (`unarc update`)**:
  - `unarc update --check`: Inquires configured release source for newer versions.
  - `unarc update`: Downloads release manifest, verifies Ed25519 signature against the embedded trust anchor, verifies SHA-256 checksum and executable format (Mach-O / ELF), stages adjacent in target directory, and atomically replaces the binary via `rename(2)` with RAII rollback protection.
  - Rejects updating through symlinks (`ErrorCode::UnsafeEntry`, exit code 21).
- **Container Deployments**:
  - Docker containers are immutable. In-place self-update (`unarc update`) is not supported and should not be used inside containers.
  - Updates occur strictly by pulling new immutable version tags:
    ```bash
    docker pull ghcr.io/<owner>/unarc:0.2.1
    ```
- **GHCR Image Tagging & Retention**:
  - **Immutable Release Tags**: `ghcr.io/<owner>/unarc:0.2.0` (primary integrity identity).
  - **Convenience Tags**: `ghcr.io/<owner>/unarc:0.2` and `ghcr.io/<owner>/unarc:latest`.
  - **Retention Policy**: GHCR automated retention prunes untagged and older image tags, retaining the latest 2 semantic release versions.

---

## Security Assumptions & Limitations

- **macOS Native**: Uses native Seatbelt sandbox (`sandbox-exec`) where kernel confinement is available, strictly scoping file reads to resolved archive volumes, writes to output destination, and denying network access.
- **Linux Container**: Enforces process group isolation, environment purging, `PR_SET_NO_NEW_PRIVS`, `PR_SET_PDEATHSIG`, and relies on host container boundaries (`--network none`, `--read-only`, `--cap-drop ALL`, non-root user). Kernel-level LSM sandboxing is reported accurately as `DEGRADED` in containerized environments.
- **Zero Silent Fallback**: If required security constraints fail, Unarc fails closed with structured exit codes.

---

## License & Third-Party Notices

Unarc is open-source software licensed under the **[MIT License](LICENSE)**.

- **Unarc Codebase**: Licensed under the MIT License (see [LICENSE](LICENSE)). Copyright (c) 2026 Unarc Contributors.
- **Bundled Engine (7-Zip / 7zz)**: Pinned 7-Zip (`7zz v26.03`) is developed by Igor Pavlov and is separately licensed under the **GNU LGPL v2.1+** (with the unRAR license restriction for RAR archive decompression and BSD/Public Domain portions for LZMA and 7z components). See [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) for details and full license terms.
- **Third-Party Dependencies**: All third-party Rust libraries retain their original permissive licenses (MIT, Apache-2.0, BSD-3-Clause). Complete notices and dependency attributions are available in [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES).
