# Unarc Architecture & Phase 2 Decisions

This document details the system design, subsystem boundaries, and technical decisions implemented for **Phase 2** of Unarc.

---

## 1. System Architecture & Component Interactions

Unarc preserves strict unidirectional dependencies, completely isolating presentation from core and engine logic:

```
                  +---------------------------+
                  |         src/main.rs       |
                  +-------------+-------------+
                                |
                                v
                  +---------------------------+
                  |         src/cli/          |
                  | (args, output,            |
                  |  interactive shell)       |
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
| SevenZipBackend)    |                   |  containment policy)|
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

1. **`src/cli/` (Presentation & Interactive UX)**:
   - `args.rs`: Finalized subcommands (`extract`, `test`, `info`, `version`) and global flags (`--json`, `--verbose`, `--quiet`).
   - `interactive.rs`: Terminal-native interactive shell featuring:
     - ASCII wordmark and subtitle.
     - Slash command suggestions (`/extract`, `/test`, `/info`, `/doctor`, `/update`, `/config`, `/help`, `/exit`).
     - Dynamic prefix filtering (e.g. `/ex`).
     - Arrow-key navigation, Enter execution, and Tab autocomplete.
     - Non-TTY safe fallback for piped or scripted execution.
     - Zero telemetry, zero persistent state, zero history files.
   - `output.rs`: Renders human-readable formatted text or structured JSON, respecting `NO_COLOR`, `--quiet`, and `--verbose`.

2. **`src/core/` (Application Orchestrator)**:
   - `Application`: Coordinates operational execution across `SecurityPolicy`, `PlatformInfo`, and `SevenZipBackend`.
   - Both direct CLI commands and interactive slash commands invoke the exact same methods on `Application` (`extract_archive`, `test_archive`, `app_info`, `doctor_check`), eliminating duplicated business logic.

3. **`src/archive/` (Archive Subsystem & Pinned Engine)**:
   - `bundled.rs`: Implements `SevenZipBackend` backing the `ArchiveBackend` trait.
   - Resolves the bundled engine hermetically without ever searching system `PATH` or invoking Homebrew.
   - Captures process stdout/stderr and converts engine return codes into structured `UnarcError` values.

4. **`src/security/` (Zero-Trust Security Layer)**:
   - Path sanitization and containment boundary verification preventing `Zip Slip` traversal and malicious targets.

---

## 2. Phase 2 & Phase 3 Architectural Decisions

### Decision 1: Bundled & Pinned 7-Zip Engine (v26.03)
- **Version**: `26.03` (official Igor Pavlov release)
- **Source**: `https://github.com/ip7z/7zip/releases/tag/26.03`
- **Target Architectures & SHA-256**:
  - macOS (7z2603-mac.tar.xz): `5ca87677072c59f5602e5c49baa27d4694bacd2259b4e507f0094249d4281480`
  - Linux ARM64 (7z2603-linux-arm64.tar.xz): `2389ba20e4d8295e8709c20b6263b69bd1ec4972fe38a04ad7a1badbf595b996`
  - Linux x86_64 (7z2603-linux-x64.tar.xz): `dc99eff5008f1ab79bd7084c68513701547a808a89502bf4133683535ab3c695`
- **Hermetic Guarantee**:
  - The engine is verified at build-time by SHA-256.
  - Zero runtime network requests are made.
  - No system-installed or Homebrew binaries are ever called.

### Decision 2: Deterministic Multipart Volume Resolution
- **Resolution Scope**:
  - Starts strictly from the user-selected archive path.
  - Scanned strictly within the immediate parent directory (`selected_path.parent()`).
  - **Zero recursive scanning**: never scans subdirectories, user home directories, or Downloads.
- **Naming Conventions Supported**:
  - Modern multipart: `<Stem>.part<N>.rar` (detects padding width, resolves parts 1 through N).
  - Legacy multipart: `<Stem>.rar` (part 1), `<Stem>.r00` (part 2), `<Stem>.r01` (part 3), etc.
  - Split 7-Zip: `<Stem>.7z.001`, `<Stem>.7z.002`, ...
