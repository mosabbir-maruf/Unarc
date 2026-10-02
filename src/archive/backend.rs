//! Archive backend abstraction and contract definition.

use super::format::ArchiveFormat;
use super::metadata::{ArchiveEntry, ArchiveMetadata};
use crate::error::ArchiveError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Result from an archive integrity test operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveTestResult {
    /// Path of the tested archive.
    pub path: PathBuf,

    /// Archive format.
    pub format: ArchiveFormat,

    /// Whether integrity test passed.
    pub passed: bool,

    /// Number of items verified, if reported.
    pub entries_checked: Option<usize>,

    /// Informational or status message.
    pub message: String,
}

/// Result from an archive extraction operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveExtractResult {
    /// Source archive path.
    pub archive_path: PathBuf,

    /// Target destination directory.
    pub destination: PathBuf,

    /// Detected format.
    pub format: ArchiveFormat,

    /// Total entries successfully extracted, if known.
    pub entries_extracted: Option<usize>,

    /// Total uncompressed bytes extracted, if known.
    pub total_bytes_extracted: Option<u64>,
}

/// Listener receiving real-time progress events from archive operations.
pub trait ProgressListener: Send {
    /// Invoked whenever the progress percentage or current file updates.
    fn on_progress(&mut self, percentage: u8, current_file: Option<&str>);
}

/// Contract for archive engine backends (e.g. bundled 7zz).
pub trait ArchiveBackend: Send + Sync {
    /// Identifier name of the backend engine.
    fn name(&self) -> &'static str;

    /// Formats supported by this backend.
    fn supported_formats(&self) -> &'static [ArchiveFormat];

    /// Inspects an archive to retrieve top-level metadata.
    fn inspect(&self, path: &Path, password: Option<&str>)
        -> Result<ArchiveMetadata, ArchiveError>;

    /// Lists entries contained within the archive.
    fn list_entries(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Vec<ArchiveEntry>, ArchiveError>;

    /// Tests the integrity of an archive without writing to disk.
    fn test(&self, path: &Path, password: Option<&str>) -> Result<ArchiveTestResult, ArchiveError>;

    /// Extracts an archive into the target destination directory.
    fn extract(
        &self,
        path: &Path,
        destination: &Path,
        password: Option<&str>,
    ) -> Result<ArchiveExtractResult, ArchiveError>;

    /// Returns whether this backend engine binary is present and operational.
    fn is_available(&self) -> bool;
}
