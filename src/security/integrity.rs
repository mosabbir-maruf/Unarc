//! Cryptographic integrity verification, canonical manifests, and signature checking.

use crate::archive::bundled::PINNED_7ZIP_VERSION;
use crate::error::SecurityError;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Expected SHA-256 for bundled macOS 7zz binary.
pub const EXPECTED_ENGINE_BINARY_SHA256_MACOS: &str =
    "74b0910e50ea44d9760a57fada2192cfd530ba8bffbe7b47c412a464b796cabf";

/// Expected SHA-256 for bundled Linux aarch64 7zz binary.
pub const EXPECTED_ENGINE_BINARY_SHA256_LINUX_ARM64: &str =
    "9a26e7d54bfdae8a8f1750cdb70697547b738c2334aa89f3d3f2c8645c8443fe";

/// Expected SHA-256 for bundled Linux x86_64 7zz binary.
pub const EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64: &str =
    "3d52c92deb7e9f1bd059692eefc33f86a144cdc548acc4b6f4c809a4dd7bc369";

/// Official embedded Ed25519 public key (hex) for authenticating release manifests.
pub const OFFICIAL_RELEASE_PUBLIC_KEY_HEX: &str =
    "90cd97dbf43425cb694d386cb89f2e04fa252fafa6bffddddfc2f3fc962a94ee";

/// Test signing seed used exclusively by integration test fixtures.
/// Production release signing keys are injected exclusively via CI secrets.
pub const OFFICIAL_RELEASE_SIGNING_SEED: [u8; 32] = *b"UNARC_OFFICIAL_RELEASE_KEY_SEED!";

/// Returns the expected SHA-256 hash of the bundled 7zz binary on the current platform.
#[must_use]
pub fn expected_engine_binary_sha256() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        EXPECTED_ENGINE_BINARY_SHA256_MACOS
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        EXPECTED_ENGINE_BINARY_SHA256_LINUX_ARM64
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64
    }
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    {
        EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64
    }
}

/// Encodes a byte slice as a lower-case hexadecimal string.
#[must_use]
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Decodes a hexadecimal string into bytes.
pub fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    if trimmed.len() % 2 != 0 {
        return Err("Invalid hex string length: must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    for i in (0..trimmed.len()).step_by(2) {
        let b = u8::from_str_radix(&trimmed[i..i + 2], 16)
            .map_err(|e| format!("Invalid hex byte at index {i}: {e}"))?;
        bytes.push(b);
    }
    Ok(bytes)
}

/// Computes the SHA-256 hash of a file on disk.
pub fn compute_sha256(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let bytes_read = file.read(&mut buf)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buf[..bytes_read]);
    }
    let result = hasher.finalize();
    Ok(hex_encode(&result))
}

/// Computes the SHA-256 hash of an in-memory byte slice.
#[must_use]
pub fn compute_sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex_encode(&result)
}

/// Verifies that the bundled 7zz binary at the specified path matches its pinned expected SHA-256 hash.
pub fn verify_bundled_engine_integrity(engine_path: &Path) -> Result<String, SecurityError> {
    if !engine_path.exists() {
        return Err(SecurityError::PolicyViolation {
            reason: format!(
                "Bundled 7zz engine not found at path: '{}'",
                engine_path.display()
            ),
        });
    }

    let computed = compute_sha256(engine_path).map_err(|e| SecurityError::PolicyViolation {
        reason: format!(
            "Failed to compute SHA-256 of bundled engine '{}': {e}",
            engine_path.display()
        ),
    })?;

    let expected = expected_engine_binary_sha256();

    if computed.to_lowercase() != expected.to_lowercase() {
        return Err(SecurityError::PolicyViolation {
            reason: format!(
                "Engine binary integrity failure: SHA-256 mismatch for '{}' (computed: {}, expected: {})",
                engine_path.display(),
                computed,
                expected
            ),
        });
    }

    Ok(computed)
}

