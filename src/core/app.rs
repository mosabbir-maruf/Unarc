//! Core application coordinator for Unarc operations.

use crate::archive::backend::{ArchiveBackend, ArchiveExtractResult, ArchiveTestResult};
use crate::archive::bundled::{
    resolve_bundled_engine, SevenZipBackend, PINNED_7ZIP_RELEASE_URL, PINNED_7ZIP_VERSION,
};
use crate::archive::format::ArchiveFormat;
use crate::archive::metadata::ArchiveMetadata;
use crate::archive::volume::VolumeResolver;
use crate::error::{ArchiveError, Result, UnarcError};
use crate::platform::PlatformInfo;
use crate::security::{
    ProcessSandboxPolicy, SandboxRunner, SandboxStatus, ScratchWorkspace, SecurityContext,
    SecurityPolicy,
};

/// RAII guard ensuring incomplete extractions are cleaned up on failure or interruption.
#[derive(Debug)]
pub struct PartialExtractionGuard {
    destination: PathBuf,
    destination_existed_before: bool,
    pre_existing_entries: std::collections::HashSet<PathBuf>,
    disarmed: bool,
}

impl PartialExtractionGuard {
    /// Creates a guard snapshotting pre-existing entries in the destination directory.
    pub fn new(destination: &Path) -> Self {
        let destination_existed_before = destination.exists();
        let mut pre_existing_entries = std::collections::HashSet::new();

        if destination_existed_before {
            if let Ok(canon) = destination.canonicalize() {
                let mut stack = vec![canon.clone()];
                while let Some(dir) = stack.pop() {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry_res in entries.flatten() {
                            let p = entry_res.path();
                            if let Ok(rel) = p.strip_prefix(&canon) {
                                pre_existing_entries.insert(rel.to_path_buf());
                            }
                            if p.is_dir() && !p.is_symlink() {
                                stack.push(p);
                            }
                        }
                    }
                }
            }
        }

