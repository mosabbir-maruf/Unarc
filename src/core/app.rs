//! Core application coordinator for Unarc operations.

use crate::archive::backend::{ArchiveBackend, ArchiveExtractResult, ArchiveTestResult};
use crate::archive::bundled::{
    resolve_bundled_engine, SevenZipBackend, EXPECTED_SHA256_LINUX_ARM64,
    EXPECTED_SHA256_LINUX_X64, EXPECTED_SHA256_MACOS, PINNED_7ZIP_RELEASE_URL, PINNED_7ZIP_VERSION,
};
use crate::archive::format::ArchiveFormat;
use crate::archive::metadata::ArchiveMetadata;
use crate::archive::volume::VolumeResolver;
use crate::error::{ArchiveError, Result};
use crate::platform::PlatformInfo;
use crate::security::{SecurityContext, SecurityPolicy};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Detailed engine information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    /// Pinned engine release version.
    pub pinned_version: String,

    /// Official download / release URL.
    pub release_url: String,

    /// Target architecture expected binary SHA-256.
    pub expected_sha256: String,

    /// Resolved executable path on disk, if available.
    pub resolved_path: Option<PathBuf>,

    /// Whether the bundled engine is functional.
    pub is_available: bool,
}

/// An individual diagnostic probe result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    /// Name of the diagnostic probe.
    pub name: String,

    /// Whether the probe succeeded.
    pub passed: bool,

    /// Descriptive outcome or error message.
    pub message: String,
}

/// Diagnostic health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Operating system and CPU info.
    pub platform: PlatformInfo,

    /// Bundled engine diagnostics.
    pub engine: EngineInfo,

    /// Active security configuration.
    pub policy: SecurityPolicy,

    /// Diagnostic probe outcomes.
    pub checks: Vec<DiagnosticCheck>,

    /// Overall operational health status.
    pub healthy: bool,
}

/// System status and build metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    /// Binary package version.
    pub version: String,

    /// Platform environment.
    pub platform: PlatformInfo,

    /// Bundled archive engine information.
    pub engine: EngineInfo,

    /// Active security policy configuration.
    pub policy: SecurityPolicy,
}

/// Core application instance orchestrating security and archive workflows.
#[derive(Debug, Clone)]
pub struct Application {
    security_context: SecurityContext,
    platform_info: PlatformInfo,
    backend: SevenZipBackend,
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
            backend: SevenZipBackend::new(),
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

