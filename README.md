# Unarc

A production-grade, security-focused archive utility CLI in Rust.

Designed targeting **macOS Apple Silicon** first, with full **Docker/Linux** development and CI support. Unarc enforces zero-trust path validation, interactive terminal UX, and a hermetically bundled archive engine.

---

## Key Principles & Scope

- **Zero Host Pollution**: All Rust compilation, formatting, linting, and testing run strictly inside pinned Docker containers (`rust:1.85.0-slim`).
- **Hermetic Pinned Archive Engine**: Bundles an exact official release of 7-Zip (`7zz v24.09`). The engine is verified by SHA-256 and never downloaded at runtime or invoked from host package managers (like Homebrew).
- **Interactive & Scriptable UX**: Supports both direct script commands and an interactive terminal shell with an ASCII wordmark, `/` command suggestions, arrow-key navigation, and Tab completion.
- **Zero-Trust Extraction**: Comprehensive path traversal protection (`Zip Slip`), absolute path rejection, null-byte checks, and boundary containment enforcement.
- **Privacy & Safety**: No telemetry, analytics, updater, network services, background daemons, persistent state, or uncoordinated file logging.

---

## Pinned 7-Zip (7zz) Engine

| Attribute | Value |
|---|---|
| **Version** | `24.09` |
| **Release Location** | `https://github.com/ip7z/7zip/releases/tag/24.09` |
| **macOS (Universal arm64/x86_64)** | `bd5765978a541323758d82ad1d30df76a2e3c86341f12d6b0524d837411e9b4a` |
| **Linux ARM64 (aarch64)** | `ea6a2595eba6441e1e60ddaa47d73d849e99ef2ba18d3f386557cdcb9dc9cebd` |
| **Linux x86_64** | `9a556170350dafb60a97348b86a94b087d97fd36007760691576cac0d88b132b` |

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

# Display binary version
unarc version
```

> **Note on Password-Protected Archives**: If an archive is encrypted, Unarc prompts for the password interactively via the terminal. No plaintext `--password` CLI argument is exposed.

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
```
