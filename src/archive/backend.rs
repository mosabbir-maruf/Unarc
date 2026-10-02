//! Archive backend abstraction and extension point.

use super::format::ArchiveFormat;
use super::metadata::{ArchiveEntry, ArchiveMetadata};
use crate::error::ArchiveError;
use std::path::Path;

/// Contract for archive engine backends (e.g. native, 7zz, libarchive).
///
/// Phase 1 defines this abstraction to guarantee clean integration for Phase 2 engines
/// without coupling CLI logic to underlying decompressor binaries.
pub trait ArchiveBackend: Send + Sync {
    /// Identifier name of the backend engine (e.g., "7zz-cli", "native-zip").
    fn name(&self) -> &'static str;

    /// Formats supported by this specific backend.
    fn supported_formats(&self) -> &'static [ArchiveFormat];

    /// Inspects an archive file to retrieve top-level metadata.
    fn inspect(&self, path: &Path) -> Result<ArchiveMetadata, ArchiveError>;

    /// Lists entries contained within the archive.
    fn list_entries(&self, path: &Path) -> Result<Vec<ArchiveEntry>, ArchiveError>;

    /// Returns whether this backend is available and functional in the current environment.
    fn is_available(&self) -> bool {
        true
    }
}
