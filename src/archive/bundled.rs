//! Bundled 7-Zip (7zz) archive engine integration.

use super::backend::{ArchiveBackend, ArchiveExtractResult, ArchiveTestResult, ProgressListener};
use super::format::ArchiveFormat;
use super::metadata::{ArchiveEntry, ArchiveMetadata};
use crate::error::ArchiveError;
use crate::security::{ProcessSandboxPolicy, SandboxRunner, ScratchWorkspace};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Exact pinned 7-Zip version.
pub const PINNED_7ZIP_VERSION: &str = "26.03";

/// Exact official source release location.
pub const PINNED_7ZIP_RELEASE_URL: &str = "https://github.com/ip7z/7zip/releases/tag/26.03";

/// Expected SHA-256 for macOS archive (`7z2603-mac.tar.xz`).
pub const EXPECTED_SHA256_MACOS: &str =
    "5ca87677072c59f5602e5c49baa27d4694bacd2259b4e507f0094249d4281480";

/// Expected SHA-256 for Linux ARM64 archive (`7z2603-linux-arm64.tar.xz`).
pub const EXPECTED_SHA256_LINUX_ARM64: &str =
    "2389ba20e4d8295e8709c20b6263b69bd1ec4972fe38a04ad7a1badbf595b996";

/// Expected SHA-256 for Linux x86_64 archive (`7z2603-linux-x64.tar.xz`).
pub const EXPECTED_SHA256_LINUX_X64: &str =
    "dc99eff5008f1ab79bd7084c68513701547a808a89502bf4133683535ab3c695";

/// Formats supported by the bundled 7zz engine.
static SUPPORTED_FORMATS: &[ArchiveFormat] = &[
    ArchiveFormat::Zip,
    ArchiveFormat::SevenZip,
    ArchiveFormat::Tar,
    ArchiveFormat::TarGz,
    ArchiveFormat::TarBz2,
    ArchiveFormat::TarXz,
    ArchiveFormat::Rar,
    ArchiveFormat::Rar5,
];

/// Embedded pinned 7zz engine binary payload for the target platform.
pub const EMBEDDED_7ZZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded_7zz.bin"));

static MATERIALIZED_ENGINE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(unix)]
static ATEXIT_REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn atexit_cleanup() {
    clean_materialized_engine();
}

/// Checks whether an embedded 7zz engine payload is compiled into the binary.
#[must_use]
#[allow(clippy::const_is_empty)]
pub fn has_embedded_engine() -> bool {
    !EMBEDDED_7ZZ.is_empty()
}

/// Returns the embedded 7zz engine payload bytes.
#[must_use]
pub fn embedded_engine_bytes() -> &'static [u8] {
    EMBEDDED_7ZZ
}

/// Verifies that an arbitrary engine payload byte slice matches the expected pinned SHA-256 hash.
pub fn verify_engine_bytes_integrity(data: &[u8]) -> Result<(), crate::error::SecurityError> {
    if data.is_empty() {
        return Err(crate::error::SecurityError::PolicyViolation {
            reason: "Engine binary payload is empty".to_string(),
        });
    }

    let computed = crate::security::integrity::compute_sha256_bytes(data);
    let expected = crate::security::integrity::expected_engine_binary_sha256();

    if !computed.eq_ignore_ascii_case(expected) {
        return Err(crate::error::SecurityError::PolicyViolation {
            reason: format!(
                "Engine binary payload hash mismatch: computed {computed}, expected {expected}"
            ),
        });
    }

    Ok(())
}

/// Verifies that the compiled embedded 7zz engine payload matches the official pinned SHA-256 hash.
pub fn verify_embedded_engine_integrity() -> Result<(), crate::error::SecurityError> {
    if !has_embedded_engine() {
        return Err(crate::error::SecurityError::PolicyViolation {
            reason: "Embedded 7zz engine payload is not compiled into this binary".to_string(),
        });
    }
    verify_engine_bytes_integrity(EMBEDDED_7ZZ)
}

/// Cleans up any active materialized 7zz engine directory and binary.
pub fn clean_materialized_engine() {
    let mut guard = MATERIALIZED_ENGINE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(path) = guard.take() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}

