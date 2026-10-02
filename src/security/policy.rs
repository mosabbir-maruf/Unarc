//! Security policy definition and enforcement.

use super::path::{
    sanitize_relative_path, verify_boundary_containment, DEFAULT_MAX_PATH_DEPTH,
    DEFAULT_MAX_PATH_LENGTH,
};
use crate::error::SecurityError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Security configuration governing archive processing and path validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Whether absolute paths are allowed in archive entries. Always false by default.
    pub allow_absolute_paths: bool,

    /// Whether symlinks are permitted. Always false by default in strict mode.
    pub allow_symlinks: bool,

    /// Maximum permissible directory nesting depth.
    pub max_path_depth: usize,

    /// Maximum permissible path length in bytes.
    pub max_path_length: usize,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

impl SecurityPolicy {
    /// Constructs a strict, zero-trust security policy.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            allow_absolute_paths: false,
            allow_symlinks: false,
            max_path_depth: DEFAULT_MAX_PATH_DEPTH,
            max_path_length: DEFAULT_MAX_PATH_LENGTH,
        }
    }

    /// Validates an entry path extracted from or contained within an archive.
    pub fn validate_entry_path(&self, path: &Path) -> Result<PathBuf, SecurityError> {
        if !self.allow_absolute_paths && path.is_absolute() {
            return Err(SecurityError::AbsolutePathNotAllowed {
                path: path.to_string_lossy().to_string(),
            });
        }

        sanitize_relative_path(path, self.max_path_depth)
    }

    /// Validates a symlink target against security policies.
    pub fn validate_symlink_target(&self, target: &Path) -> Result<(), SecurityError> {
        if !self.allow_symlinks {
            return Err(SecurityError::InsecureSymlink {
                target: target.to_string_lossy().to_string(),
            });
        }

        if target.is_absolute() {
            return Err(SecurityError::InsecureSymlink {
                target: format!(
                    "Absolute symlink targets are forbidden: {}",
                    target.to_string_lossy()
                ),
            });
        }

        // Relative symlink must not traverse outside containment
        sanitize_relative_path(target, self.max_path_depth)?;
        Ok(())
    }

    /// Validates that an extraction destination target remains safely bounded.
    pub fn validate_destination(
        &self,
        base_dir: &Path,
        rel_path: &Path,
    ) -> Result<PathBuf, SecurityError> {
        verify_boundary_containment(base_dir, rel_path, self.max_path_depth)
    }
}

/// Runtime security context tracking validation metrics and policy state.
#[derive(Debug, Clone)]
pub struct SecurityContext {
    policy: SecurityPolicy,
}

impl SecurityContext {
    /// Creates a new security context with the given policy.
    #[must_use]
    pub fn new(policy: SecurityPolicy) -> Self {
        Self { policy }
    }

    /// Returns a reference to the active security policy.
    #[must_use]
    pub fn policy(&self) -> &SecurityPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_policy_rejects_symlinks() {
        let policy = SecurityPolicy::strict();
        let res = policy.validate_symlink_target(Path::new("target.txt"));
        assert!(matches!(res, Err(SecurityError::InsecureSymlink { .. })));
    }

    #[test]
    fn test_policy_with_symlinks_enabled() {
        let mut policy = SecurityPolicy::strict();
        policy.allow_symlinks = true;

        assert!(policy
            .validate_symlink_target(Path::new("target.txt"))
            .is_ok());

        // Still rejects absolute symlink
        assert!(policy
            .validate_symlink_target(Path::new("/etc/hosts"))
            .is_err());
    }

    #[test]
    fn test_security_context_initialization() {
        let ctx = SecurityContext::new(SecurityPolicy::strict());
        assert!(!ctx.policy().allow_symlinks);
        assert!(!ctx.policy().allow_absolute_paths);
    }
}
