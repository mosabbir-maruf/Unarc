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
  - No artificial archive or extraction file-size limits: supports legitimate 100GB+ archives subject only to filesystem capacity.
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