/// Scans temporary directory and cleans up stale engine folders left by dead processes.
#[cfg(unix)]
fn clean_stale_engine_dirs() {
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if s.starts_with(".unarc_engine_") {
                let parts: Vec<&str> = s.split('_').collect();
                if parts.len() >= 3 {
                    if let Ok(pid) = parts[2].parse::<libc::pid_t>() {
                        let is_dead = unsafe {
                            libc::kill(pid, 0) == -1
                                && std::io::Error::last_os_error().raw_os_error()
                                    == Some(libc::ESRCH)
                        };
                        if is_dead {
                            let _ = std::fs::remove_dir_all(entry.path());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(unix))]
fn clean_stale_engine_dirs() {}

/// Materializes the embedded 7zz engine payload into a secure temporary location.
pub fn get_or_materialize_embedded_engine() -> Result<PathBuf, ArchiveError> {
    let mut guard = MATERIALIZED_ENGINE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(ref path) = *guard {
        if path.is_file() {
            return Ok(path.clone());
        }
    }

    // 1. Verify embedded payload integrity before writing anything to disk
    verify_embedded_engine_integrity().map_err(|e| ArchiveError::BackendFailure {
        backend: "bundled-7zz".to_string(),
        message: format!("Embedded engine integrity check failed: {e}"),
    })?;

    // 2. Clean up any stale engine directories from dead PIDs
    clean_stale_engine_dirs();

    // 3. Create private user-owned directory with restrictive permissions (0700 on Unix)
    let token = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let engine_dir =
        std::env::temp_dir().join(format!(".unarc_engine_{}_{}", std::process::id(), token));

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(false);
        builder.mode(0o700);
        builder
            .create(&engine_dir)
            .map_err(|e| ArchiveError::BackendFailure {
                backend: "bundled-7zz".to_string(),
                message: format!(
                    "Failed to create secure engine directory '{}': {e}",
                    engine_dir.display()
                ),
            })?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir(&engine_dir).map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!(
                "Failed to create secure engine directory '{}': {e}",
                engine_dir.display()
            ),
        })?;
    }

    let bin_path = engine_dir.join("7zz");

    // 4. Write binary payload
    std::fs::write(&bin_path, EMBEDDED_7ZZ).map_err(|e| {
        let _ = std::fs::remove_dir_all(&engine_dir);
        ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to write embedded engine binary: {e}"),
        }
    })?;

    // 5. Restrict permissions to read and execute only (0500 on Unix) to prevent post-creation modification
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o500))
        {
            let _ = std::fs::remove_dir_all(&engine_dir);
            return Err(ArchiveError::BackendFailure {
                backend: "bundled-7zz".to_string(),
                message: format!("Failed to set secure permissions on engine binary: {e}"),
            });
        }
    }

    // 6. Verify written file integrity
    let written_hash = crate::security::integrity::compute_sha256(&bin_path).map_err(|e| {
        let _ = std::fs::remove_dir_all(&engine_dir);
        ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to verify written engine hash: {e}"),
        }
    })?;
    let expected = crate::security::integrity::expected_engine_binary_sha256();
    if !written_hash.eq_ignore_ascii_case(expected) {
        let _ = std::fs::remove_dir_all(&engine_dir);
        return Err(ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Written engine binary hash mismatch: {written_hash} != {expected}"),
        });
    }

    // 7. Register atexit handler on Unix for automatic process exit cleanup
    #[cfg(unix)]
    {
        if !ATEXIT_REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            unsafe {
                libc::atexit(atexit_cleanup);
            }
        }
    }

    *guard = Some(bin_path.clone());
    Ok(bin_path)
}

