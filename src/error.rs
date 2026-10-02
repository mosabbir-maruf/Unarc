//! Error types, error codes, and stable exit codes for Unarc.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Result alias for Unarc operations.
pub type Result<T, E = UnarcError> = std::result::Result<T, E>;

/// Structured failure class identifiers for deterministic, script-friendly consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Target input archive path does not exist.
    InputNotFound,
    /// Target input archive path exists but is not a regular file (e.g. directory, socket).
    InputNotFile,
    /// Archive file format is unknown or unsupported.
    UnsupportedFormat,
    /// A required volume of a split/multipart archive was not found.
    MissingVolume,
    /// An archive volume exists but is corrupt, mismatched, or invalid.
    InvalidVolume,
    /// Archive header, structure, or data integrity check failed.
    CorruptArchive,
    /// Password required to inspect or decrypt this archive.
    PasswordRequired,
    /// Provided password was incorrect or decryption failed.
    InvalidPassword,
    /// Output destination path is invalid, exists as a file, or cannot be accessed.
    OutputInvalid,
    /// Path traversal attack attempt detected (e.g. `../` or root breakout).
    PathTraversal,
    /// Archive contains unsafe entry (insecure symlink, hardlink, device node, FIFO).
    UnsafeEntry,
    /// Execution or operation violates active security policy or sandbox requirements.
    SecurityPolicyViolation,
    /// Filesystem permission denied on input archive or output destination.
    PermissionDenied,
    /// Archive extraction failed during decompression or writing.
    ExtractionFailed,
    /// Bundled archive engine crashed, failed to spawn, or reported engine fault.
    EngineFailed,
    /// Operation interrupted by OS signal (SIGINT or SIGTERM).
    Interrupted,
    /// CLI syntax, argument parsing, or usage error.
    CliError,
}

impl ErrorCode {
    /// Returns the uppercase machine-readable error string identifier.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InputNotFound => "INPUT_NOT_FOUND",
            Self::InputNotFile => "INPUT_NOT_FILE",
            Self::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            Self::MissingVolume => "MISSING_VOLUME",
            Self::InvalidVolume => "INVALID_VOLUME",
            Self::CorruptArchive => "CORRUPT_ARCHIVE",
            Self::PasswordRequired => "PASSWORD_REQUIRED",
            Self::InvalidPassword => "INVALID_PASSWORD",
            Self::OutputInvalid => "OUTPUT_INVALID",
            Self::PathTraversal => "PATH_TRAVERSAL",
            Self::UnsafeEntry => "UNSAFE_ENTRY",
            Self::SecurityPolicyViolation => "SECURITY_POLICY_VIOLATION",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ExtractionFailed => "EXTRACTION_FAILED",
            Self::EngineFailed => "ENGINE_FAILED",
            Self::Interrupted => "INTERRUPTED",
            Self::CliError => "CLI_ERROR",
        }
    }

    /// Returns the stable UNIX process exit code for this failure class.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InputNotFound => 10,
            Self::InputNotFile => 11,
            Self::UnsupportedFormat => 12,
            Self::MissingVolume => 13,
            Self::InvalidVolume => 14,
            Self::CorruptArchive => 15,
            Self::PasswordRequired => 16,
            Self::InvalidPassword => 17,
            Self::OutputInvalid => 18,
            Self::PathTraversal => 20,
            Self::UnsafeEntry => 21,
            Self::SecurityPolicyViolation => 22,
            Self::PermissionDenied => 30,
            Self::ExtractionFailed => 40,
            Self::EngineFailed => 41,
            Self::Interrupted => 130,
            Self::CliError => 2,
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Top-level error enum representing all failures in Unarc.
#[derive(Debug, Error)]
pub enum UnarcError {
    /// Command-line argument or usage error.
    #[error("CLI error: {0}")]
    Cli(String),