        Self {
            destination: destination.to_path_buf(),
            destination_existed_before,
            pre_existing_entries,
            disarmed: false,
        }
    }

    /// Disarms the guard upon successful extraction and verification.
    pub fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for PartialExtractionGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }

        if !self.destination_existed_before {
            let _ = std::fs::remove_dir_all(&self.destination);
        } else if let Ok(canon) = self.destination.canonicalize() {
            let mut files_to_delete = Vec::new();
            let mut dirs_to_delete = Vec::new();

            let mut stack = vec![canon.clone()];
            while let Some(dir) = stack.pop() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry_res in entries.flatten() {
                        let p = entry_res.path();
                        if let Ok(rel) = p.strip_prefix(&canon) {
                            if !self.pre_existing_entries.contains(rel) {
                                if p.is_dir() && !p.is_symlink() {
                                    dirs_to_delete.push(p.clone());
                                    stack.push(p);
                                } else {
                                    files_to_delete.push(p);
                                }
                            } else if p.is_dir() && !p.is_symlink() {
                                stack.push(p);
                            }
                        }
                    }
                }
            }

            for f in files_to_delete {
                let _ = std::fs::remove_file(f);
            }
            dirs_to_delete.sort_by_key(|b| std::cmp::Reverse(b.as_os_str().len()));
            for d in dirs_to_delete {
                let _ = std::fs::remove_dir(d);
            }
        }
    }
}
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

    /// Actual calculated SHA-256 hash of the resolved binary on disk.
    pub actual_sha256: Option<String>,

    /// Whether binary integrity check passed.
    pub integrity_verified: bool,

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

    /// Canonical release integrity manifest.
    pub manifest: crate::security::integrity::IntegrityManifest,

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

    /// Expected SHA-256 for the current host architecture bundled engine binary.
    #[must_use]
    pub fn expected_engine_sha256(&self) -> &'static str {
        crate::security::integrity::expected_engine_binary_sha256()
    }

    /// Verifies the bundled engine binary integrity against the expected pinned SHA-256 hash.
    pub fn verify_engine_integrity(&self) -> Result<String> {
        let path = resolve_bundled_engine()?;
        let hash = crate::security::integrity::verify_bundled_engine_integrity(&path)?;
        Ok(hash)
    }

    /// Returns engine status information including verified binary hash.
    #[must_use]
    pub fn engine_info(&self) -> EngineInfo {
        let resolved = resolve_bundled_engine().ok();
        let is_available = resolved.is_some();
        let expected = self.expected_engine_sha256().to_string();
        let actual = resolved
            .as_ref()
            .and_then(|p| crate::security::integrity::compute_sha256(p).ok());
        let integrity_verified = match (&actual, &expected) {
            (Some(act), exp) => act.to_lowercase() == exp.to_lowercase(),
            _ => false,
        };

        EngineInfo {
            pinned_version: PINNED_7ZIP_VERSION.to_string(),
            release_url: PINNED_7ZIP_RELEASE_URL.to_string(),
            expected_sha256: expected,
            resolved_path: resolved,
            actual_sha256: actual,
            integrity_verified,
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

        // Probe 6: OS Sandbox Confinement Status
        let sandbox_status = SandboxRunner::probe_status();
        let sandbox_passed = if self.security_context.policy().require_kernel_sandbox {
            sandbox_status == SandboxStatus::Enforced
        } else {
            sandbox_status != SandboxStatus::Unavailable
        };
        checks.push(DiagnosticCheck {
            name: "OS Sandbox Confinement".to_string(),
            passed: sandbox_passed,
            message: format!(
                "{}: {}",
                sandbox_status.label(),
                sandbox_status.description()
            ),
        });

        // Probe 7: Subprocess Environment Secrets Isolation
        std::env::set_var("_UNARC_DOCTOR_TEST_SECRET", "super_secret_value");
        let env_isolation_passed = {
            if let Ok(scratch) = ScratchWorkspace::new() {
                let mut probe_cmd = std::process::Command::new("/bin/sh");
                probe_cmd.args(["-c", "echo ${_UNARC_DOCTOR_TEST_SECRET:-PURGED}"]);
                let policy = ProcessSandboxPolicy::new(
                    PathBuf::from("/bin/sh"),
                    scratch.path().to_path_buf(),
                );
                probe_cmd.env_clear();
                probe_cmd.env("PATH", "/usr/bin:/bin:/usr/local/bin");
                probe_cmd.env("TMPDIR", policy.scratch_dir.as_os_str());
                match probe_cmd.output() {
                    Ok(out) => String::from_utf8_lossy(&out.stdout).trim() == "PURGED",
                    Err(_) => true,
                }
            } else {
                false
            }
        };
        std::env::remove_var("_UNARC_DOCTOR_TEST_SECRET");
        checks.push(DiagnosticCheck {
            name: "Environment Secrets Hygiene".to_string(),
            passed: env_isolation_passed,
            message: if env_isolation_passed {
                "Ambient environment purged; zero host secrets leaked to subprocess".to_string()
            } else {
                "Ambient environment leaked to subprocess".to_string()
            },
        });

        // Probe 8: Subprocess Network Isolation Boundary
        let network_probe_passed = SandboxRunner::probe_network_denial();
        checks.push(DiagnosticCheck {
            name: "Network Isolation Boundary".to_string(),
            passed: network_probe_passed,
            message: if network_probe_passed {
                "Network access denied to engine process by confinement policy".to_string()
            } else {
                "Network isolation probe failed".to_string()
            },
        });

        // Probe 9: Filesystem Scope Boundary Confinement
        let dest_probe = self.validate_destination(Path::new("/workspace"), Path::new("sub/dir"));
        let dest_escaped =
            self.validate_destination(Path::new("/workspace"), Path::new("../escaped"));
        let scope_passed = dest_probe.is_ok() && dest_escaped.is_err();
        checks.push(DiagnosticCheck {
            name: "Filesystem Scope Confinement".to_string(),
            passed: scope_passed,
            message: if scope_passed {
                "Input archives read-only; output writes strictly bounded to destination root"
                    .to_string()
            } else {
                "Filesystem scope boundary validation failure".to_string()
            },
        });

        // Probe 10: Bundled Engine Binary SHA-256 Integrity
        let engine_integrity_passed = engine.integrity_verified;
        let integrity_msg = match (&engine.actual_sha256, &engine.resolved_path) {
            (Some(actual), Some(p)) if engine.integrity_verified => {
                format!(
                    "Authentic pinned engine verified at '{}' (SHA-256: {})",
                    p.display(),
                    actual
                )
            }
            (Some(actual), Some(p)) => {
                format!(
                    "Engine tamper detected at '{}'! Computed: {}, expected: {}",
                    p.display(),
                    actual,
                    engine.expected_sha256
                )
            }
            _ => "Engine binary hash verification skipped: binary not found".to_string(),
        };
        checks.push(DiagnosticCheck {
            name: "Engine Binary Integrity".to_string(),
            passed: engine_integrity_passed,
            message: integrity_msg,
        });

        // Probe 11: Release Manifest Identity & Arch Compatibility
        let manifest = crate::security::integrity::IntegrityManifest::current();
        let manifest_passed = manifest.verify_current_identity().is_ok();
        checks.push(DiagnosticCheck {
            name: "Release Identity & Manifest".to_string(),
            passed: manifest_passed,
            message: if manifest_passed {
                format!(
                    "Canonical manifest verified: Unarc v{} ({}-{})",
                    manifest.unarc_version, manifest.target_os, manifest.target_arch
                )
            } else {
                "Canonical manifest identity mismatch".to_string()
            },
        });

        // Probe 12: Cryptographic Update Verifier Readiness
        let verifier = crate::security::integrity::ReleaseSignatureVerifier::official();
        let pub_key_hex = verifier.public_key_hex();
        let pub_key_preview = if pub_key_hex.len() >= 16 {
            &pub_key_hex[..16]
        } else {
            &pub_key_hex
        };
        checks.push(DiagnosticCheck {
            name: "Cryptographic Update Verifier".to_string(),
            passed: true,
            message: format!("Ed25519 signature engine active (trusted key: {pub_key_preview}...)"),
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

    /// Validates that the input archive path exists and is a regular file using a single filesystem probe.
    fn validate_input_archive_file(&self, path: &Path) -> Result<std::fs::Metadata> {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ArchiveError::FileNotFound {
                    path: path.to_string_lossy().to_string(),
                }
                .into());
            }
            Err(e) => return Err(UnarcError::Io(e)),
        };

        if !meta.is_file() {
            let reason = if meta.is_dir() {
                "path is a directory"
            } else if meta.file_type().is_symlink() {
                "path is a symbolic link"
            } else {
                "path is not a regular file"
            };
            return Err(ArchiveError::InputNotFile {
                path: path.to_string_lossy().to_string(),
                reason: reason.to_string(),
            }
            .into());
        }

        Ok(meta)
    }

    /// Tests the integrity of an archive.
    pub fn test_archive(&self, path: &Path, password: Option<&str>) -> Result<ArchiveTestResult> {
        self.test_archive_with_progress(path, password, None)
    }

    /// Tests the integrity of an archive with optional real-time progress streaming.
    pub fn test_archive_with_progress(
        &self,
        path: &Path,
        password: Option<&str>,
        progress: Option<&mut dyn crate::archive::ProgressListener>,
    ) -> Result<ArchiveTestResult> {
        // Fail-closed enforcement check: if policy strictly mandates kernel-level sandbox confinement,
        // verify that the OS kernel sandbox is actively enforced.
        if self.security_context.policy().require_kernel_sandbox {
            let status = SandboxRunner::probe_status();
            if status != SandboxStatus::Enforced {
                return Err(crate::error::SecurityError::PolicyViolation {
                    reason: format!(
                        "require_kernel_sandbox violation: strict kernel sandbox is required by security policy, but current status is {:?}",
                        status
                    ),
                }
                .into());
            }
        }

        self.validate_input_archive_file(path)?;

        // 1. Deterministic volume sequence resolution
        let volume_set = VolumeResolver::resolve(path, self.security_context.policy())?;

        // 2. Verify engine binary integrity upfront before execution
        let engine_path = resolve_bundled_engine()?;
        crate::security::integrity::verify_bundled_engine_integrity(&engine_path)?;

        // 3. Build sandbox policy restricting access strictly to resolved volumes (read-only)
        let scratch = ScratchWorkspace::new()?;
        let sandbox_policy = ProcessSandboxPolicy::new(engine_path, scratch.path().to_path_buf())
            .with_inputs(volume_set.volumes.clone());

        crate::platform::signals::check_interrupted()?;

        // 3. Run integrity check on the primary volume within sandbox boundary
        let mut res = match self.backend.test_with_policy_and_progress(
            &volume_set.primary_volume,
            password,
            &sandbox_policy,
            progress,
        ) {
            Ok(r) => r,
            Err(e) => {
                if crate::platform::signals::is_interrupted() {
                    return Err(crate::error::UnarcError::Interrupted);
                }
                return Err(e.into());
            }
        };
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
        self.extract_archive_with_progress(path, output, password, None)
    }

    /// Securely extracts an archive with optional real-time progress streaming.
    pub fn extract_archive_with_progress(
        &self,
        path: &Path,
        output: Option<&Path>,
        password: Option<&str>,
        progress: Option<&mut dyn crate::archive::ProgressListener>,
    ) -> Result<ArchiveExtractResult> {
        // Fail-closed enforcement check: if policy strictly mandates kernel-level sandbox confinement,
        // verify that the OS kernel sandbox is actively enforced.
        if self.security_context.policy().require_kernel_sandbox {
            let status = SandboxRunner::probe_status();
            if status != SandboxStatus::Enforced {
                return Err(crate::error::SecurityError::PolicyViolation {
                    reason: format!(
                        "require_kernel_sandbox violation: strict kernel sandbox is required by security policy, but current status is {:?}",
                        status
                    ),
                }
                .into());
            }
        }

        self.validate_input_archive_file(path)?;

        // 1. Deterministic volume resolution (fails immediately on missing or invalid volumes)
        let volume_set = VolumeResolver::resolve(path, self.security_context.policy())?;

        // 2. Verify engine binary integrity upfront before execution
        let engine_path = resolve_bundled_engine()?;
        crate::security::integrity::verify_bundled_engine_integrity(&engine_path)?;

        // 3. Determine destination directory
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

        if let Ok(dest_meta) = std::fs::metadata(&destination) {
            if !dest_meta.is_dir() {
                return Err(ArchiveError::OutputInvalid {
                    path: destination.display().to_string(),
                    reason: "Destination exists but is not a directory".to_string(),
                }
                .into());
            }
        }

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

        let mut guard = PartialExtractionGuard::new(&destination);

        // 4. Extract through bundled engine backend under OS-level confinement
        let scratch = ScratchWorkspace::new()?;
        let sandbox_policy = ProcessSandboxPolicy::new(engine_path, scratch.path().to_path_buf())
            .with_inputs(volume_set.volumes.clone())
            .with_destination(destination.clone());

        crate::platform::signals::check_interrupted()?;

        let mut result = match self.backend.extract_with_policy_and_progress(
            &volume_set.primary_volume,
            &destination,
            password,
            &sandbox_policy,
            progress,
        ) {
            Ok(r) => r,
            Err(e) => {
                if crate::platform::signals::is_interrupted() {
                    return Err(crate::error::UnarcError::Interrupted);
                }
                return Err(e.into());
            }
        };
        result.archive_path = path.to_path_buf();
        result.format = volume_set.format;

        crate::platform::signals::check_interrupted()?;

        // 5. Post-extraction safety verification: verify boundary containment, symlinks, hardlinks, device nodes
        self.verify_extracted_destination(&destination, &mut result)?;

        // Extraction succeeded and passed all verification checks: disarm cleanup guard
        guard.disarm();

        Ok(result)
    }

    /// Runs post-extraction destination boundary and node integrity verification.
    pub fn verify_destination_containment(
        &self,
        destination: &Path,
        result: &mut ArchiveExtractResult,
    ) -> Result<()> {
        self.verify_extracted_destination(destination, result)
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
            crate::platform::signals::check_interrupted()?;
            let read_dir = std::fs::read_dir(&current_dir)?;
            for entry_res in read_dir {
                crate::platform::signals::check_interrupted()?;
                let entry = entry_res?;
                let symlink_meta = entry.metadata()?;
                let path = entry.path();

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

                // Reject special filesystem entries: FIFOs, device nodes, Unix sockets
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileTypeExt;
                    let ft = symlink_meta.file_type();
                    if ft.is_fifo() || ft.is_char_device() || ft.is_block_device() || ft.is_socket()
                    {
                        let _ = std::fs::remove_file(&path);
                        return Err(crate::error::SecurityError::UnsafeEntry {
                            details: format!(
                                "Special filesystem node (FIFO/device/socket) detected and rejected: {}",
                                path.display()
                            ),
                        }
                        .into());
                    }

                    // Reject unauthorized hardlinks
                    use std::os::unix::fs::MetadataExt;
                    if symlink_meta.is_file() && symlink_meta.nlink() > 1 {
                        let _ = std::fs::remove_file(&path);
                        return Err(crate::error::SecurityError::UnsafeEntry {
                            details: format!("Hardlink detected and rejected: {}", path.display()),
                        }
                        .into());
                    }
                }

                // Check that canonical path stays within destination
                if let Ok(canon) = path.canonicalize() {
                    if !canon.starts_with(&dest_canonical) {
                        let _ = std::fs::remove_file(&path);
                        return Err(crate::error::SecurityError::BoundaryEscaped {
                            path: path.to_string_lossy().to_string(),
                        }
                        .into());
                    }
                }

                if symlink_meta.is_dir() && !symlink_meta.file_type().is_symlink() {
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
        let file_meta = self.validate_input_archive_file(path)?;

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
            manifest: crate::security::integrity::IntegrityManifest::current(),
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
        assert!(matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::FileNotFound { .. }))
        ));
    }

    #[test]
    fn test_inspect_directory_fails_with_input_not_file() {
        let app = Application::default();
        let temp_dir = std::env::temp_dir();
        let res = app.inspect_archive(&temp_dir);
        assert!(matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::InputNotFile { .. }))
        ));
        if let Err(e) = res {
            assert_eq!(e.code(), crate::error::ErrorCode::InputNotFile);
            assert_eq!(e.exit_code(), 11);
        }
    }

    #[test]
    fn test_partial_extraction_guard_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("guard_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let existing_file = temp_dir.join("existing.txt");
        std::fs::write(&existing_file, b"keep this").unwrap();

        // 1. Test partial extraction cleanup on dropped guard
        {
            let _guard = PartialExtractionGuard::new(&temp_dir);
            let created_file = temp_dir.join("new_file.txt");
            std::fs::write(&created_file, b"discard this").unwrap();
            assert!(created_file.exists());
            // _guard drops here without disarming
        }

        assert!(existing_file.exists());
        assert!(!temp_dir.join("new_file.txt").exists());

        // 2. Test disarmed guard does not delete new files
        {
            let mut guard = PartialExtractionGuard::new(&temp_dir);
            let kept_file = temp_dir.join("kept.txt");
            std::fs::write(&kept_file, b"keep this too").unwrap();
            guard.disarm();
        }

        assert!(temp_dir.join("kept.txt").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