/// Locates the bundled 7zz executable adhering strictly to hermetic bundling rules.
///
/// Priority order:
/// 1. `UNARC_BUNDLED_7ZZ` environment variable override (for tests/CI).
/// 2. Adjacent `7zz` binary on disk (`current_exe.parent().join("7zz")`).
/// 3. Known hermetic container bundle locations (`/opt/unarc/bin/7zz`).
/// 4. Materialized embedded 7zz engine (self-contained native distribution).
///
/// Security constraints:
/// - NEVER queries the system PATH.
/// - NEVER invokes Homebrew or arbitrary system-installed 7-Zip binaries.
/// - NEVER attempts to download the binary at runtime.
pub fn resolve_bundled_engine() -> Result<PathBuf, ArchiveError> {
    // 1. Explicit bundled engine environment override (used in Docker and CI)
    if let Ok(env_path) = std::env::var("UNARC_BUNDLED_7ZZ") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Ok(p);
        }
    }

    // 2. Relative to current binary
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let adjacent = dir.join("7zz");
            if adjacent.is_file() {
                return Ok(adjacent);
            }
            let bundled_dir = dir.join("bundled").join("7zz");
            if bundled_dir.is_file() {
                return Ok(bundled_dir);
            }
        }
    }

    // 3. Known hermetic container bundle locations
    let known_locations = [
        "/opt/unarc/bin/7zz",
        "/usr/local/lib/unarc/bin/7zz",
        "/usr/local/lib/unarc/7zz",
    ];

    for loc in known_locations {
        let p = Path::new(loc);
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
    }

    // 4. Materialize embedded engine if available (single-file self-contained distribution)
    if has_embedded_engine() {
        return get_or_materialize_embedded_engine();
    }

    Err(ArchiveError::BackendFailure {
        backend: "bundled-7zz".to_string(),
        message: format!(
            "Bundled 7zz engine (v{}) not found. Unarc must only run with its pinned bundled engine.",
            PINNED_7ZIP_VERSION
        ),
    })
}

/// Archive engine backend backed by the bundled 7zz binary.
#[derive(Debug, Default, Clone)]
pub struct SevenZipBackend;

/// Stateful parser for 7-Zip progress streams (`-bsp2`).
#[derive(Debug, Default)]
pub struct SevenZipProgressParser {
    buf: Vec<u8>,
}