/// Verifies that the byte slice conforms to the valid executable format for the target OS.
pub fn verify_executable_format(data: &[u8], target_os: &str) -> Result<(), SecurityError> {
    if data.len() < 1024 {
        return Err(SecurityError::PolicyViolation {
            reason: format!(
                "Downloaded binary is suspiciously small ({} bytes, minimum 1024 bytes)",
                data.len()
            ),
        });
    }

    match target_os {
        "macos" => {
            // Mach-O Magic Numbers:
            // 0xFEEDFACF (64-bit LE: CF FA ED FE)
            // 0xFEEDFACE (32-bit LE: CE FA ED FE)
            // 0xCAFEBABE (Universal binary BE: CA FE BA BE)
            // 0xBEBAFECA (Universal binary LE: BE BA FE CA)
            let magic = &data[0..4];
            let is_macho_64_le = magic == [0xCF, 0xFA, 0xED, 0xFE];
            let is_macho_64_be = magic == [0xFE, 0xED, 0xFA, 0xCF];
            let is_macho_32_le = magic == [0xCE, 0xFA, 0xED, 0xFE];
            let is_macho_32_be = magic == [0xFE, 0xED, 0xFA, 0xCE];
            let is_universal_be = magic == [0xCA, 0xFE, 0xBA, 0xBE];
            let is_universal_le = magic == [0xBE, 0xBA, 0xFE, 0xCA];

            if !(is_macho_64_le
                || is_macho_64_be
                || is_macho_32_le
                || is_macho_32_be
                || is_universal_be
                || is_universal_le)
            {
                return Err(SecurityError::PolicyViolation {
                    reason: format!(
                        "Invalid binary executable format: expected Mach-O binary headers, found magic bytes: {:02x?}",
                        magic
                    ),
                });
            }
        }
        "linux" => {
            // ELF Magic Number: 0x7F 'E' 'L' 'F'
            let magic = &data[0..4];
            if magic != [0x7F, b'E', b'L', b'F'] {
                return Err(SecurityError::PolicyViolation {
                    reason: format!(
                        "Invalid binary executable format: expected ELF binary header, found magic bytes: {:02x?}",
                        magic
                    ),
                });
            }
        }
        _ => {
            return Err(SecurityError::PolicyViolation {
                reason: format!("Unsupported target OS format validation: {target_os}"),
            });
        }
    }

    Ok(())
}

/// Cryptographic verifier for release manifests and metadata using Ed25519 signatures.
#[derive(Debug, Clone)]
pub struct ReleaseSignatureVerifier {
    verifying_key: VerifyingKey,
}

impl Default for ReleaseSignatureVerifier {
    fn default() -> Self {
        Self::official()
    }
}

impl ReleaseSignatureVerifier {
    /// Constructs a verifier configured with Unarc's official public signing key.
    #[must_use]
    pub fn official() -> Self {
        Self::from_public_key_hex(OFFICIAL_RELEASE_PUBLIC_KEY_HEX)
            .expect("Official embedded Ed25519 public key must be valid")
    }

    /// Constructs a verifier from an existing `VerifyingKey`.
    #[must_use]
    pub fn from_verifying_key(verifying_key: VerifyingKey) -> Self {
        Self { verifying_key }
    }

