# Unarc

An open-source, production-grade, security-focused archive utility CLI in Rust.

Targeting **macOS Apple Silicon** (`aarch64-apple-darwin`), **Linux** (`x86_64`, `aarch64`), and **Docker** distroless container runtime. Unarc enforces zero-trust path validation, interactive terminal UX, deterministic exit codes, and an authentic, hermetically bundled archive engine (`7zz v26.03`).

---

## Quick Start

Choose the path that fits your needs:

| Audience | Use Case | Distribution Path | Toolchain Required |
|---|---|---|---|
| **Native End User** *(Recommended)* | "I just want to use Unarc on my computer" | [**GitHub Releases**](#native-installation-recommended) | **None** (Single self-contained executable) |
| **Docker User** | "I want to use Docker without installing native binaries" | [**GHCR Container Image**](#docker-ghcr) | **Docker only** (Copy/paste canonical command) |
| **Developer / Contributor** | "I want to build from source or contribute code" | [**Source Repository**](#developer--build-from-source) | **Rust & Cargo** (`git clone` + `cargo run`) |

---

### Path 1: Native End User (Recommended)
Download the standalone, self-contained `unarc` executable for your system from [GitHub Releases](https://github.com/mosabbir-maruf/Unarc/releases/latest). Unarc natively embeds the authentic pinned `7zz v26.03` engine—no Rust, Cargo, 7-Zip, Homebrew, Docker, or runtime dependencies are required.

After downloading:
```bash
# Make executable and run immediately:
chmod +x unarc
./unarc

# Or move to your PATH (e.g., ~/.local/bin) to run anywhere:
mv unarc ~/.local/bin/
unarc
```

### Path 2: Docker User (GHCR)
Run Unarc inside an isolated, hardened distroless container directly from the GitHub Container Registry. You do not need to memorize this command—simply copy and paste the canonical invocation:
```bash
docker run -it --rm -v "$PWD:/work" ghcr.io/mosabbir-maruf/unarc:latest
```

### Path 3: Developer / Build From Source
Clone the repository and compile locally using Cargo:
```bash
git clone https://github.com/mosabbir-maruf/Unarc.git
cd Unarc
cargo run --release
```
*(Note: `--release` is Cargo's compiler optimization profile for fast native execution, not a GitHub Release.)*

---

## Native Installation (Recommended)

Unarc is distributed as a **single, self-contained native executable** for supported operating systems.
**Zero runtime dependencies**: Normal users do not need Rust, Cargo, 7zz, Homebrew, Python, or Docker.

### True Single-File Architecture
- **Embedded Archive Engine**: The authentic, pinned 7-Zip engine binary (`v26.03`) is embedded directly into the `unarc` executable at compile time.
- **On-Demand Materialization**: When an archive operation begins, Unarc securely materializes the embedded engine into a private, restricted temporary directory (`0700` directory mode, `0500` read/execute non-writable binary).
- **Cryptographic Verification**: The engine is verified against its pinned official SHA-256 hash both at compile time and before execution at runtime.
- **Automatic Cleanup**: Temporary materialized engine directories are automatically purged upon process termination and safely swept on startup.
- **Air-Gapped Operation**: Unarc never queries system `PATH`, never invokes Homebrew, and never downloads binaries from the internet at runtime.

### Supported Platforms
- **macOS Apple Silicon**: `macos-arm64` (`aarch64-apple-darwin`)
- **Linux x86_64**: `linux-x86_64` (`x86_64-unknown-linux-gnu`)
- **Linux ARM64**: `linux-aarch64` (`aarch64-unknown-linux-gnu`)

### Quick Download & Run (Manual)
1. Go to [GitHub Releases (Latest)](https://github.com/mosabbir-maruf/Unarc/releases/latest).
2. Download the standalone executable for your operating system:
   - `unarc-0.2.0-macos-arm64` (macOS Apple Silicon)
   - `unarc-0.2.0-linux-x86_64` (Linux 64-bit Intel/AMD)
   - `unarc-0.2.0-linux-aarch64` (Linux 64-bit ARM)
3. Make it executable and run:
   ```bash
   chmod +x unarc-0.2.0-macos-arm64
   ./unarc-0.2.0-macos-arm64
   ```
4. *(Optional)* Move it to your `PATH` (such as `~/.local/bin/unarc`) to run from any terminal:
   ```bash
   mkdir -p ~/.local/bin
   mv unarc-0.2.0-macos-arm64 ~/.local/bin/unarc
   chmod 0755 ~/.local/bin/unarc
   unarc
   ```

*(Note: `.tar.gz` bundles containing documentation and licenses are also provided alongside for package maintainers.)*

### Scripted Installation (Terminal)

You can download and verify the standalone executable directly from your shell:

```bash
# 1. Set target platform (options: macos-arm64, linux-x86_64, linux-aarch64)
PLATFORM="macos-arm64"

# 2. Check the latest release tag at https://github.com/mosabbir-maruf/Unarc/releases/latest
VERSION="0.2.0"

# 3. Download the standalone executable and SHA-256 checksum
curl -sSLO "https://github.com/mosabbir-maruf/Unarc/releases/download/v${VERSION}/unarc-${VERSION}-${PLATFORM}"
curl -sSLO "https://github.com/mosabbir-maruf/Unarc/releases/download/v${VERSION}/unarc-${VERSION}-${PLATFORM}.sha256"

# 4. Verify checksum
# macOS:
shasum -a 256 -c "unarc-${VERSION}-${PLATFORM}.sha256"
# Linux:
# sha256sum -c "unarc-${VERSION}-${PLATFORM}.sha256"

# 5. Install executable into PATH
mkdir -p ~/.local/bin
mv "unarc-${VERSION}-${PLATFORM}" ~/.local/bin/unarc
chmod 0755 ~/.local/bin/unarc
rm -f "unarc-${VERSION}-${PLATFORM}.sha256"

# 6. Verify installation
unarc version
unarc doctor
```

> [!TIP]
> Ensure `~/.local/bin` is in your `PATH` (e.g., `export PATH="$HOME/.local/bin:$PATH"` in `~/.zshrc` or `~/.bashrc`).

---

## Docker (GHCR)

Unarc is published as an ultra-minimal distroless container image on GitHub Container Registry (GHCR): `ghcr.io/mosabbir-maruf/unarc:latest`.

### Key Characteristics
- **Zero Host Installation**: No Rust, Cargo, or 7zz required on your host machine.
- **Direct Container Distribution**: Pulled directly from GHCR without cloning source code.
- **Automatic Working Directory**: The container defaults to `/work`. Mounting `-v "$PWD:/work"` maps your current host directory directly into `/work`, so relative paths to your local files work naturally.
- **Copy/Paste Friendly**: You do not need to memorize Docker flags or paths. Simply copy and paste the canonical commands below whenever you need to run Unarc.

### Canonical Docker Invocations

#### 1. Pull the Image
```bash
docker pull ghcr.io/mosabbir-maruf/unarc:latest
```

#### 2. Interactive Mode (Default)
Copy and paste this canonical invocation to launch the interactive Unarc shell:
```bash
docker run -it --rm -v "$PWD:/work" ghcr.io/mosabbir-maruf/unarc:latest
```

#### 3. Direct Subcommand Examples
Pass CLI subcommands as trailing arguments after the image name:

- **Test Archive Integrity**:
  ```bash
  docker run --rm -v "$PWD:/work" ghcr.io/mosabbir-maruf/unarc:latest test archive.zip
  ```

- **Extract Archive to Output Directory**:
  ```bash
  docker run --rm -v "$PWD:/work" ghcr.io/mosabbir-maruf/unarc:latest extract archive.zip --output out
  ```

- **Run System and Engine Diagnostics**:
  ```bash
  docker run --rm ghcr.io/mosabbir-maruf/unarc:latest doctor
  ```

- **Check Version**:
  ```bash
  docker run --rm ghcr.io/mosabbir-maruf/unarc:latest version
  ```

---

## Docker Security

Unarc supports two distinct container execution modes depending on operational requirements:

### 1. Convenience Mode (Default)
```bash
docker run -it --rm -v "$PWD:/work" ghcr.io/mosabbir-maruf/unarc:latest
```
- **Execution User**: Runs strictly as unprivileged non-root user `65532:65532` (`nonroot`).
- **Minimal Image Base**: Distroless base (`gcr.io/distroless/cc-debian12:nonroot`) contains zero shells (`/bin/sh` does not exist), zero package managers (`apt`, `dpkg` absent), zero compilers, and zero interpreters.
- **Internal Confinement**: The Unarc application code automatically applies `prctl(PR_SET_NO_NEW_PRIVS)` and `prctl(PR_SET_PDEATHSIG, SIGKILL)` before spawning the engine, wipes ambient environment variables, and enforces strict path traversal limits.
- **Filesystem Mount**: Mounts `$PWD` at `/work` with read-write permissions for the current host working directory.

### 2. Maximum-Security Mode (Hardened CI / Untrusted Archives)
For automated CI/CD pipelines, untrusted multi-tenant archives, or high-assurance environments:
```bash
docker run --rm --network none --read-only --cap-drop ALL \
  -v "$PWD/archive.zip:/input/archive.zip:ro" \
  -v "$PWD/output:/output:rw" \
  ghcr.io/mosabbir-maruf/unarc:latest \
  extract /input/archive.zip --output /output
```

### Why These Flags Provide Additional Defense in Depth:
- `--network none`: Disables the container network namespace entirely at the kernel level, eliminating network egress or socket binding capabilities.
- `--read-only`: Mounts the container root filesystem as strictly read-only, preventing writes anywhere outside declared volumes (`/tmp` scratch volume remains isolated).
- `--cap-drop ALL`: Strips all Linux kernel capabilities from the container process tree.
- Explicit `:ro` and `:rw` volume boundaries: Mounts the archive input file as strictly read-only (`:ro`) and confines filesystem write access exclusively to the target output directory (`:rw`), preventing accidental modification of adjacent host files.

> [!NOTE]
> The convenience mode and the maximum-security mode do **not** have identical Docker-level isolation. While the image is inherently non-root and distroless, the explicit flags `--network none`, `--read-only`, and `--cap-drop ALL` enforce kernel-level constraints that the container runtime does not activate automatically without those flags.

---

## Password-Protected Archives

Unarc implements secure, ephemeral password handling:

- **Interactive Masked Prompt**: When an archive is encrypted, Unarc prompts on the terminal using masked input without echo (`rpassword`). The password is never displayed on screen.
- **No CLI Argument**: Unarc provides **no** `--password` or `-p` flag. This prevents archive passwords from appearing in process listings (`ps`, `/proc`), shell history files, or CI job logs.
- **No Environment Variable**: Unarc does **not** read passwords from environment variables (e.g. `UNARC_PASSWORD`), avoiding credential leakage across process inheritance trees.
- **No Persistence**: Passwords are held strictly in ephemeral process memory for the extraction operation and are wiped immediately upon completion; they are never written to disk or configuration caches.
- **Non-Interactive Behavior**: If a password-protected archive is encountered in a non-interactive environment (stdin closed or not attached to a TTY), Unarc immediately fails closed with exit code `16` (`PASSWORD_REQUIRED`). If an incorrect password is entered, Unarc fails with exit code `17` (`INVALID_PASSWORD`).

---

## CLI

### Subcommands

| Subcommand | Description | Example |
|---|---|---|
| `extract <ARCHIVE>` | Securely extracts archive with path traversal and containment verification. Supports `--output <DIR>`. | `unarc extract archive.zip --output ./out` |
| `test <ARCHIVE>` | Tests archive integrity without writing extracted files to disk. | `unarc test archive.7z` |
| `info` | Displays platform architecture, bundled engine version, and active security policy. Supports `--json`. | `unarc info` |
| `doctor` | Runs end-to-end diagnostics, engine integrity checks, and sandbox boundary probes. Supports `--json`. | `unarc doctor` |
| `update` | Checks for or applies cryptographically signed self-updates (native binaries only). Supports `--check`. | `unarc update --check` |
| `version` | Displays the current Unarc version string. | `unarc version` |

### Global Flags
- `--json`: Formats command output as structured JSON.
- `-v, --verbose`: Increases logging verbosity.
- `-q, --quiet`: Suppresses non-essential terminal output.

### Interactive Mode

Running `unarc` without arguments enters the interactive shell:

```
 _   _ _   _   _    ____   ____ 
| | | | \ | | / \  |  _ \ / ___|
| | | |  \| |/ _ \ | |_) | |    
| |_| | |\  / ___ \|  _ <| |___ 
 \___/|_| \_/_/   \_\_| \_\\____|
     Secure Archive Utility

Type '/' to view commands, or '/help' for usage guidance.
unarc › 
```

#### Interactive Commands
- `/extract`: Prompts for archive path and destination, then extracts securely.
- `/test`: Prompts for archive path and tests integrity.
- `/info`: Displays platform, engine, and security details.
- `/doctor`: Runs diagnostic health checks.
- `/update`: Checks for and applies cryptographically verified self-update.
- `/config`: Displays active security policy.
- `/help`: Displays interactive command reference.
- `/exit`: Exits the interactive session.

#### Interactive Navigation
- Type `/` to open the command palette.
- Prefix filtering narrows suggestions (e.g., typing `/ex` filters to `/extract` and `/exit`).
- **Up/Down Arrow keys** navigate the suggestion list.
- **Tab** autocompletes the selected command.
- **Enter** executes.

---

## Supported Platforms / Formats

### Supported Platforms
- **macOS Apple Silicon**: `macos-arm64` (`aarch64-apple-darwin`)
- **Linux x86_64**: `linux-x86_64` (`x86_64-unknown-linux-gnu`)
- **Linux ARM64**: `linux-aarch64` (`aarch64-unknown-linux-gnu`)
- **Docker**: Multi-architecture image (`linux/amd64`, `linux/arm64`)

### Supported Formats
- **ZIP** (`.zip`): Standard ZIP archives (Deflate, BZIP2, LZMA, AES encryption).
- **7-Zip** (`.7z`): 7-Zip archives (LZMA, LZMA2, PPMd, BCJ, AES-256).
- **TAR** (`.tar`): POSIX tar archives.
- **Gzip TAR** (`.tar.gz`, `.tgz`): Gzip-compressed tar archives.
- **Bzip2 TAR** (`.tar.bz2`, `.tbz2`): Bzip2-compressed tar archives.
- **XZ TAR** (`.tar.xz`, `.txz`): XZ-compressed tar archives.
- **RAR** (`.rar`): RAR 4.x legacy and RAR 5.x archives, including multipart volume sequences (`.part1.rar`, `.r00`).

---

## Security

Unarc is architected with a zero-trust model for archive processing:

- **Path Traversal Protection (`Zip Slip`)**: Strict rejection of directory traversal sequences (`../`), absolute paths (`/etc/shadow`), null bytes, or paths escaping the root boundary.
- **Unsafe Entry Rejection**: Blocks extraction of symlinks, hardlinks, device nodes, FIFOs, and sockets by default.
- **Output Boundary Containment**: Canonical destination checking ensures every extracted entry resolves strictly within the target destination root before disk writes occur.
- **Hermetic Pinned Engine**: Bundles an exact official release of 7-Zip (`7zz v26.03`), verified by SHA-256 checksum during packaging and verified at runtime by `unarc doctor`. Unarc never invokes system PATH binaries, never calls Homebrew, and never downloads binaries at runtime.
- **Native OS Sandboxing**:
  - *macOS*: Uses Apple Seatbelt (`sandbox-exec`) kernel confinement to deny network access and restrict filesystem operations strictly to resolved volumes and destination directories.
  - *Linux*: Employs process group isolation, environment purging, `PR_SET_NO_NEW_PRIVS`, and `PR_SET_PDEATHSIG` (terminates child processes if parent dies).
- **Docker Isolation**: Non-root UID/GID `65532:65532`, distroless base with no shells or compilers, and full support for `--network none`, `--read-only`, and `--cap-drop ALL`.
- **Zero Telemetry / Offline Operation**: Completely offline; zero network calls during extraction; zero background services, telemetry, or uncoordinated logging.

---

## Developer / Build From Source

> [!NOTE]
> This section is strictly for contributors and developers modifying Unarc source code or building locally. Normal native end users should use the [Native Installation](#native-installation-recommended) prebuilt binaries or the [Docker](#docker-ghcr) container image above.

Building Unarc locally from source requires the Rust toolchain (`rustc` and `cargo`), because it compiles the binary and all dependencies directly on your machine.

### Prerequisites
- **Rust Toolchain**: Rust 1.85.0+ installed (via [rustup.rs](https://rustup.rs/)).
- **Optional**: Docker (for running isolated verification scripts and tests hermetically via `make`).

### Building and Running from Source

To clone the source repository, compile the project, and run Unarc:

```bash
# 1. Clone the repository
git clone https://github.com/mosabbir-maruf/Unarc.git
cd Unarc

# 2. Compile and run locally
cargo run --release
```

> [!IMPORTANT]
> The `--release` flag passed to `cargo run` or `cargo build` is **Cargo's release build profile** (instructing `rustc` to compile with full compiler optimizations and strip debug assertions). It is a local compilation setting, not a prebuilt GitHub Release bundle.

### Additional Developer Workflow Commands

```bash
# Build release binary without immediately executing
cargo build --release

# Run debug build
cargo build

# Run with developer container (hermetic, pinned toolchain)
make build          # Debug build in container
make release        # Release build in container
make test           # Run unit & integration tests
make check          # Full verification pass (fmt, clippy, tests, release build)

# Build development and production Docker images
make docker-build
make docker-prod
```

---

## Verification / Development Checks

Developers can run the full project verification suite using the existing scripts:

```bash
# 1. Format check
./scripts/dev.sh fmt-check
# (or: cargo fmt --all -- --check)

# 2. Clippy lint check (warnings treated as errors)
./scripts/dev.sh clippy
# (or: cargo clippy --all-targets --all-features -- -D warnings)

# 3. Unit and integration tests
./scripts/dev.sh test
# (or: cargo test --all-targets)

# 4. Hardened Docker runtime verification pass (9/9 checks)
./scripts/verify-docker.sh unarc:latest

# 5. OS-level security boundary verification pass (7/7 checks)
./scripts/verify-security.sh

# 6. Containerized secret scanning (gitleaks)
CID=$(docker create zricethezav/gitleaks:latest detect --source /repo --verbose --no-git) && \
  docker cp . "$CID:/repo" && docker start -a "$CID" && docker rm "$CID"
```

---

## Troubleshooting

### 1. macOS Gatekeeper / Quarantine Alert
If macOS blocks execution of downloaded binaries with an alert stating the developer cannot be verified:
```bash
xattr -d com.apple.quarantine ~/.local/bin/unarc
```

### 2. Docker Volume Permissions
- Ensure Docker Desktop has permission to access your host directory (Docker Settings -> Resources -> File Sharing).
- If extraction fails with `Permission denied` (exit code `30`), ensure the mounted output directory is writable by unprivileged container user `65532:65532` (`chmod 777 ./out` or adjust permissions).

### 3. Password-Protected Archive in Non-Interactive Mode
If extracting an encrypted archive in automated scripts or non-TTY environments, Unarc fails closed with exit code `16` (`PASSWORD_REQUIRED`). Run interactively with a TTY (`docker run -it ...` or native terminal) to enter the masked password.

### 4. Missing or Corrupt Multipart Archive Volume
When extracting multipart archive sets (`.part1.rar` or `.r00`), ensure all constituent volume files are present in the same directory as the initial volume. Missing volumes trigger exit code `13` (`MISSING_VOLUME`); corrupt volumes trigger exit code `14` (`INVALID_VOLUME`).

### 5. Engine Resolution Priority
Unarc resolves its archive engine in strict priority:
1. `UNARC_BUNDLED_7ZZ` environment variable (if explicitly set for custom/testing setups).
2. An adjacent `7zz` executable located in the same directory (for backwards compatibility / dev setups).
3. Hardcoded container path `/opt/unarc/bin/7zz` (in Docker distroless runtime).
4. **Embedded Authentic Engine**: Unarc automatically materializes its embedded pinned 7zz engine into a private restricted temporary directory. No external engine files are required.

---

## Release / Integrity

Official Unarc releases include cryptographic integrity verification:

- **Release Artifacts**:
  - `unarc-<version>-<platform>`: Standalone self-contained native executable.
  - `unarc-<version>-<platform>.sha256`: SHA-256 digest file for the standalone executable.
  - `unarc-<version>-<platform>.manifest.json`: Cryptographically signed JSON release manifest.
  - `unarc-<version>-<platform>.tar.gz`: Full archive bundle (including documentation and licenses).
  - `unarc-<version>-<platform>.tar.gz.sha256`: SHA-256 digest file for the archive bundle.
- **Official Public Key (Ed25519 hex)**:
  `90cd97dbf43425cb694d386cb89f2e04fa252fafa6bffddddfc2f3fc962a94ee`
- **Native Self-Update**:
  Running `unarc update` connects to `https://github.com/mosabbir-maruf/Unarc/releases/latest/download`, downloads the release manifest, verifies the Ed25519 signature against the embedded trust anchor, verifies the SHA-256 checksum and executable format (Mach-O / ELF), stages adjacent in the target directory, and atomically replaces the binary via `rename(2)` with RAII rollback protection.

---

## Exit Codes

Unarc returns deterministic exit codes for scripting and automation:

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

## License & Third-Party Notices

Unarc is open-source software licensed under the **[MIT License](LICENSE)**.

- **Unarc Codebase**: Licensed under the MIT License (see [LICENSE](LICENSE)). Copyright (c) 2026 Mosabbir Maruf.
- **Bundled Engine (7-Zip / 7zz)**: Pinned 7-Zip (`7zz v26.03`) is developed by Igor Pavlov and is separately licensed under the **GNU LGPL v2.1+** (with the unRAR license restriction for RAR archive decompression and BSD/Public Domain portions for LZMA and 7z components). See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for details and full license terms.
- **Third-Party Dependencies**: All third-party Rust libraries retain their original permissive licenses (MIT, Apache-2.0, BSD-3-Clause). Complete notices and dependency attributions are available in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