impl SevenZipProgressParser {
    /// Creates a new progress parser instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(256),
        }
    }

    /// Feeds a chunk from the 7zz stderr stream and invokes callback for each detected percentage update.
    pub fn feed<F>(&mut self, chunk: &[u8], mut callback: F)
    where
        F: FnMut(u8, Option<&str>),
    {
        self.buf.extend_from_slice(chunk);
        let mut i = 0;
        let mut last_matched_end = 0;

        while let Some(rel_pct) = self.buf[i..].iter().position(|&b| b == b'%') {
            let pct_idx = i + rel_pct;

            // Search backwards for 1-3 digits
            let mut start = pct_idx;
            while start > 0 && self.buf[start - 1].is_ascii_digit() {
                start -= 1;
            }

            if start < pct_idx && (pct_idx - start) <= 3 {
                if let Ok(pct_str) = std::str::from_utf8(&self.buf[start..pct_idx]) {
                    if let Ok(pct) = pct_str.parse::<u8>() {
                        if pct <= 100 {
                            // Search forwards for filename up to \x08, \r, \n, or null
                            let mut fwd = pct_idx + 1;
                            while fwd < self.buf.len() && self.buf[fwd] == b' ' {
                                fwd += 1;
                            }

                            // Skip action code or item counter if present (e.g. "- ", "T ", "10 ")
                            if fwd < self.buf.len() {
                                let mut code_end = fwd;
                                while code_end < self.buf.len()
                                    && (self.buf[code_end] == b'-'
                                        || self.buf[code_end] == b'+'
                                        || self.buf[code_end] == b'T'
                                        || self.buf[code_end] == b'U'
                                        || self.buf[code_end] == b'R'
                                        || self.buf[code_end].is_ascii_digit())
                                {
                                    code_end += 1;
                                }
                                if code_end > fwd
                                    && code_end < self.buf.len()
                                    && self.buf[code_end] == b' '
                                {
                                    fwd = code_end + 1;
                                    while fwd < self.buf.len() && self.buf[fwd] == b' ' {
                                        fwd += 1;
                                    }
                                }
                            }

                            let name_start = fwd;
                            while fwd < self.buf.len()
                                && self.buf[fwd] != 8
                                && self.buf[fwd] != b'\r'
                                && self.buf[fwd] != b'\n'
                                && self.buf[fwd] != 0
                            {
                                fwd += 1;
                            }

                            let current_file = if fwd > name_start {
                                let raw = String::from_utf8_lossy(&self.buf[name_start..fwd]);
                                let trimmed = raw.trim();
                                if !trimmed.is_empty() {
                                    Some(trimmed.to_string())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            callback(pct, current_file.as_deref());
                            i = fwd;
                            last_matched_end = fwd;
                            continue;
                        }
                    }
                }
            }

            i = pct_idx + 1;
        }

        if last_matched_end > 0 {
            self.buf.drain(..last_matched_end);
        } else if self.buf.len() > 128 {
            let keep_from = self.buf.len() - 128;
            self.buf.drain(..keep_from);
        }
    }
}

impl SevenZipBackend {
    /// Creates a new bundled 7zz backend instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Invokes the bundled 7zz binary confined within the specified sandbox policy.
    pub fn execute_with_policy<I, S>(
        &self,
        args: I,
        policy: &ProcessSandboxPolicy,
    ) -> Result<Output, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        SandboxRunner::execute(policy, args)
    }

    /// Invokes the bundled 7zz binary with an isolated scratch sandbox.
    fn execute_7zz<I, S>(&self, args: I, input_path: Option<&Path>) -> Result<Output, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let engine_path = resolve_bundled_engine()?;
        let scratch = ScratchWorkspace::new().map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to create isolated scratch workspace: {e}"),
        })?;

        let mut policy = ProcessSandboxPolicy::new(engine_path, scratch.path().to_path_buf());
        if let Some(p) = input_path {
            policy = policy.with_input(p.to_path_buf());
        }

        SandboxRunner::execute(&policy, args)
    }

    /// Tests the integrity of an archive confined by the given sandbox policy.
    pub fn test_with_policy(
        &self,
        path: &Path,
        password: Option<&str>,
        policy: &ProcessSandboxPolicy,
    ) -> Result<ArchiveTestResult, ArchiveError> {
        self.test_with_policy_and_progress(path, password, policy, None)
    }

    /// Tests the integrity of an archive confined by the given sandbox policy, with optional progress streaming.
    pub fn test_with_policy_and_progress(
        &self,
        path: &Path,
        password: Option<&str>,
        policy: &ProcessSandboxPolicy,
        mut progress: Option<&mut dyn ProgressListener>,
    ) -> Result<ArchiveTestResult, ArchiveError> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        let format = ArchiveFormat::from_path(path).unwrap_or(ArchiveFormat::Zip);

        let mut args: Vec<String> = vec![
            "t".to_string(),
            "-y".to_string(),
            "-bso1".to_string(),
            "-bse2".to_string(),
        ];

        if progress.is_some() {
            args.push("-bsp2".to_string());
        }

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string());
        }

        args.push(path.to_string_lossy().to_string());

        let output = if let Some(ref mut cb) = progress {
            let mut parser = SevenZipProgressParser::new();
            SandboxRunner::execute_with_progress(
                policy,
                &args,
                Some(move |chunk: &[u8]| {
                    parser.feed(chunk, |pct, file| cb.on_progress(pct, file));
                }),
            )?
        } else {
            self.execute_with_policy(&args, policy)?
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        if crate::platform::signals::is_interrupted() {
            return Err(ArchiveError::ExtractionFailed {
                message: "Integrity check interrupted by signal".to_string(),
            });
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = output.status.signal() {
                if sig == libc::SIGINT || sig == libc::SIGTERM || sig == libc::SIGKILL {
                    crate::platform::signals::set_interrupted(true);
                    return Err(ArchiveError::ExtractionFailed {
                        message: "Process terminated by signal".to_string(),
                    });
                }
            }
        }

        if combined.contains("Permission denied") || combined.contains("Access is denied") {
            return Err(ArchiveError::PermissionDenied {
                path: path.to_string_lossy().to_string(),
            });
        }

        if combined.contains("Cannot find volume")
            || combined.contains("Can not find volume")
            || combined.contains("Missing volume")
            || combined.contains("Cannot open volume")
        {
            return Err(ArchiveError::MissingVolume {
                expected: "next volume".to_string(),
                details: "Engine reported missing volume during integrity check".to_string(),
            });
        }

        if combined.contains("Wrong password")
            || combined.contains("Can not open encrypted archive")
            || combined.contains("Data Error in encrypted file")
        {
            return if password.is_some() {
                Err(ArchiveError::InvalidPassword {
                    path: path.to_string_lossy().to_string(),
                })
            } else {
                Err(ArchiveError::PasswordRequired {
                    path: path.to_string_lossy().to_string(),
                })
            };
        }

        if !output.status.success() {
            if combined.contains("Headers Error")
                || combined.contains("Data Error")
                || combined.contains("Cannot open the file as archive")
            {
                return Err(ArchiveError::CorruptArchive {
                    message: "Archive headers or data CRC check failed".to_string(),
                });
            }
            return Err(ArchiveError::BackendFailure {
                backend: "bundled-7zz".to_string(),
                message: format!("Integrity check returned failure code: {:?}", output.status),
            });
        }

        Ok(ArchiveTestResult {
            path: path.to_path_buf(),
            format,
            passed: true,
            entries_checked: None,
            message: "Everything is Ok".to_string(),
        })
    }

    /// Extracts an archive into the destination directory confined by the given sandbox policy.
    pub fn extract_with_policy(
        &self,
        path: &Path,
        destination: &Path,
        password: Option<&str>,
        policy: &ProcessSandboxPolicy,
    ) -> Result<ArchiveExtractResult, ArchiveError> {
        self.extract_with_policy_and_progress(path, destination, password, policy, None)
    }

    /// Extracts an archive into the destination directory confined by the given sandbox policy, with optional progress streaming.
    pub fn extract_with_policy_and_progress(
        &self,
        path: &Path,
        destination: &Path,
        password: Option<&str>,
        policy: &ProcessSandboxPolicy,
        mut progress: Option<&mut dyn ProgressListener>,
    ) -> Result<ArchiveExtractResult, ArchiveError> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        // Create destination directory if it does not already exist
        std::fs::create_dir_all(destination).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => ArchiveError::PermissionDenied {
                path: destination.display().to_string(),
            },
            _ => ArchiveError::OutputInvalid {
                path: destination.display().to_string(),
                reason: e.to_string(),
            },
        })?;

        let format = ArchiveFormat::from_path(path).unwrap_or(ArchiveFormat::Zip);

        let mut args: Vec<String> = vec![
            "x".to_string(),
            format!("-o{}", destination.display()),
            "-y".to_string(),
            "-snl-".to_string(), // Disable symbolic link extraction for security
            "-snh-".to_string(), // Disable hard link extraction for security
            "-bso1".to_string(),
            "-bse2".to_string(),
        ];

        if progress.is_some() {
            args.push("-bsp2".to_string());
        }

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string());
        }

        args.push(path.to_string_lossy().to_string());

        let output = if let Some(ref mut cb) = progress {
            let mut parser = SevenZipProgressParser::new();
            SandboxRunner::execute_with_progress(
                policy,
                &args,
                Some(move |chunk: &[u8]| {
                    parser.feed(chunk, |pct, file| cb.on_progress(pct, file));
                }),
            )?
        } else {
            self.execute_with_policy(&args, policy)?
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

        if crate::platform::signals::is_interrupted() {
            return Err(ArchiveError::ExtractionFailed {
                message: "Extraction interrupted by signal".to_string(),
            });
        }

        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = output.status.signal() {
                if sig == libc::SIGINT || sig == libc::SIGTERM || sig == libc::SIGKILL {
                    crate::platform::signals::set_interrupted(true);
                    return Err(ArchiveError::ExtractionFailed {
                        message: "Process terminated by signal".to_string(),
                    });
                }
            }
        }

        if combined.contains("Permission denied") || combined.contains("Access is denied") {
            return Err(ArchiveError::PermissionDenied {
                path: destination.display().to_string(),
            });
        }

        if combined.contains("Cannot find volume")
            || combined.contains("Can not find volume")
            || combined.contains("Missing volume")
            || combined.contains("Cannot open volume")
        {
            return Err(ArchiveError::MissingVolume {
                expected: "next volume".to_string(),
                details: "Engine reported missing volume during extraction".to_string(),
            });
        }

        if combined.contains("Wrong password")
            || combined.contains("Can not open encrypted archive")
            || combined.contains("Data Error in encrypted file")
        {
            return if password.is_some() {
                Err(ArchiveError::InvalidPassword {
                    path: path.to_string_lossy().to_string(),
                })
            } else {
                Err(ArchiveError::PasswordRequired {
                    path: path.to_string_lossy().to_string(),
                })
            };
        }

        if !output.status.success() {
            if combined.contains("Headers Error")
                || combined.contains("Data Error")
                || combined.contains("Cannot open the file as archive")
            {
                return Err(ArchiveError::CorruptArchive {
                    message: "Archive contains corrupted files or headers".to_string(),
                });
            }
            if combined.contains("No space left on device") || combined.contains("Disk full") {
                return Err(ArchiveError::ExtractionFailed {
                    message: "Insufficient disk space to extract archive".to_string(),
                });
            }
            return Err(ArchiveError::ExtractionFailed {
                message: format!("Extraction failed with exit code: {:?}", output.status),
            });
        }

        Ok(ArchiveExtractResult {
            archive_path: path.to_path_buf(),
            destination: destination.to_path_buf(),
            format,
            entries_extracted: None,
            total_bytes_extracted: None,
        })
    }
}