    /// Process execution interrupted by signal.
    #[error("Operation interrupted by signal")]
    Interrupted,

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
    /// Returns the structured error code classification.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Cli(_) => ErrorCode::CliError,
            Self::Interrupted => ErrorCode::Interrupted,
            Self::Security(sec) => match sec {
                SecurityError::PathTraversal { .. } | SecurityError::BoundaryEscaped { .. } => {
                    ErrorCode::PathTraversal
                }
                SecurityError::InsecureSymlink { .. } | SecurityError::UnsafeEntry { .. } => {
                    ErrorCode::UnsafeEntry
                }
                SecurityError::AbsolutePathNotAllowed { .. }
                | SecurityError::PolicyViolation { .. } => ErrorCode::SecurityPolicyViolation,
                SecurityError::InvalidPath { .. } => ErrorCode::OutputInvalid,
                SecurityError::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            },
            Self::Archive(arch) => match arch {
                ArchiveError::FileNotFound { .. } => ErrorCode::InputNotFound,
                ArchiveError::InputNotFile { .. } => ErrorCode::InputNotFile,
                ArchiveError::UnsupportedFormat { .. } => ErrorCode::UnsupportedFormat,
                ArchiveError::MissingVolume { .. } => ErrorCode::MissingVolume,
                ArchiveError::InvalidVolume { .. } => ErrorCode::InvalidVolume,
                ArchiveError::CorruptArchive { .. } => ErrorCode::CorruptArchive,
                ArchiveError::PasswordRequired { .. } => ErrorCode::PasswordRequired,
                ArchiveError::InvalidPassword { .. } => ErrorCode::InvalidPassword,
                ArchiveError::OutputInvalid { .. } => ErrorCode::OutputInvalid,
                ArchiveError::ExtractionFailed { .. } => ErrorCode::ExtractionFailed,
                ArchiveError::EngineFailed { .. } | ArchiveError::BackendFailure { .. } => {
                    ErrorCode::EngineFailed
                }
                ArchiveError::PermissionDenied { .. } => ErrorCode::PermissionDenied,
            },
            Self::Platform(plat) => match plat {
                PlatformError::PermissionDenied { .. } => ErrorCode::PermissionDenied,
                PlatformError::Unsupported { .. } => ErrorCode::SecurityPolicyViolation,
            },
            Self::Io(io) => match io.kind() {
                std::io::ErrorKind::NotFound => ErrorCode::InputNotFound,
                std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
                std::io::ErrorKind::Interrupted => ErrorCode::Interrupted,
                std::io::ErrorKind::AlreadyExists => ErrorCode::OutputInvalid,
                _ => ErrorCode::ExtractionFailed,
            },
        }
    }

    /// Returns the stable UNIX process exit code for this error.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        self.code().exit_code()
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

    /// Unsafe entry rejected (special filesystem nodes, hardlink breakout, etc.).
    #[error("Unsafe archive entry: {details}")]
    UnsafeEntry { details: String },

    /// Operation denied due to insufficient permissions.
    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },

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

    /// Target input exists but is not a regular file.
    #[error("Input path is not a regular file: {path} ({reason})")]
    InputNotFile { path: String, reason: String },

    /// Archive header or structure is invalid/corrupt.
    #[error("Corrupt or invalid archive structure: {message}")]
    CorruptArchive { message: String },

    /// Password is required to decrypt this archive.
    #[error("Password required for encrypted archive: {path}")]
    PasswordRequired { path: String },

    /// Password provided failed decryption verification.
    #[error("Invalid password provided for archive: {path}")]
    InvalidPassword { path: String },

    /// Output destination path is invalid or cannot be initialized.
    #[error("Invalid output destination: {path} ({reason})")]
    OutputInvalid { path: String, reason: String },

    /// Archive extraction failed.
    #[error("Extraction failed: {message}")]
    ExtractionFailed { message: String },

    /// Engine binary or execution failure.
    #[error("Engine failure ({engine}): {message}")]
    EngineFailed { engine: String, message: String },

    /// Archive backend operation is not yet available or failed.
    #[error("Backend error ({backend}): {message}")]
    BackendFailure { backend: String, message: String },

    /// Permission denied when accessing archive or destination.
    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    /// A required volume for a multipart archive is missing.
    #[error("MISSING_VOLUME: required volume '{expected}' not found ({details})")]
    MissingVolume { expected: String, details: String },

    /// An archive volume is invalid, mismatched, or corrupted.
    #[error("INVALID_VOLUME: volume '{path}' is invalid ({reason})")]
    InvalidVolume { path: String, reason: String },
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
    fn test_all_16_exit_codes() {
        // Verify all 16 specified exit codes
        assert_eq!(ErrorCode::InputNotFound.exit_code(), 10);
        assert_eq!(ErrorCode::InputNotFile.exit_code(), 11);
        assert_eq!(ErrorCode::UnsupportedFormat.exit_code(), 12);
        assert_eq!(ErrorCode::MissingVolume.exit_code(), 13);
        assert_eq!(ErrorCode::InvalidVolume.exit_code(), 14);
        assert_eq!(ErrorCode::CorruptArchive.exit_code(), 15);
        assert_eq!(ErrorCode::PasswordRequired.exit_code(), 16);
        assert_eq!(ErrorCode::InvalidPassword.exit_code(), 17);
        assert_eq!(ErrorCode::OutputInvalid.exit_code(), 18);
        assert_eq!(ErrorCode::PathTraversal.exit_code(), 20);
        assert_eq!(ErrorCode::UnsafeEntry.exit_code(), 21);
        assert_eq!(ErrorCode::SecurityPolicyViolation.exit_code(), 22);
        assert_eq!(ErrorCode::PermissionDenied.exit_code(), 30);
        assert_eq!(ErrorCode::ExtractionFailed.exit_code(), 40);
        assert_eq!(ErrorCode::EngineFailed.exit_code(), 41);
        assert_eq!(ErrorCode::Interrupted.exit_code(), 130);
    }

    #[test]
    fn test_error_code_mapping() {
        assert_eq!(
            UnarcError::Archive(ArchiveError::FileNotFound {
                path: "test.zip".into()
            })
            .code(),
            ErrorCode::InputNotFound
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::InputNotFile {
                path: "/tmp".into(),
                reason: "directory".into(),
            })
            .code(),
            ErrorCode::InputNotFile
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::UnsupportedFormat {
                format: "unknown".into()
            })
            .code(),
            ErrorCode::UnsupportedFormat
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::MissingVolume {
                expected: "vol2".into(),
                details: "gap".into(),
            })
            .code(),
            ErrorCode::MissingVolume
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::InvalidVolume {
                path: "vol1".into(),
                reason: "bad header".into(),
            })
            .code(),
            ErrorCode::InvalidVolume
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::CorruptArchive {
                message: "crc error".into()
            })
            .code(),
            ErrorCode::CorruptArchive
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::PasswordRequired {
                path: "enc.rar".into()
            })
            .code(),
            ErrorCode::PasswordRequired
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::InvalidPassword {
                path: "enc.rar".into()
            })
            .code(),
            ErrorCode::InvalidPassword
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::OutputInvalid {
                path: "/out".into(),
                reason: "is a file".into(),
            })
            .code(),
            ErrorCode::OutputInvalid
        );
        assert_eq!(
            UnarcError::Security(SecurityError::PathTraversal {
                path: "../etc".into()
            })
            .code(),
            ErrorCode::PathTraversal
        );
        assert_eq!(
            UnarcError::Security(SecurityError::UnsafeEntry {
                details: "fifo".into()
            })
            .code(),
            ErrorCode::UnsafeEntry
        );
        assert_eq!(
            UnarcError::Security(SecurityError::InsecureSymlink {
                target: "/etc/passwd".into()
            })
            .code(),
            ErrorCode::UnsafeEntry
        );
        assert_eq!(
            UnarcError::Security(SecurityError::PolicyViolation {
                reason: "forbidden".into()
            })
            .code(),
            ErrorCode::SecurityPolicyViolation
        );
        assert_eq!(
            UnarcError::Platform(PlatformError::PermissionDenied {
                operation: "read".into()
            })
            .code(),
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::ExtractionFailed {
                message: "write error".into()
            })
            .code(),
            ErrorCode::ExtractionFailed
        );
        assert_eq!(
            UnarcError::Archive(ArchiveError::EngineFailed {
                engine: "7zz".into(),
                message: "segfault".into(),
            })
            .code(),
            ErrorCode::EngineFailed
        );
        assert_eq!(UnarcError::Interrupted.code(), ErrorCode::Interrupted);
        assert_eq!(UnarcError::Interrupted.exit_code(), 130);
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

        let missing_vol = ArchiveError::MissingVolume {
            expected: "archive.part2.rar".into(),
            details: "gap detected".into(),
        };
        assert_eq!(
            missing_vol.to_string(),
            "MISSING_VOLUME: required volume 'archive.part2.rar' not found (gap detected)"
        );

        let invalid_vol = ArchiveError::InvalidVolume {
            path: "archive.part1.rar".into(),
            reason: "corrupted header".into(),
        };
        assert_eq!(
            invalid_vol.to_string(),
            "INVALID_VOLUME: volume 'archive.part1.rar' is invalid (corrupted header)"
        );
    }
}
