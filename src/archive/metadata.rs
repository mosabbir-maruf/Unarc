//! Metadata structures representing archives and contained entries.

use super::format::ArchiveFormat;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata describing an individual entry within an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// Relative path of the entry inside the archive.
    pub path: PathBuf,

    /// Uncompressed file size in bytes.
    pub uncompressed_size: u64,

    /// Compressed size in bytes, if known by the container format.
    pub compressed_size: Option<u64>,

    /// Indicates whether the entry represents a directory.
    pub is_directory: bool,

    /// Indicates whether the entry represents a symbolic link.
    pub is_symlink: bool,

    /// Target path if this entry is a symbolic link.
    pub symlink_target: Option<PathBuf>,
}

/// High-level metadata of an inspected archive file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    /// Detected archive format.
    pub format: ArchiveFormat,

    /// Size of the archive file on disk in bytes.
    pub file_size_bytes: u64,

    /// Total number of contained entries, if resolved during inspection.
    pub entries_count: Option<usize>,

    /// Total aggregate uncompressed size in bytes, if known.
    pub total_uncompressed_bytes: Option<u64>,

    /// Indicates whether the archive requires a password / is encrypted.
    pub is_encrypted: bool,

    /// Indicates whether the archive utilizes solid compression (e.g., 7z/rar).
    pub is_solid: bool,
}

impl ArchiveMetadata {
    /// Creates basic metadata initialized from file size and format.
    #[must_use]
    pub fn new(format: ArchiveFormat, file_size_bytes: u64) -> Self {
        Self {
            format,
            file_size_bytes,
            entries_count: None,
            total_uncompressed_bytes: None,
            is_encrypted: false,
            is_solid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_metadata_creation() {
        let meta = ArchiveMetadata::new(ArchiveFormat::Zip, 1024);
        assert_eq!(meta.format, ArchiveFormat::Zip);
        assert_eq!(meta.file_size_bytes, 1024);
        assert_eq!(meta.entries_count, None);
        assert!(!meta.is_encrypted);
    }

    #[test]
    fn test_archive_entry_properties() {
        let entry = ArchiveEntry {
            path: PathBuf::from("docs/readme.txt"),
            uncompressed_size: 200,
            compressed_size: Some(85),
            is_directory: false,
            is_symlink: false,
            symlink_target: None,
        };

        assert_eq!(entry.uncompressed_size, 200);
        assert!(!entry.is_directory);
    }
}
