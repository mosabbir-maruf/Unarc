//! Core application coordinator for Unarc operations.

use crate::archive::format::ArchiveFormat;
use crate::archive::metadata::ArchiveMetadata;
use crate::error::{ArchiveError, Result};
use crate::platform::PlatformInfo;
use crate::security::{SecurityContext, SecurityPolicy};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// System status and build metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    /// Binary package version.
    pub version: &'static str,

    /// Platform environment.
    pub platform: PlatformInfo,

    /// Active security policy configuration.
    pub policy: SecurityPolicy,
}

/// Core application instance orchestrating security and archive workflows.
#[derive(Debug, Clone)]
pub struct Application {
    security_context: SecurityContext,
    platform_info: PlatformInfo,
}

impl Default for Application {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Application {
    /// Creates a new application instance with the provided or default security policy.
    #[must_use]
    pub fn new(policy: Option<SecurityPolicy>) -> Self {
        let active_policy = policy.unwrap_or_else(SecurityPolicy::strict);
        Self {
            security_context: SecurityContext::new(active_policy),
            platform_info: PlatformInfo::current(),
        }
    }

    /// Returns the target platform runtime information.
    #[must_use]
    pub fn platform(&self) -> &PlatformInfo {
        &self.platform_info
    }

    /// Returns the active security policy.
    #[must_use]
    pub fn policy(&self) -> &SecurityPolicy {
        self.security_context.policy()
    }

    /// Inspects an archive on the filesystem, detecting format and extracting file metadata.
    pub fn inspect_archive(&self, path: &Path) -> Result<ArchiveMetadata> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            }
            .into());
        }

        let file_meta = std::fs::metadata(path)?;
        let file_size = file_meta.len();

        // Read initial header bytes for magic number identification
        let mut header = [0u8; 512];
        let bytes_read = {
            let mut file = File::open(path)?;
            file.read(&mut header)?
        };

        let format = ArchiveFormat::from_magic_bytes(&header[..bytes_read])
            .or_else(|| ArchiveFormat::from_extension(path))
            .ok_or_else(|| ArchiveError::UnsupportedFormat {
                format: path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            })?;

        let metadata = ArchiveMetadata::new(format, file_size);
        Ok(metadata)
    }

    /// Validates an entry path according to active security policies.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        self.security_context
            .policy()
            .validate_entry_path(path)
            .map_err(Into::into)
    }

    /// Validates an extraction destination boundary.
    pub fn validate_destination(&self, base_dir: &Path, rel_path: &Path) -> Result<PathBuf> {
        self.security_context
            .policy()
            .validate_destination(base_dir, rel_path)
            .map_err(Into::into)
    }

    /// Returns structured runtime info for diagnostics and CLI info queries.
    #[must_use]
    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            version: env!("CARGO_PKG_VERSION"),
            platform: self.platform_info.clone(),
            policy: self.security_context.policy().clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_app_info() {
        let app = Application::default();
        let info = app.app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert!(!info.policy.allow_absolute_paths);
    }

    #[test]
    fn test_app_validate_path_security() {
        let app = Application::default();
        let safe = app.validate_path(Path::new("safe/path.txt"));
        assert!(safe.is_ok());

        let traversal = app.validate_path(Path::new("../escape.txt"));
        assert!(traversal.is_err());
    }

    #[test]
    fn test_inspect_nonexistent_file() {
        let app = Application::default();
        let res = app.inspect_archive(Path::new("non_existent_file.zip"));
        assert!(res.is_err());
    }

    #[test]
    fn test_inspect_valid_zip_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_archive.zip");

        // Write minimal ZIP header magic bytes: PK\x05\x06 followed by empty end-of-central-directory
        let mut f = File::create(&temp_file).unwrap();
        f.write_all(
            b"PK\x05\x06\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        )
        .unwrap();
        drop(f);

        let app = Application::default();
        let meta = app.inspect_archive(&temp_file).unwrap();
        assert_eq!(meta.format, ArchiveFormat::Zip);
        assert_eq!(meta.file_size_bytes, 22);

        let _ = std::fs::remove_file(temp_file);
    }
}
