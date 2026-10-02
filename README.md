# Unarc

A production-grade, security-focused archive utility CLI in Rust.

Designed targeting **macOS Apple Silicon** first, with full **Docker/Linux** development and CI support. Unarc enforces zero-trust path validation, interactive terminal UX, and a hermetically bundled archive engine.

---

## Key Principles & Scope

- **Zero Host Pollution**: All Rust compilation, formatting, linting, and testing run strictly inside pinned Docker containers (`rust:1.85.0-slim`).
- **Hermetic Pinned Archive Engine**: Bundles an exact official release of 7-Zip (`7zz v26.03`). The engine is verified by SHA-256 and never downloaded at runtime or invoked from host package managers (like Homebrew).
- **Interactive & Scriptable UX**: Supports both direct script commands and an interactive terminal shell with an ASCII wordmark, `/` command suggestions, arrow-key navigation, and Tab completion.
- **Zero-Trust Extraction**: Comprehensive path traversal protection (`Zip Slip`), absolute path rejection, null-byte checks, and boundary containment enforcement.
- **Production Reliability & Cleanup**: Deterministic exit codes, clean SIGINT/SIGTERM cancellation without zombie processes, and automatic partial extraction cleanup on failure.
- **Privacy & Safety**: No telemetry, analytics, updater, network services, background daemons, persistent state, or uncoordinated file logging.

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

# Display binary version
unarc version
```

> **Note on Password-Protected Archives**: When an archive is encrypted, Unarc prompts interactively via masked terminal input (`rpassword`) without echo. In headless/scripted environments, the `UNARC_PASSWORD` environment variable is supported.

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
- `/update`: Inspects engine pinning and hermetic policy.
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
| **Encrypted (5MB)** | 5.1 MB | 5.00 MB | 108 ms | 16.00 MB | 45.88 MB/s | PASS |

- **Bounded Memory**: Peak RSS remains under 30MB even for large streaming extractions.
- **High Throughput**: 190–770 MB/s streaming decompression.

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
```