    /// Expected SHA-256 for the current host architecture.
    #[must_use]
    pub fn expected_engine_sha256(&self) -> &'static str {
        if self.platform_info.is_macos {
            EXPECTED_SHA256_MACOS
        } else if self.platform_info.arch == "aarch64" {
            EXPECTED_SHA256_LINUX_ARM64
        } else {
            EXPECTED_SHA256_LINUX_X64
        }
    }

    /// Returns engine status information.
    #[must_use]
    pub fn engine_info(&self) -> EngineInfo {
        let resolved = resolve_bundled_engine().ok();
        let is_available = resolved.is_some();
        EngineInfo {
            pinned_version: PINNED_7ZIP_VERSION.to_string(),
            release_url: PINNED_7ZIP_RELEASE_URL.to_string(),
            expected_sha256: self.expected_engine_sha256().to_string(),
            resolved_path: resolved,
            is_available,
        }
    }

    /// Runs a comprehensive system and engine health check with active diagnostic probes.
    #[must_use]
    pub fn doctor_check(&self) -> DoctorReport {
        let engine = self.engine_info();
        let mut checks = Vec::new();

        // Probe 1: Platform & Architecture Detection
        checks.push(DiagnosticCheck {
            name: "Platform Detection".to_string(),
            passed: true,
            message: format!("{}-{}", self.platform_info.os, self.platform_info.arch),
        });

        // Probe 2: Bundled Engine Resolution
        let engine_resolved = engine.resolved_path.is_some();
        checks.push(DiagnosticCheck {
            name: "Bundled Engine Resolution".to_string(),
            passed: engine_resolved,
            message: engine
                .resolved_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Bundled 7zz engine not found in search paths".to_string()),
        });

        // Probe 3: Bundled Engine Execution Probe
        let mut exec_probe_passed = false;
        let exec_message = if let Some(ref path) = engine.resolved_path {
            match std::process::Command::new(path).arg("i").output() {
                Ok(out) => {
                    let out_str = String::from_utf8_lossy(&out.stdout);
                    if out.status.success() && out_str.contains("7-Zip") {
                        exec_probe_passed = true;
                        format!("Engine execution successful (v{})", PINNED_7ZIP_VERSION)
                    } else {
                        format!("Engine execution failed with exit code: {:?}", out.status)
                    }
                }
                Err(e) => format!("Failed to spawn engine binary: {e}"),
            }
        } else {
            "Engine execution probe skipped: engine binary not located".to_string()
        };

        checks.push(DiagnosticCheck {
            name: "Engine Execution Probe".to_string(),
            passed: exec_probe_passed,
            message: exec_message,
        });

        // Probe 4: Temp/Scratch Filesystem Access
        let temp_probe = std::env::temp_dir().join(".unarc_doctor_probe");
        let temp_passed = match std::fs::write(&temp_probe, b"probe") {
            Ok(()) => {
                let _ = std::fs::remove_file(&temp_probe);
                true
            }
            Err(_) => false,
        };
        checks.push(DiagnosticCheck {
            name: "Filesystem Scratch Workspace".to_string(),
            passed: temp_passed,
            message: if temp_passed {
                "Temporary directory is accessible and writable".to_string()
            } else {
                "Unable to write to temporary workspace directory".to_string()
            },
        });

        // Probe 5: Security Policy Integrity
        let traversal_check = self.validate_path(Path::new("../escape.txt")).is_err();
        checks.push(DiagnosticCheck {
            name: "Zero-Trust Policy Enforcement".to_string(),
            passed: traversal_check,
            message: if traversal_check {
                "Traversal defense and containment verified".to_string()
            } else {
                "Security policy failed traversal rejection test".to_string()
            },
        });

        let healthy = checks.iter().all(|c| c.passed);

        DoctorReport {
            platform: self.platform_info.clone(),
            engine,
            policy: self.security_context.policy().clone(),
            checks,
            healthy,
        }
    }

    /// Tests the integrity of an archive.
    pub fn test_archive(&self, path: &Path, password: Option<&str>) -> Result<ArchiveTestResult> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            }
            .into());
        }

        // 1. Deterministic volume sequence resolution
        let volume_set = VolumeResolver::resolve(path, self.security_context.policy())?;

        // 2. Run integrity check on the primary volume
        let mut res = self.backend.test(&volume_set.primary_volume, password)?;
        res.path = path.to_path_buf();
        res.format = volume_set.format;
        Ok(res)
    }

    /// Securely extracts an archive to a specified or default destination.
    pub fn extract_archive(
        &self,
        path: &Path,
        output: Option<&Path>,
        password: Option<&str>,
    ) -> Result<ArchiveExtractResult> {
        if !path.exists() {
            return Err(ArchiveError::FileNotFound {
                path: path.to_string_lossy().to_string(),
            }
            .into());
        }

        // 1. Deterministic volume resolution (fails immediately on missing or invalid volumes)
        let volume_set = VolumeResolver::resolve(path, self.security_context.policy())?;

        // 2. Determine destination directory
        let destination = if let Some(out) = output {
            out.to_path_buf()
        } else {
            // Default stem directory in current working directory
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "extracted".to_string());

            let clean_stem = if let Some((st, _, _)) =
                crate::archive::volume::parse_modern_part_filename(&file_name)
            {
                st
            } else if let Some((st, _)) =
                crate::archive::volume::parse_legacy_part_filename(&file_name)
            {
                st
            } else {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "extracted".to_string())
            };
            std::env::current_dir()?.join(clean_stem)
        };

        // 3. Pre-extract security inspection:
        // List entries in the archive to detect path traversal, absolute paths, or unauthorized symlinks upfront
        if let Ok(entries) = self
            .backend
            .list_entries(&volume_set.primary_volume, password)
        {
            for entry in entries {
                self.security_context
                    .policy()
                    .validate_entry_path(&entry.path)?;

                if entry.is_symlink && !self.security_context.policy().allow_symlinks {
                    return Err(crate::error::SecurityError::InsecureSymlink {
                        target: entry.path.to_string_lossy().to_string(),
                    }
                    .into());
                }
            }
        }

        // 4. Extract through bundled engine backend
        let mut result =
            self.backend
                .extract(&volume_set.primary_volume, &destination, password)?;
        result.archive_path = path.to_path_buf();
        result.format = volume_set.format;

        // 5. Post-extraction safety verification: verify boundary containment
        self.verify_extracted_destination(&destination, &mut result)?;

        Ok(result)
    }

    /// Verifies that all extracted files remain safely contained within the destination boundary.
    fn verify_extracted_destination(
        &self,
        destination: &Path,
        result: &mut ArchiveExtractResult,
    ) -> Result<()> {
        let dest_canonical = destination
            .canonicalize()
            .map_err(crate::error::UnarcError::Io)?;

        let mut entry_count = 0usize;
        let mut total_bytes = 0u64;

        let mut stack = vec![dest_canonical.clone()];
        while let Some(current_dir) = stack.pop() {
            let read_dir = std::fs::read_dir(&current_dir)?;
            for entry_res in read_dir {
                let entry = entry_res?;
                let path = entry.path();
                let symlink_meta = std::fs::symlink_metadata(&path)?;

                // Reject any created symlinks if policy forbids symlinks
                if symlink_meta.file_type().is_symlink()
                    && !self.security_context.policy().allow_symlinks
                {
                    let _ = std::fs::remove_file(&path);
                    return Err(crate::error::SecurityError::InsecureSymlink {
                        target: path.to_string_lossy().to_string(),
                    }
                    .into());
                }

                // Check that canonical path stays within destination
                if let Ok(canon) = path.canonicalize() {
                    if !canon.starts_with(&dest_canonical) {
                        return Err(crate::error::SecurityError::BoundaryEscaped {
                            path: path.to_string_lossy().to_string(),
                        }
                        .into());
                    }
                }

                if symlink_meta.is_dir() {
                    stack.push(path);
                } else if symlink_meta.is_file() {
                    entry_count += 1;
                    total_bytes += symlink_meta.len();
                }
            }
        }

        result.entries_extracted = Some(entry_count);
        result.total_bytes_extracted = Some(total_bytes);

        Ok(())
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
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: self.platform_info.clone(),
            engine: self.engine_info(),
            policy: self.security_context.policy().clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_info() {
        let app = Application::default();
        let info = app.app_info();
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.engine.pinned_version, "26.03");
        assert!(!info.policy.allow_absolute_paths);
    }

    #[test]
    fn test_doctor_check() {
        let app = Application::default();
        let doc = app.doctor_check();
        assert_eq!(doc.engine.pinned_version, "26.03");
        assert!(!doc.checks.is_empty());
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
}