- **Sequence Integrity & Gap Detection**:
  - Starting from any volume (e.g. `Movie.part2.rar`) resolves the full sequence starting from part 1.
  - If any volume in the sequence `1..N` is missing: immediately fails with a structured `MISSING_VOLUME` error detailing the missing file.
  - If any volume is not a regular file, is a forbidden symlink, or has an invalid/corrupted archive signature: immediately fails with `INVALID_VOLUME`.
  - Checks archive headers (MAIN_HEAD and ENDARC_HEAD in RAR4/RAR5) to detect if additional subsequent volumes are expected.
  - Isolates unrelated sibling archives (e.g. `Movie2.part1.rar` vs `Movie.part1.rar`).
  - Read-only guarantees: input volumes are never renamed, deleted, moved, or altered.

### Decision 3: Production-Grade Safe Streaming Extraction
- **Streaming Execution**:
  - Archive data is streamed directly to disk via the pinned engine; no archives or member files are buffered into memory.
  - No artificial archive, file, or total-output-size limits: architectural streaming guarantees support arbitrarily large archives subject only to filesystem capacity.
- **Multi-Stage Output Safety**:
  1. *Pre-extraction validation*: Lists member entry paths and verifies every entry against `SecurityPolicy::validate_entry_path`. Any path traversal (`../`) or absolute path (`/`) is rejected before extraction begins.
  2. *Hardened extraction flags*: Engine is invoked with `-snl-` (disable symbolic link extraction) and `-snh-` (disable hard link extraction).
  3. *Post-extraction containment check*: Walks destination directory to verify canonical paths of all created entries remain bounded inside the target destination, ensuring zero leakage outside the selected root.
- **Privacy & Hygiene**:
  - No persistent extraction logs, staging caches, or history databases are created.

### Decision 4: Interactive Password Prompting & Presentation Isolation
- Passwords are never accepted via CLI flags.
- Prompts interactively using masked terminal input (`rpassword`).
- Output formatter respects `NO_COLOR`, `--quiet`, `--verbose`, and structured `--json` modes.


### Decision 5: Non-TTY and Color Handling
- If `NO_COLOR` is present in the environment, ANSI escape sequences are completely suppressed.
- If standard input is non-interactive (e.g. piped or redirected), Unarc automatically falls back to line-by-line streaming without attempting raw terminal mode manipulation.

---

## 3. Phase 4 Architectural Decisions: OS-Level Security Boundary & Process Confinement

### Decision 1: Untrusted Execution Model for Bundled Engine
- The pinned 7zz binary is treated strictly as an **untrusted execution component**.
- Application-level path sanitation (Zip Slip traversal defense, depth limits) remains active and is now backed by real **OS/process kernel containment**.

### Decision 2: Multi-Layered Subprocess Confinement
- **`src/security/sandbox.rs` (`ProcessSandboxPolicy`)**:
  - Explicit containment boundaries defining:
    - `input_files`: Strictly resolved multipart volumes granted **read-only** access.
    - `destination_dir`: Explicitly designated extraction root granted **read-write** access.
    - `scratch_dir`: Dedicated per-operation temporary directory with guaranteed RAII cleanup.
    - `deny_network`: Denies all network operations (loopback, internet, sockets).
- **macOS Native Seatbelt Isolation (`sandbox-exec`)**:
  - Dynamically synthesizes native Seatbelt Profile Language (SBPL) profiles confining the 7zz process.
  - Denies `network*` entirely.
  - Restricts file reads strictly to: engine executable, system dynamic libraries (`/usr/lib`, `/System/Library`, `/usr/share`, `/dev`), and the explicitly resolved archive volumes.
  - Restricts file writes strictly to the designated output destination and isolated scratch directory.
  - Prevents the engine from accessing user home directories, SSH keys, credentials, browser data, and unrelated directories.
- **Linux & Container Isolation Contract**:
  - Drops privileges using `PR_SET_NO_NEW_PRIVS`.
  - Pairs with Linux Landlock filesystem LSM where kernel support is present.
  - Operates cleanly within hardened Docker container boundaries.

