//! Cryptographic self-update manager with atomic replacement and verification.

use crate::error::{ArchiveError, Result, SecurityError, UnarcError};
use crate::security::integrity::{
    compute_sha256, verify_executable_format, ReleaseManifest, ReleaseSignatureVerifier,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Default official update source repository.
pub const DEFAULT_UPDATE_SOURCE: &str = "https://github.com/unarc/unarc/releases/latest/download";

/// Trait abstraction for downloading release manifests and artifacts.
pub trait DownloadTransport: Send + Sync {
    /// Fetches the raw content bytes from the given source URI or path.
    fn fetch(&self, source: &str) -> Result<Vec<u8>, UnarcError>;

    /// Downloads the content from source directly into the destination file.
    fn download_to_file(&self, source: &str, destination: &Path) -> Result<(), UnarcError>;
}

/// System curl-backed transport supporting HTTPS and local file URIs.
#[derive(Debug, Default, Clone)]
pub struct SystemCurlTransport;

impl DownloadTransport for SystemCurlTransport {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, UnarcError> {
        if let Some(file_path) = source.strip_prefix("file://") {
            return std::fs::read(file_path).map_err(UnarcError::Io);
        }

        if source.starts_with('/') || Path::new(source).is_file() {
            return std::fs::read(source).map_err(UnarcError::Io);
        }

        let output = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--max-time",
                "30",
                source,
            ])
            .output()
            .map_err(|e| {
                UnarcError::Archive(ArchiveError::BackendFailure {
                    backend: "updater-curl".to_string(),
                    message: format!("Failed to invoke curl: {e}"),
                })
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(UnarcError::Archive(ArchiveError::BackendFailure {
                backend: "updater-curl".to_string(),
                message: format!("Download failed from {source}: {stderr}"),
            }));
        }

        Ok(output.stdout)
    }

    fn download_to_file(&self, source: &str, destination: &Path) -> Result<(), UnarcError> {
        let bytes = self.fetch(source)?;
        let mut file = File::create(destination)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }
}

/// In-memory mock transport for testing and local fixtures.
#[derive(Debug, Default, Clone)]
pub struct MockDownloadTransport {
    files: HashMap<String, Vec<u8>>,
}

impl MockDownloadTransport {
    /// Creates a new empty mock transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Registers a source URI with mock data.
    pub fn register(&mut self, source: impl Into<String>, data: Vec<u8>) {
        self.files.insert(source.into(), data);
    }
}

impl DownloadTransport for MockDownloadTransport {
    fn fetch(&self, source: &str) -> Result<Vec<u8>, UnarcError> {
        if let Some(file_path) = source.strip_prefix("file://") {
            if let Ok(bytes) = std::fs::read(file_path) {
                return Ok(bytes);
            }
        }

        if let Some(data) = self.files.get(source) {
            Ok(data.clone())
        } else if let Ok(bytes) = std::fs::read(source) {
            Ok(bytes)
        } else {
            Err(UnarcError::Archive(ArchiveError::BackendFailure {
                backend: "mock-updater".to_string(),
                message: format!("Mock source not found: {source}"),
            }))
        }
    }

    fn download_to_file(&self, source: &str, destination: &Path) -> Result<(), UnarcError> {
        let bytes = self.fetch(source)?;
        let mut file = File::create(destination)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }
}

/// Outcome of checking for available updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Currently installed binary version.
    pub current_version: String,
    /// Latest release version reported by manifest.
    pub latest_version: String,
    /// Whether the latest version is newer than current version.
    pub update_available: bool,
    /// Cryptographically verified release manifest.
    pub manifest: ReleaseManifest,
}

/// Outcome of applying an update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateApplyResult {
    /// Previous version before update.
    pub previous_version: String,
    /// Newly installed version.
    pub new_version: String,
    /// Path to the updated executable.
    pub binary_path: PathBuf,
}

/// Compares two semver strings (e.g. "0.2.0" and "0.3.0"). Returns true if `remote` > `current`.
#[must_use]
pub fn is_version_newer(remote: &str, current: &str) -> bool {
    let parse = |v: &str| -> (u32, u32, u32) {
        let parts: Vec<u32> = v
            .trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        (
            *parts.first().unwrap_or(&0),
            *parts.get(1).unwrap_or(&0),
            *parts.get(2).unwrap_or(&0),
        )
    };

    let r = parse(remote);
    let c = parse(current);
    r > c
}

