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

### Decision 1: Bundled & Pinned 7-Zip Engine (v24.09)
- **Version**: `24.09` (official Igor Pavlov release)
- **Source**: `https://github.com/ip7z/7zip/releases/tag/24.09`
- **Target Architectures & SHA-256**:
  - macOS (Universal `aarch64` + `x86_64`): `bd5765978a541323758d82ad1d30df76a2e3c86341f12d6b0524d837411e9b4a`
  - Linux ARM64 (`aarch64`): `ea6a2595eba6441e1e60ddaa47d73d849e99ef2ba18d3f386557cdcb9dc9cebd`
  - Linux x86_64 (`x86_64`): `9a556170350dafb60a97348b86a94b087d97fd36007760691576cac0d88b132b`
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