### Decision 3: Subprocess Lifecycle & Anti-Orphan Guarantees
- **Process Group Isolation**:
  - Every child process is spawned in an independent process group (`setpgid(0, 0)`).
- **Kernel Death Signal Hook (`PR_SET_PDEATHSIG`)**:
  - On Linux, instructs the kernel to deliver `SIGKILL` to the child process tree immediately if Unarc is terminated or crashes.
- **RAII Process Termination (`ChildProcessGuard`)**:
  - Wraps spawned `std::process::Child`.
  - On `Drop` (e.g. panic, signal, cancellation, or error), sends `SIGTERM` to the entire process group (`-pgid`), waits a brief grace period, and escalates to `SIGKILL` if still alive, reaping the child.
  - Active process group tracking (`ACTIVE_PGID`) ensures clean immediate cleanup on SIGINT/SIGTERM.

### Decision 4: Environment Variable Scrubbing & Secrets Hygiene
- Subprocess invocations invoke `.env_clear()`, stripping all ambient parent environment variables.
- Host credentials (`AWS_*`, `SSH_AUTH_SOCK`, `GITHUB_TOKEN`, tokens, user secrets) are never leaked to the engine subprocess.
- Passes only the strictly minimal execution variables: `PATH` (`/usr/bin:/bin:/usr/local/bin`), `LANG` (`C.UTF-8`), `LC_ALL` (`C.UTF-8`), and `TMPDIR` (`scratch_dir`).

### Decision 5: Defense Against Special Nodes & Symlink TOCTOU
- Extraction flags `-snl-` (no symlinks) and `-snh-` (no hardlinks) are passed to the engine.
- Post-extraction safety verification traverses the output directory using `symlink_metadata` (never following symlinks):
  - Detects and immediately deletes any unauthorized symlinks (`is_symlink()`).
  - Detects and rejects hardlinks (`nlink() > 1`).
  - Detects and rejects special filesystem nodes (`is_fifo()`, `is_char_device()`, `is_block_device()`, `is_socket()`).
  - Verifies canonical containment (`canon.starts_with(dest_canonical)`).

### Decision 6: Transparent Security Diagnostics (`doctor`)
- Probes and reports real security posture without security theater:
  - `OS Sandbox Confinement`: Reports `Enforced` (macOS Seatbelt / Linux Landlock), `Degraded` (containerized process isolation active), or `Unavailable`.
  - `Environment Secrets Hygiene`: Verifies ambient environment scrubbing.
  - `Network Isolation Boundary`: Verifies network denial.
  - `Filesystem Scope Confinement`: Verifies read-only input and output write containment.

---

## 4. Phase 5 Architectural Decisions: Production Reliability, Signals & Resource Correctness

### Decision 1: Stable, Deterministic Exit Codes
Unarc formalizes a stable taxonomy of 16 structured error codes mapped to deterministic process exit codes across direct and interactive execution:

| Exit Code | `ErrorCode` Variant | Description |
|---|---|---|
| `0` | Success | Operation completed successfully |
| `2` | `CliError` | CLI argument parsing or flag validation error |
| `10` | `InputNotFound` | User-specified archive file does not exist |
| `11` | `InputNotFile` | User-specified archive path is a directory or special node |
| `12` | `UnsupportedFormat` | Archive format is unsupported or unrecognized |
| `13` | `MissingVolume` | Multipart sequence is missing a required volume |
| `14` | `InvalidVolume` | Multipart volume is corrupted, invalid signature, or not a regular file |
| `15` | `CorruptArchive` | Archive corrupted, bad CRC, or truncated header |
| `16` | `PasswordRequired` | Password is required to decrypt headers or payload |
| `17` | `InvalidPassword` | Supplied password failed authentication |
| `18` | `OutputInvalid` | Output directory is invalid or conflicts with an existing file |
| `20` | `PathTraversal` | Entry path violates boundary (Zip Slip, `../`, leading slash) |
| `21` | `UnsafeEntry` | Entry contains unauthorized symlink, hardlink, or FIFO node |
| `22` | `SecurityPolicyViolation` | Enforced security policy or kernel sandbox constraint failed |
| `30` | `PermissionDenied` | Operating system denied access to file or directory |
| `40` | `ExtractionFailed` | General extraction process error |
| `41` | `EngineFailed` | Subprocess engine execution failed or crashed |
| `130` | `Interrupted` | Execution cancelled by SIGINT (`Ctrl+C`) or SIGTERM |