impl ArchiveBackend for SevenZipBackend {
    fn name(&self) -> &'static str {
        "bundled-7zz"
    }

    fn supported_formats(&self) -> &'static [ArchiveFormat] {
        SUPPORTED_FORMATS
    }

    fn is_available(&self) -> bool {
        resolve_bundled_engine().is_ok()
    }

    fn inspect(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<ArchiveMetadata, ArchiveError> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or_default();

        let format = ArchiveFormat::from_path(path).unwrap_or(ArchiveFormat::Zip);

        let mut args: Vec<String> = vec![
            "l".to_string(),
            "-slt".to_string(),
            "-bso1".to_string(),
            "-bse2".to_string(),
        ];

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string()); // empty password to probe header encryption
        }

        args.push(path.to_string_lossy().to_string());

        let output = self.execute_7zz(&args, Some(path))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let is_encrypted = stdout.contains("Encrypted = +")
            || stdout.contains("Method = 7zAES")
            || stdout.contains("Method = AES")
            || stderr.contains("Wrong password")
            || stderr.contains("Enter password");

        let is_solid = stdout.contains("Solid = +");

        let mut entries_count = 0usize;
        for line in stdout.lines() {
            if line.starts_with("Path = ") && !line.ends_with(path.to_string_lossy().as_ref()) {
                entries_count += 1;
            }
        }

        let mut metadata = ArchiveMetadata::new(format, file_size);
        metadata.is_encrypted = is_encrypted;
        metadata.is_solid = is_solid;
        if entries_count > 0 {
            metadata.entries_count = Some(entries_count);
        }

        Ok(metadata)
    }

    fn list_entries(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Vec<ArchiveEntry>, ArchiveError> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        let mut args: Vec<String> = vec![
            "l".to_string(),
            "-slt".to_string(),
            "-bso1".to_string(),
            "-bse2".to_string(),
        ];

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string());
        }

        args.push(path.to_string_lossy().to_string());

        let output = self.execute_7zz(&args, Some(path))?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut entries = Vec::new();
        let mut cur_path: Option<PathBuf> = None;
        let mut cur_size = 0u64;
        let mut cur_packed: Option<u64> = None;
        let mut cur_is_dir = false;
        let mut cur_is_symlink = false;
        let mut in_entries_section = false;

        for line in stdout.lines() {
            if line.starts_with("----------") || line.starts_with("---") {
                in_entries_section = true;
                continue;
            }

            if !in_entries_section {
                continue;
            }

            if let Some(stripped) = line.strip_prefix("Path = ") {
                let p = PathBuf::from(stripped.trim());
                if let Some(last_p) = cur_path.take() {
                    if last_p != path {
                        entries.push(ArchiveEntry {
                            path: last_p,
                            uncompressed_size: cur_size,
                            compressed_size: cur_packed,
                            is_directory: cur_is_dir,
                            is_symlink: cur_is_symlink,
                            symlink_target: None,
                        });
                    }
                    cur_size = 0;
                    cur_packed = None;
                    cur_is_dir = false;
                    cur_is_symlink = false;
                }
                cur_path = Some(p);
            } else if let Some(stripped) = line.strip_prefix("Size = ") {
                cur_size = stripped.trim().parse::<u64>().unwrap_or(0);
            } else if let Some(stripped) = line.strip_prefix("Packed Size = ") {
                cur_packed = stripped.trim().parse::<u64>().ok();
            } else if let Some(stripped) = line.strip_prefix("Folder = ") {
                cur_is_dir = stripped.trim() == "+";
            } else if let Some(stripped) = line.strip_prefix("Symbolic Link = ") {
                cur_is_symlink = stripped.trim() == "+" || !stripped.trim().is_empty();
            } else if let Some(stripped) = line.strip_prefix("Attributes = ") {
                let attr = stripped.trim();
                if attr.starts_with('l') || attr.contains('l') {
                    cur_is_symlink = true;
                }
            } else if let Some(stripped) = line.strip_prefix("Characteristics = ") {
                if stripped.contains("Symbolic Link") || stripped.contains("SymLink") {
                    cur_is_symlink = true;
                }
            }
        }

        if let Some(last_p) = cur_path {
            if last_p != path {
                entries.push(ArchiveEntry {
                    path: last_p,
                    uncompressed_size: cur_size,
                    compressed_size: cur_packed,
                    is_directory: cur_is_dir,
                    is_symlink: cur_is_symlink,
                    symlink_target: None,
                });
            }
        }

        Ok(entries)
    }

    fn test(&self, path: &Path, password: Option<&str>) -> Result<ArchiveTestResult, ArchiveError> {
        let engine_path = resolve_bundled_engine()?;
        let scratch = ScratchWorkspace::new().map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to create isolated scratch workspace: {e}"),
        })?;
        let policy = ProcessSandboxPolicy::new(engine_path, scratch.path().to_path_buf())
            .with_input(path.to_path_buf());
        self.test_with_policy(path, password, &policy)
    }

    fn extract(
        &self,
        path: &Path,
        destination: &Path,
        password: Option<&str>,
    ) -> Result<ArchiveExtractResult, ArchiveError> {
        let engine_path = resolve_bundled_engine()?;
        let scratch = ScratchWorkspace::new().map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to create isolated scratch workspace: {e}"),
        })?;
        let policy = ProcessSandboxPolicy::new(engine_path, scratch.path().to_path_buf())
            .with_input(path.to_path_buf())
            .with_destination(destination.to_path_buf());
        self.extract_with_policy(path, destination, password, &policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pinned_version_constants() {
        assert_eq!(PINNED_7ZIP_VERSION, "26.03");
        assert_eq!(EXPECTED_SHA256_MACOS.len(), 64);
        assert_eq!(EXPECTED_SHA256_LINUX_ARM64.len(), 64);
        assert_eq!(EXPECTED_SHA256_LINUX_X64.len(), 64);
    }

    #[test]
    fn test_resolve_bundled_engine_env_var() {
        let temp_dir = std::env::temp_dir();
        let fake_7zz = temp_dir.join("fake_7zz");
        let _ = std::fs::write(&fake_7zz, b"binary");

        std::env::set_var("UNARC_BUNDLED_7ZZ", fake_7zz.to_string_lossy().as_ref());
        let res = resolve_bundled_engine();
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), fake_7zz);

        std::env::remove_var("UNARC_BUNDLED_7ZZ");
        let _ = std::fs::remove_file(fake_7zz);
    }

    #[test]
    fn test_seven_zip_progress_parser() {
        let mut parser = SevenZipProgressParser::new();
        let mut updates = Vec::new();

        // Feed split chunks simulating real 7zz progress stream
        parser.feed(b"  0M Scan /tmp/\x08\x08  4", |pct, f| {
            updates.push((pct, f.map(String::from)));
        });
        assert!(updates.is_empty());

        parser.feed(b"5% - eboot.bin\x08\x08 5", |pct, f| {
            updates.push((pct, f.map(String::from)));
        });
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0], (45, Some("eboot.bin".to_string())));

        parser.feed(b"0% T test_file.pkg\x08\x08 100%\x08\x08", |pct, f| {
            updates.push((pct, f.map(String::from)));
        });
        assert_eq!(updates.len(), 3);
        assert_eq!(updates[1], (50, Some("test_file.pkg".to_string())));
        assert_eq!(updates[2], (100, None));
    }

    #[test]
    fn test_embedded_engine_tamper_rejection() {
        let tampered = b"this is completely invalid engine payload";
        let res = verify_engine_bytes_integrity(tampered);
        assert!(res.is_err());
        assert!(matches!(
            res,
            Err(crate::error::SecurityError::PolicyViolation { .. })
        ));
    }

    #[test]
    fn test_embedded_engine_empty_payload_rejection() {
        let empty = b"";
        let res = verify_engine_bytes_integrity(empty);
        assert!(res.is_err());
    }

    #[test]
    fn test_clean_materialized_engine() {
        clean_materialized_engine();
        let guard = MATERIALIZED_ENGINE.lock().unwrap();
        assert!(guard.is_none());
    }
}
