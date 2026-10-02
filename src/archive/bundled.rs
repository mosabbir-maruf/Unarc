//! Bundled 7-Zip (7zz) archive engine integration.

use super::backend::{ArchiveBackend, ArchiveExtractResult, ArchiveTestResult};
use super::format::ArchiveFormat;
use super::metadata::{ArchiveEntry, ArchiveMetadata};
use crate::error::ArchiveError;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
];

/// Locates the bundled 7zz executable adhering strictly to hermetic bundling rules.
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

impl SevenZipBackend {
    /// Creates a new bundled 7zz backend instance.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Invokes the bundled 7zz binary with the provided arguments.
    fn execute_7zz<I, S>(&self, args: I) -> Result<Output, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let engine_path = resolve_bundled_engine()?;

        let mut cmd = Command::new(engine_path);
        cmd.args(args);

        cmd.output().map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to spawn 7zz process: {e}"),
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

        let output = self.execute_7zz(&args)?;
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

        let output = self.execute_7zz(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut entries = Vec::new();
        let mut cur_path: Option<PathBuf> = None;
        let mut cur_size = 0u64;
        let mut cur_packed: Option<u64> = None;
        let mut cur_is_dir = false;

        for line in stdout.lines() {
            if let Some(stripped) = line.strip_prefix("Path = ") {
                let p = PathBuf::from(stripped.trim());
                if cur_path.is_some() && p != path {
                    if let Some(last_p) = cur_path.take() {
                        entries.push(ArchiveEntry {
                            path: last_p,
                            uncompressed_size: cur_size,
                            compressed_size: cur_packed,
                            is_directory: cur_is_dir,
                            is_symlink: false,
                            symlink_target: None,
                        });
                    }
                    cur_size = 0;
                    cur_packed = None;
                    cur_is_dir = false;
                }
                cur_path = Some(p);
            } else if let Some(stripped) = line.strip_prefix("Size = ") {
                cur_size = stripped.trim().parse::<u64>().unwrap_or(0);
            } else if let Some(stripped) = line.strip_prefix("Packed Size = ") {
                cur_packed = stripped.trim().parse::<u64>().ok();
            } else if let Some(stripped) = line.strip_prefix("Folder = ") {
                cur_is_dir = stripped.trim() == "+";
            }
        }

        if let Some(last_p) = cur_path {
            if last_p != path {
                entries.push(ArchiveEntry {
                    path: last_p,
                    uncompressed_size: cur_size,
                    compressed_size: cur_packed,
                    is_directory: cur_is_dir,
                    is_symlink: false,
                    symlink_target: None,
                });
            }
        }

        Ok(entries)
    }

    fn test(&self, path: &Path, password: Option<&str>) -> Result<ArchiveTestResult, ArchiveError> {
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

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string());
        }

        args.push(path.to_string_lossy().to_string());

        let output = self.execute_7zz(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

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
            if combined.contains("Headers Error") || combined.contains("Data Error") {
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

    fn extract(
        &self,
        path: &Path,
        destination: &Path,
        password: Option<&str>,
    ) -> Result<ArchiveExtractResult, ArchiveError> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            });
        }

        // Create destination directory if it does not already exist
        std::fs::create_dir_all(destination).map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to create destination directory: {e}"),
        })?;

        let format = ArchiveFormat::from_path(path).unwrap_or(ArchiveFormat::Zip);

        let mut args: Vec<String> = vec![
            "x".to_string(),
            format!("-o{}", destination.display()),
            "-y".to_string(),
            "-bso1".to_string(),
            "-bse2".to_string(),
        ];

        if let Some(pwd) = password {
            args.push(format!("-p{pwd}"));
        } else {
            args.push("-p".to_string());
        }

        args.push(path.to_string_lossy().to_string());

        let output = self.execute_7zz(&args)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");

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
            if combined.contains("Headers Error") || combined.contains("Data Error") {
                return Err(ArchiveError::CorruptArchive {
                    message: "Archive contains corrupted files or headers".to_string(),
                });
            }
            return Err(ArchiveError::BackendFailure {
                backend: "bundled-7zz".to_string(),
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
}