### Decision 2: Interruption Handling & Clean Process Reaping
- **Signal Registration (`src/platform/signals.rs`)**:
  - Installs global POSIX signal hooks for `SIGINT` and `SIGTERM` via `signal-hook`.
  - Atomically sets a lock-free `INTERRUPTED` atomic flag (`Ordering::SeqCst`).
- **Immediate Subprocess Group Termination**:
  - Signal hooks immediately signal the active child process group via `libc::kill(-pgid, libc::SIGKILL)`.
  - Guarantees child engines (7zz) cannot outlive the parent Unarc process.
- **Child Process Guard Reaping**:
  - `ChildProcessGuard` tracks the child PID and process group ID.
  - On `Drop`, sends `SIGTERM`, waits a 20ms grace period, escalates to `SIGKILL` if still alive, and reaps the child status to prevent zombie processes.
- **Periodic Interruption Checks**:
  - Application loops (`extract_archive`, `test_archive`, `verify_extracted_destination`) check `is_interrupted()` at critical boundaries and abort cleanly with `UnarcError::Interrupted`.

### Decision 3: Partial Extraction Cleanup Guard
- **`PartialExtractionGuard` (`src/core/app.rs`)**:
  - Snapshots pre-existing files and directories within the destination directory before extraction begins.
  - Implements RAII `Drop`: if extraction fails or is interrupted before clean completion, it systematically removes all newly created files and directories.
  - **Pre-existing file protection**: files and directories present before extraction began are preserved untouched.
  - Disarmed strictly when extraction completes, verification passes, and all security boundaries are satisfied.

### Decision 4: Interactive Password Prompts & Secret Hygiene
- **Distinction**:
  - Clean separation between `PASSWORD_REQUIRED` (code 16) when no password was provided and `INVALID_PASSWORD` (code 17) when the provided password fails verification.
- **Terminal Hygiene**:
  - Passwords are never accepted through environment variables or CLI flags.
  - Uses `rpassword::prompt_password` for terminal input without echoing to the screen.
  - Automated replacement secret transport is intentionally deferred to a future phase.
  - Passwords are never logged, never cached in memory longer than the operation duration, and never leaked to persistent files.

### Decision 5: Bounded Resource Footprint & Benchmark Scope
- **Streaming Architectural Guarantee**:
  - Operates via streaming I/O; never loads archive files or extraction payloads into memory buffers.
  - Architecture imposes no artificial limits on archive, file, or total output size.
- **Representative Benchmark Suite (`scripts/benchmark.sh`)**:
  - Synthetic test suite evaluating:
    - Small archive (100 KB text)
    - Medium archive (10 MB binary)
    - Large archive (50 MB binary streaming)
    - Multipart archive (10 MB split across 5 MB volumes)
  - **Memory Boundedness**: Peak resident set size (RSS) remains bounded strictly under 28MB across all sizes (50MB payload consumes only 27.4 MB RSS).
  - **Decompression Throughput**: 190–770 MB/s streaming decompression.
  - **Benchmark Scope Distinction**: The automated benchmark suite validates bounded-memory behavior on representative sizes (up to 50MB); empirical 100GB+ extraction is not part of the current automated test run and is bounded only by underlying disk storage.

---

## 5. Phase 6 Architectural Decisions: Binary Integrity, Engine Verification & Cryptographic Self-Update

### Decision 1: Canonical Release Integrity Manifest (`src/security/integrity.rs`)
- **Structure**:
  - `unarc_version`: Package version (`CARGO_PKG_VERSION`).
  - `target_os`: Operating system identifier (`std::env::consts::OS`).
  - `target_arch`: Target architecture (`std::env::consts::ARCH`).
  - `bundled_7zz_version`: Pinned 7-Zip release version (`26.03`).
  - `bundled_7zz_sha256`: Expected SHA-256 hash of the extracted `7zz` executable on disk.
