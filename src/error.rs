//! Error types and exit codes for Unarc.

use thiserror::Error;

/// Result alias for Unarc operations.
pub type Result<T, E = UnarcError> = std::result::Result<T, E>;

/// Top-level error enum representing all failures in Unarc.
#[derive(Debug, Error)]
pub enum UnarcError {
    /// Command-line argument or usage error.
    #[error("CLI error: {0}")]
    Cli(String),

    /// Security or policy constraint violation.
    #[error("Security violation: {0}")]
    Security(#[from] SecurityError),

    /// Archive format, engine, or password error.
    #[error("Archive error: {0}")]
    Archive(#[from] ArchiveError),

    /// Platform-specific operation error.
    #[error("Platform error: {0}")]
    Platform(#[from] PlatformError),

    /// Low-level I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl UnarcError {
    /// Returns the recommended UNIX process exit code for this error.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Cli(_) => 1,
            Self::Security(_) => 2,
            Self::Archive(_) => 3,
            Self::Platform(_) => 4,
            Self::Io(_) => 5,
        }
    }
}

/// Security and policy violation errors.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum SecurityError {
    /// Attempted path traversal (e.g. "../", parent component).
    #[error("Path traversal detected: {path}")]
    PathTraversal { path: String },

    /// Absolute path provided when only relative paths are permitted.
    #[error("Absolute path not permitted: {path}")]
    AbsolutePathNotAllowed { path: String },

    /// Target path escapes the designated base directory boundary.
    #[error("Path escapes destination boundary: {path}")]
    BoundaryEscaped { path: String },

    /// Path contains invalid, reserved, or null characters.
    #[error("Invalid path component: {details}")]
    InvalidPath { details: String },

    /// Symlink target violates security boundary or policy.
    #[error("Insecure symlink rejected: {target}")]
    InsecureSymlink { target: String },

    /// Operation violates an active security policy.
    #[error("Policy violation: {reason}")]
    PolicyViolation { reason: String },
}

/// Archive format, header, and inspection errors.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum ArchiveError {
    /// File format could not be determined or is not supported.
    #[error("Unsupported archive format: {format}")]
    UnsupportedFormat { format: String },

    /// Archive file was not found at specified path.
    #[error("Archive file not found: {path}")]
    FileNotFound { path: String },

    /// Archive header or structure is invalid/corrupt.
    #[error("Corrupt or invalid archive structure: {message}")]
    CorruptArchive { message: String },

    /// Password is required to decrypt this archive.
    #[error("Password required for encrypted archive: {path}")]
    PasswordRequired { path: String },

    /// Password provided failed decryption verification.
    #[error("Invalid password provided for archive: {path}")]
    InvalidPassword { path: String },

    /// Archive backend operation is not yet available or failed.
    #[error("Backend error ({backend}): {message}")]
    BackendFailure { backend: String, message: String },
}

/// Platform capability and environment errors.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum PlatformError {
    /// Feature is unsupported on the current operating system or architecture.
    #[error("Unsupported platform feature: {details}")]
    Unsupported { details: String },

    /// Insufficient filesystem or sandbox permissions.
    #[error("Permission denied for operation: {operation}")]
    PermissionDenied { operation: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_codes() {
        assert_eq!(UnarcError::Cli("invalid flag".into()).exit_code(), 1);
        assert_eq!(
            UnarcError::Security(SecurityError::PathTraversal {
                path: "../etc".into()
            })
            .exit_code(),
            2
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::UnsupportedFormat {
                format: "unknown".into()
            })
            .exit_code(),
            3
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::PasswordRequired {
                path: "secret.7z".into()
            })
            .exit_code(),
            3
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::InvalidPassword {
                path: "secret.7z".into()
            })
            .exit_code(),
            3
        );
        assert_eq!(
            UnarcError::Platform(PlatformError::Unsupported {
                details: "test".into()
            })
            .exit_code(),
            4
        );
        assert_eq!(
            UnarcError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "not found"
            ))
            .exit_code(),
            5
        );
    }

    #[test]
    fn test_error_display_formatting() {
        let err = SecurityError::PathTraversal {
            path: "../secret".into(),
        };
        assert_eq!(err.to_string(), "Path traversal detected: ../secret");

        let top_err: UnarcError = err.into();
        assert_eq!(
            top_err.to_string(),
            "Security violation: Path traversal detected: ../secret"
        );

        let pass_err = ArchiveError::PasswordRequired {
            path: "data.zip".into(),
        };
        assert_eq!(
            pass_err.to_string(),
            "Password required for encrypted archive: data.zip"
        );
    }
}
