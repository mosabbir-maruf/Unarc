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

## 2. Phase 2 Architectural Decisions

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

### Decision 2: Interactive Password Prompting
- Encryption passwords are never accepted via command-line arguments to prevent exposing sensitive secrets in shell history or process tables (`ps aux`).
- When an archive requires a password, Unarc prompts the user interactively (suppressing terminal echo via `rpassword`).

### Decision 3: Unified Execution Path
- Direct CLI mode and interactive mode share the exact same underlying logic in `Application`.
- Subcommands like `inspect` and `validate` are retained internally as foundational building blocks without exposing redundant public CLI commands.

### Decision 4: Non-TTY and Color Handling
- If `NO_COLOR` is present in the environment, ANSI escape sequences are completely suppressed.
- If standard input is non-interactive (e.g. piped or redirected), Unarc automatically falls back to line-by-line streaming without attempting raw terminal mode manipulation.