- **Fail-Closed Runtime Verification**:
  - `verify_bundled_engine_integrity(path)` computes the actual SHA-256 hash of the bundled 7zz binary on disk and verifies it against the compile-time pinned hash.
  - Verification is mandatory and runs upfront before any engine invocation in `extract_archive` and `test_archive`.
  - Any mismatch immediately fails closed with `SecurityError::PolicyViolation` / `ErrorCode::SecurityPolicyViolation` (exit code `22`). Unarc never silently continues with a tampered engine.

### Decision 2: Cryptographic Release Verification (Ed25519 & SHA-256)
- **Zero Engine Dependency**: Verification is implemented using pure Rust (`ed25519-dalek` and `sha2`), completely independent of the archive extraction engine.
- **Release Metadata Manifest (`ReleaseManifest`)**:
  - Encodes release metadata: version, target OS, target architecture, bundled 7zz version, expected 7zz SHA-256, artifact download URL, and artifact SHA-256.
  - Signed with an Ed25519 private key; verified using Unarc's official public key (`ReleaseSignatureVerifier`).
  - Untrusted Source Defense: Content is never trusted simply because it originated from a specific URL or host. Verification requires valid cryptographic signature, architecture compatibility, and matching SHA-256 hash.

### Decision 3: Explicit Self-Update & Transactional Atomic Rollback (`src/core/update.rs`)
- **Strict User Invocation**:
  - `unarc update`: Explicit user action only. No background daemons, cron jobs, background threads, or automatic polling.
  - `--check`: Probes configured source for new versions without downloading or applying changes.
  - `--source <url|path>`: Allows specifying custom release sources (e.g. for offline airgapped updates or test fixtures).
- **Update Verification Pipeline**:
  1. *Target Executable Validation*: Upfront validation of the target binary path; immediately rejects updating through a symlink (`ErrorCode::UnsafeEntry`, exit code 21) before contacting update sources.
  2. *Query*: Fetches `manifest.json` from the source.
  3. *Cryptographic Signature Check*: Verifies Ed25519 signature over canonical manifest bytes against the official embedded trust anchor.
  4. *Architecture Compatibility Check*: Asserts `target_os == CURRENT_OS` and `target_arch == CURRENT_ARCH`.
  5. *Atomic Staging*: Downloads the release artifact into an adjacent temporary staging file (`.unarc-update-staging-<pid>-<rand>`) strictly in the target binary's parent directory, guaranteeing same-filesystem atomic `rename(2)`.
  6. *Checksum Verification*: Computes SHA-256 on the staged file; asserts exact match with `artifact_sha256`.
  7. *Executable Format Verification*: Checks binary magic bytes: Mach-O on macOS (`0xFEEDFACF`, etc.) and ELF on Linux (`0x7F 'E' 'L' 'F'`). Never executes an unverified binary.
  8. *Permissions*: Applies POSIX `0755` executable permissions to the staged file prior to replacement.
  9. *Atomic Replacement*: Invokes POSIX `std::fs::rename`, replacing the existing executable in a single atomic filesystem operation.
- **Rollback Guarantee**:
  - Backed by RAII `StagingGuard`: any failure before the final atomic rename leaves the existing binary completely untouched and cleans up the staging file.
  - Bundled 7zz Engine Immutability: The bundled 7zz engine is never updated independently at runtime; engine updates may only arrive as part of a verified, signed new Unarc release.

### Decision 4: Extended Doctor Diagnostics
- `unarc doctor` probes expanded to verify:
  - `Engine Binary Integrity`: Checks actual disk SHA-256 of 7zz against expected hash.
  - `Release Identity & Manifest`: Verifies canonical manifest identity and architecture compatibility.
  - `Cryptographic Update Verifier`: Validates Ed25519 verification engine readiness and public key configuration.

---

## 6. Phase 7 Architectural Decisions: Hardened Docker Runtime, CI Automation, Native macOS Packaging & GHCR