/// RAII cleanup guard for staging update files.
struct StagingGuard<'a> {
    path: &'a Path,
    disarmed: bool,
}

impl<'a> StagingGuard<'a> {
    fn new(path: &'a Path) -> Self {
        Self {
            path,
            disarmed: false,
        }
    }

    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for StagingGuard<'_> {
    fn drop(&mut self) {
        if !self.disarmed && self.path.exists() {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

/// Manages explicit Unarc self-updates with cryptographic signature verification.
#[derive(Debug, Clone)]
pub struct UpdateManager<T: DownloadTransport = SystemCurlTransport> {
    transport: T,
    verifier: ReleaseSignatureVerifier,
    default_source: String,
}

impl Default for UpdateManager<SystemCurlTransport> {
    fn default() -> Self {
        Self::new(
            SystemCurlTransport,
            ReleaseSignatureVerifier::official(),
            None,
        )
    }
}

impl<T: DownloadTransport> UpdateManager<T> {
    /// Creates a new update manager instance.
    pub fn new(
        transport: T,
        verifier: ReleaseSignatureVerifier,
        default_source: Option<String>,
    ) -> Self {
        Self {
            transport,
            verifier,
            default_source: default_source.unwrap_or_else(|| DEFAULT_UPDATE_SOURCE.to_string()),
        }
    }

    /// Resolves the full URL or path to the release manifest.
    fn resolve_manifest_source(&self, source_override: Option<&str>) -> (String, String) {
        let base = source_override
            .unwrap_or(&self.default_source)
            .trim_end_matches('/');

        if base.ends_with(".json") {
            let parent = Path::new(base)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            (base.to_string(), parent)
        } else {
            (format!("{base}/manifest.json"), base.to_string())
        }
    }

    /// Checks the update source for a new signed release.
    pub fn check_for_update(
        &self,
        source_override: Option<&str>,
    ) -> Result<UpdateCheckResult, UnarcError> {
        let (manifest_url, _) = self.resolve_manifest_source(source_override);
        let manifest_bytes = self.transport.fetch(&manifest_url)?;

        let manifest: ReleaseManifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
            UnarcError::Security(SecurityError::PolicyViolation {
                reason: format!("Failed to parse release manifest JSON: {e}"),
            })
        })?;

        // 1. Verify cryptographic Ed25519 signature
        manifest
            .verify_signature(&self.verifier)
            .map_err(UnarcError::Security)?;

        // 2. Verify target architecture compatibility
        manifest
            .verify_architecture_compatibility()
            .map_err(UnarcError::Security)?;

        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let update_available = is_version_newer(&manifest.version, &current_version);

        Ok(UpdateCheckResult {
            current_version,
            latest_version: manifest.version.clone(),
            update_available,
            manifest,
        })
    }

    /// Downloads, verifies, and atomically replaces the running binary.
    pub fn apply_update(
        &self,
        source_override: Option<&str>,
        target_executable: Option<&Path>,
    ) -> Result<UpdateApplyResult, UnarcError> {
        // 1. Resolve and validate current binary path upfront before downloading
        let target_exe = if let Some(p) = target_executable {
            p.to_path_buf()
        } else {
            std::env::current_exe()?
        };

        // Verify update-path safety: reject updating through a symlinked binary path
        let meta = std::fs::symlink_metadata(&target_exe).map_err(UnarcError::Io)?;
        if meta.file_type().is_symlink() {
            return Err(UnarcError::Security(SecurityError::InsecureSymlink {
                target: format!(
                    "Refusing to update through a symlinked binary path: '{}'. Update the real target binary directly.",
                    target_exe.display()
                ),
            }));
        }

        if !meta.is_file() {
            return Err(UnarcError::Archive(ArchiveError::FileNotFound {
                path: target_exe.display().to_string(),
            }));
        }

        let parent_dir = target_exe.parent().ok_or_else(|| {
            UnarcError::Security(SecurityError::InvalidPath {
                details: "Target binary has no parent directory".to_string(),
            })
        })?;

        // 2. Query release source and verify manifest
        let check_res = self.check_for_update(source_override)?;
        let manifest = check_res.manifest;
        let (_, base_url) = self.resolve_manifest_source(source_override);

        // 2. Resolve artifact URL
        let artifact_source = if manifest.artifact_url.starts_with("http://")
            || manifest.artifact_url.starts_with("https://")
            || manifest.artifact_url.starts_with("file://")
            || (manifest.artifact_url.starts_with('/')
                && Path::new(&manifest.artifact_url).is_file())
        {
            manifest.artifact_url.clone()
        } else {
            format!("{base_url}/{}", manifest.artifact_url)
        };

        // 3. Staging file in SAME parent directory for POSIX atomic rename
        let staging_name = format!(
            ".unarc_staging_{}_{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros()
        );
        let staging_path = parent_dir.join(staging_name);
        let mut guard = StagingGuard::new(&staging_path);

        // 4. Download artifact to staging file
        self.transport
            .download_to_file(&artifact_source, &staging_path)?;

        // 5. Verify downloaded file SHA-256
        let computed_sha = compute_sha256(&staging_path)?;
        if computed_sha.to_lowercase() != manifest.artifact_sha256.to_lowercase() {
            return Err(UnarcError::Security(SecurityError::PolicyViolation {
                reason: format!(
                    "Artifact SHA-256 mismatch: expected {}, got {}",
                    manifest.artifact_sha256, computed_sha
                ),
            }));
        }

        // 6. Verify executable binary header/format
        let mut header_buf = vec![0u8; 4096];
        let bytes_read = {
            let mut f = File::open(&staging_path)?;
            f.read(&mut header_buf)?
        };
        verify_executable_format(&header_buf[..bytes_read], &manifest.target_os)
            .map_err(UnarcError::Security)?;

        // 7. Set POSIX executable permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755))?;
        }

        // 8. Atomic replacement via rename
        std::fs::rename(&staging_path, &target_exe).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                UnarcError::Security(SecurityError::PermissionDenied {
                    operation: format!(
                        "Replace executable '{}': permission denied",
                        target_exe.display()
                    ),
                })
            }
            _ => UnarcError::Io(e),
        })?;

        // 9. Disarm rollback guard
        guard.disarm();

        Ok(UpdateApplyResult {
            previous_version: check_res.current_version,
            new_version: manifest.version,
            binary_path: target_exe,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::integrity::{
        expected_engine_binary_sha256, OFFICIAL_RELEASE_SIGNING_SEED,
    };
    use ed25519_dalek::SigningKey;

    #[test]
    fn test_version_newer_logic() {
        assert!(is_version_newer("0.3.0", "0.2.0"));
        assert!(is_version_newer("1.0.0", "0.9.9"));
        assert!(is_version_newer("0.2.1", "0.2.0"));
        assert!(!is_version_newer("0.2.0", "0.2.0"));
        assert!(!is_version_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn test_mock_update_flow() {
        let signing_key = SigningKey::from_bytes(&OFFICIAL_RELEASE_SIGNING_SEED);
        let verifier = ReleaseSignatureVerifier::from_verifying_key(signing_key.verifying_key());

        // Create dummy valid executable binary for current OS
        let mut dummy_binary = vec![0u8; 2048];
        #[cfg(target_os = "macos")]
        dummy_binary[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
        #[cfg(target_os = "linux")]
        dummy_binary[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);

        let bin_sha = crate::security::integrity::compute_sha256_bytes(&dummy_binary);

        let mut manifest = ReleaseManifest {
            version: "0.9.0".to_string(),
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            bundled_7zz_version: crate::archive::bundled::PINNED_7ZIP_VERSION.to_string(),
            bundled_7zz_sha256: expected_engine_binary_sha256().to_string(),
            artifact_url: "unarc_new".to_string(),
            artifact_sha256: bin_sha,
            signature: None,
        };
        manifest.sign(&signing_key);

        let manifest_json = serde_json::to_vec(&manifest).unwrap();

        let mut mock_transport = MockDownloadTransport::new();
        mock_transport.register("https://update.test/manifest.json", manifest_json);
        mock_transport.register("https://update.test/unarc_new", dummy_binary.clone());

        let manager = UpdateManager::new(
            mock_transport,
            verifier,
            Some("https://update.test".to_string()),
        );

        let check = manager.check_for_update(None).unwrap();
        assert_eq!(check.latest_version, "0.9.0");
        assert!(check.update_available);

        // Test apply update to temporary file target
        let temp_dir =
            std::env::temp_dir().join(format!("unarc_update_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let target_bin = temp_dir.join("unarc_target");
        std::fs::write(&target_bin, b"original").unwrap();

        let apply_res = manager.apply_update(None, Some(&target_bin)).unwrap();
        assert_eq!(apply_res.new_version, "0.9.0");
        let updated_bytes = std::fs::read(&target_bin).unwrap();
        assert_eq!(updated_bytes, dummy_binary);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