    /// Constructs a verifier from a 32-byte hex-encoded public key string.
    pub fn from_public_key_hex(hex_str: &str) -> Result<Self, SecurityError> {
        let bytes = hex_decode(hex_str).map_err(|e| SecurityError::PolicyViolation {
            reason: format!("Invalid public key hex string: {e}"),
        })?;
        if bytes.len() != 32 {
            return Err(SecurityError::PolicyViolation {
                reason: format!("Public key must be 32 bytes, got {}", bytes.len()),
            });
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        let verifying_key =
            VerifyingKey::from_bytes(&arr).map_err(|e| SecurityError::PolicyViolation {
                reason: format!("Invalid Ed25519 public key bytes: {e}"),
            })?;
        Ok(Self { verifying_key })
    }

    /// Returns the hex-encoded 32-byte public key string.
    #[must_use]
    pub fn public_key_hex(&self) -> String {
        hex_encode(&self.verifying_key.to_bytes())
    }

    /// Verifies that the message data matches the given hex-encoded 64-byte signature.
    pub fn verify(&self, message: &[u8], signature_hex: &str) -> Result<(), SecurityError> {
        let sig_bytes = hex_decode(signature_hex).map_err(|e| SecurityError::PolicyViolation {
            reason: format!("Invalid signature hex string: {e}"),
        })?;
        if sig_bytes.len() != 64 {
            return Err(SecurityError::PolicyViolation {
                reason: format!("Signature must be 64 bytes, got {}", sig_bytes.len()),
            });
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&arr);
        self.verifying_key
            .verify_strict(message, &signature)
            .map_err(|e| SecurityError::PolicyViolation {
                reason: format!("Ed25519 cryptographic signature verification failed: {e}"),
            })
    }
}

/// Canonical integrity manifest representing the local release build identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegrityManifest {
    /// Unarc binary package version.
    pub unarc_version: String,
    /// Operating system target.
    pub target_os: String,
    /// Architecture target.
    pub target_arch: String,
    /// Bundled 7zz engine version.
    pub bundled_7zz_version: String,
    /// Expected SHA-256 of bundled 7zz binary.
    pub bundled_7zz_sha256: String,
}

impl Default for IntegrityManifest {
    fn default() -> Self {
        Self::current()
    }
}

impl IntegrityManifest {
    /// Constructs the canonical integrity manifest for the running build.
    #[must_use]
    pub fn current() -> Self {
        Self {
            unarc_version: env!("CARGO_PKG_VERSION").to_string(),
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
            bundled_7zz_sha256: expected_engine_binary_sha256().to_string(),
        }
    }

    /// Verifies the current build identity against running environment constants.
    pub fn verify_current_identity(&self) -> Result<(), SecurityError> {
        if self.unarc_version != env!("CARGO_PKG_VERSION") {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Manifest version mismatch: expected {}, got {}",
                    env!("CARGO_PKG_VERSION"),
                    self.unarc_version
                ),
            });
        }
        if self.target_os != std::env::consts::OS {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Manifest target OS mismatch: expected {}, got {}",
                    std::env::consts::OS,
                    self.target_os
                ),
            });
        }
        if self.target_arch != std::env::consts::ARCH {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Manifest target architecture mismatch: expected {}, got {}",
                    std::env::consts::ARCH,
                    self.target_arch
                ),
            });
        }
        if self.bundled_7zz_version != PINNED_7ZIP_VERSION {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Manifest bundled 7zz version mismatch: expected {}, got {}",
                    PINNED_7ZIP_VERSION, self.bundled_7zz_version
                ),
            });
        }
        let expected_hash = expected_engine_binary_sha256();
        if self.bundled_7zz_sha256.to_lowercase() != expected_hash.to_lowercase() {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Manifest bundled 7zz SHA-256 mismatch: expected {}, got {}",
                    expected_hash, self.bundled_7zz_sha256
                ),
            });
        }
        Ok(())
    }
}