### Decision 1: Hardened Distroless Production Docker Runtime
- **Multi-Stage Container Architecture**:
  - `dev` stage: `rust:1.85.0-slim` pinned toolchain for hermetic development, clippy, and testing.
  - `builder` stage: compiles stripped, optimized release binaries with `panic = "abort"`, `lto = true`, `opt-level = 3`.
  - `runtime` stage: based strictly on `gcr.io/distroless/cc-debian12:nonroot`.
- **Zero Attack Surface & Attack Tool Elimination**:
  - Contains NO compilers (`gcc`, `rustc`), NO build utilities (`cargo`, `make`), NO dev tools (`git`, `curl`), and NO package managers (`apt`, `dpkg`).
  - Contains NO shell interpreter: `/bin/sh` and `/bin/bash` are absent from the runtime image.
  - Contains strictly the two runtime binaries: `/usr/local/bin/unarc` and `/opt/unarc/bin/7zz`, dynamically linked to minimal system `glibc` and `libstdc++`.

### Decision 2: Principle of Least Privilege in Container Invocation
- **Strict Non-Root Runtime User**:
  - Executes as unprivileged user `nonroot:nonroot` (`UID:GID 65532:65532`).
- **Read-Only Root Compatibility**:
  - Designed to execute under `docker run --read-only`.
  - An anonymous volume (`VOLUME ["/tmp"]`) provides isolated temporary scratch space with standard `1777` sticky permissions.
- **Full Linux Capability Dropping**:
  - Designed to operate with `--cap-drop ALL`, preventing privilege escalation.
- **Explicit Network Denial**:
  - Operates under `--network none`. Unarc extraction processes cannot establish outbound or inbound network connections.
- **Asymmetric Filesystem Mounts**:
  - Archive inputs mounted strictly read-only (`:ro`).
  - Output destination directory mounted explicitly writable (`:rw`). Attempts to write to `/input` or root `/` fail immediately.

### Decision 3: Automated Multi-Platform GitHub Actions CI/CD Pipeline
- **Separation of Concerns**:
  - `ci.yml`: Standard PR and branch CI running pinned format check, Clippy lints with warnings denied, unit/integration tests, and hardened Docker runtime verification.
  - `release-macos.yml`: Builds and packages native Apple Silicon (`aarch64-apple-darwin`) release bundle with pinned 7zz and Ed25519 manifest signature.
  - `release-linux.yml`: Matrix build for Linux `x86_64` and `aarch64` native bundles.
  - `docker-publish.yml`: Multi-arch QEMU/Buildx compilation, verification via `scripts/verify-docker.sh`, and publication to GHCR with automated version pruning.
  - `release.yml`: Coordinates cross-platform release builds, performs pre-publication artifact and signature verification, and creates GitHub Releases with immutable assets.
- **Fail-Closed Enforcement**:
  - Any failure in formatting, linting, tests, security probes, checksum checks, or cryptographic signature verification fails the pipeline immediately.

### Decision 4: Standalone Self-Contained Apple Silicon Native Packaging
- **Zero Host Prerequisites**:
  - macOS release tarball (`unarc-<version>-macos-arm64.tar.gz`) bundles both `unarc` and authentic pinned `7zz` (v26.03, verified by SHA-256).
  - Operates standalone: end users require no Homebrew, Python, Docker, or system-installed 7-Zip.
  - Mach-O executable architecture verified as 64-bit arm64.

### Decision 5: Separation of Update Topologies
- **Native Host Executables**:
  - Managed via `unarc update`. Atomically staged adjacent to target executable and replaced via `rename(2)` after Ed25519 signature and SHA-256 verification.
- **Container Environments**:
  - Docker containers are immutable. In-place binary self-updates are not applicable inside containers; updates are achieved exclusively by pulling new immutable semantic image tags from GHCR.

### Decision 6: GHCR Image Tagging & Automated Retention
- **Semantic Version Tags**:
  - `ghcr.io/mosabbir-maruf/unarc:<version>` serves as the immutable release identity.
  - `latest` and major/minor tags (`0.2`) are provided exclusively as convenience pointers and never as integrity anchors.
- **Automated Retention**:
  - CI policy prunes untagged and older image tags, retaining the latest two semantic release versions to prevent unbounded registry bloat.
