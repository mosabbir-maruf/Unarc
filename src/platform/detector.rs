//! Platform detection and architecture capability interrogation.

use serde::{Deserialize, Serialize};

/// Platform runtime and target architecture information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system name (e.g. "macos", "linux").
    pub os: String,

    /// Target CPU architecture (e.g. "aarch64", "x86_64").
    pub arch: String,

    /// Indicates whether target is Apple Silicon (macOS on aarch64).
    pub is_apple_silicon: bool,

    /// Indicates whether target is macOS.
    pub is_macos: bool,

    /// Indicates whether target is Linux.
    pub is_linux: bool,

    /// Platform capability profile.
    pub capabilities: PlatformCapabilities,
}

/// Feature capabilities supported by the current platform environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    /// macOS quarantine extended attribute support (`com.apple.quarantine`).
    pub supports_quarantine_xattr: bool,

    /// Standard POSIX file mode / permission support.
    pub supports_posix_permissions: bool,

    /// Sandbox confinement integration capability.
    pub supports_sandbox_confinement: bool,
}

impl PlatformInfo {
    /// Detects current compile-time and runtime target platform information.
    #[must_use]
    pub fn current() -> Self {
        let is_macos = cfg!(target_os = "macos");
        let is_linux = cfg!(target_os = "linux");
        let is_aarch64 = cfg!(target_arch = "aarch64");
        let is_apple_silicon = is_macos && is_aarch64;

        let capabilities = PlatformCapabilities {
            supports_quarantine_xattr: is_macos,
            supports_posix_permissions: is_macos || is_linux,
            supports_sandbox_confinement: is_macos || is_linux,
        };

        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            is_apple_silicon,
            is_macos,
            is_linux,
            capabilities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_info_current() {
        let info = PlatformInfo::current();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());

        if info.os == "macos" && info.arch == "aarch64" {
            assert!(info.is_apple_silicon);
            assert!(info.is_macos);
            assert!(info.capabilities.supports_quarantine_xattr);
        } else if info.os == "linux" {
            assert!(info.is_linux);
            assert!(!info.capabilities.supports_quarantine_xattr);
        }
    }
}