/// Release manifest metadata distributed with signed releases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    /// New version string.
    pub version: String,
    /// Target operating system (e.g. "macos", "linux").
    pub target_os: String,
    /// Target architecture (e.g. "aarch64", "x86_64").
    pub target_arch: String,
    /// Bundled 7-Zip engine version.
    pub bundled_7zz_version: String,
    /// Expected SHA-256 for the bundled 7zz binary.
    pub bundled_7zz_sha256: String,
    /// Relative or absolute download URL for the release artifact.
    pub artifact_url: String,
    /// SHA-256 hash of the release executable binary.
    pub artifact_sha256: String,
    /// Hex-encoded Ed25519 signature over canonical manifest bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl ReleaseManifest {
    /// Computes canonical JSON bytes of manifest for signing or signature verification.
    #[must_use]
    pub fn canonical_signing_bytes(&self) -> Vec<u8> {
        let mut clone = self.clone();
        clone.signature = None;
        serde_json::to_vec(&clone).unwrap_or_default()
    }

    /// Cryptographically signs the manifest with the provided signing key.
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let canonical = self.canonical_signing_bytes();
        let sig = signing_key.sign(&canonical);
        self.signature = Some(hex_encode(&sig.to_bytes()));
    }

    /// Verifies the signature of the manifest using the specified verifier.
    pub fn verify_signature(
        &self,
        verifier: &ReleaseSignatureVerifier,
    ) -> Result<(), SecurityError> {
        let sig_hex = self
            .signature
            .as_deref()
            .ok_or_else(|| SecurityError::PolicyViolation {
                reason: "Release manifest missing cryptographic signature".to_string(),
            })?;
        let canonical = self.canonical_signing_bytes();
        verifier.verify(&canonical, sig_hex)
    }

    /// Verifies architecture compatibility with current host operating system and architecture.
    pub fn verify_architecture_compatibility(&self) -> Result<(), SecurityError> {
        if self.target_os != std::env::consts::OS || self.target_arch != std::env::consts::ARCH {
            return Err(SecurityError::PolicyViolation {
                reason: format!(
                    "Architecture mismatch: release is built for {}-{}, but host system is {}-{}",
                    self.target_os,
                    self.target_arch,
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_encode_decode() {
        let data = b"Hello, Unarc!";
        let encoded = hex_encode(data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(data.as_slice(), decoded.as_slice());
    }

    #[test]
    fn test_sha256_bytes() {
        let hash = compute_sha256_bytes(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_canonical_manifest_identity() {
        let manifest = IntegrityManifest::current();
        assert!(manifest.verify_current_identity().is_ok());
    }

    #[test]
    fn test_ed25519_sign_and_verify() {
        let signing_key = SigningKey::from_bytes(&OFFICIAL_RELEASE_SIGNING_SEED);
        let verifier = ReleaseSignatureVerifier::from_verifying_key(signing_key.verifying_key());

        let mut manifest = ReleaseManifest {
            version: "0.3.0".to_string(),
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
            bundled_7zz_sha256: expected_engine_binary_sha256().to_string(),
            artifact_url: "unarc".to_string(),
            artifact_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            signature: None,
        };

        manifest.sign(&signing_key);
        assert!(manifest.signature.is_some());
        assert!(manifest.verify_signature(&verifier).is_ok());

        // Tamper with manifest content
        manifest.version = "0.4.0".to_string();
        assert!(manifest.verify_signature(&verifier).is_err());
    }

    #[test]
    fn test_architecture_compatibility_check() {
        let mut manifest = ReleaseManifest {
            version: "0.3.0".to_string(),
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
            bundled_7zz_sha256: expected_engine_binary_sha256().to_string(),
            artifact_url: "unarc".to_string(),
            artifact_sha256: "hash".to_string(),
            signature: None,
        };
        assert!(manifest.verify_architecture_compatibility().is_ok());

        // Mismatched arch
        manifest.target_arch = "mips64".to_string();
        assert!(manifest.verify_architecture_compatibility().is_err());
    }

    #[test]
    fn test_executable_format_validation() {
        let elf_dummy = [
            0x7F, b'E', b'L', b'F', 0x02, 0x01, 0x01, 0x00, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut elf_buf = vec![0u8; 2048];
        elf_buf[..elf_dummy.len()].copy_from_slice(&elf_dummy);

        let macho_dummy = [0xCF, 0xFA, 0xED, 0xFE];
        let mut macho_buf = vec![0u8; 2048];
        macho_buf[..macho_dummy.len()].copy_from_slice(&macho_dummy);

        assert!(verify_executable_format(&elf_buf, "linux").is_ok());
        assert!(verify_executable_format(&elf_buf, "macos").is_err());

        assert!(verify_executable_format(&macho_buf, "macos").is_ok());
        assert!(verify_executable_format(&macho_buf, "linux").is_err());
    }
}
