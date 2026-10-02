//! Archive abstraction layer for format detection and metadata inspection.

pub mod backend;
pub mod bundled;
pub mod format;
pub mod metadata;
pub mod volume;

pub use backend::{ArchiveBackend, ArchiveExtractResult, ArchiveTestResult};
pub use bundled::{
    resolve_bundled_engine, SevenZipBackend, EXPECTED_SHA256_LINUX_ARM64,
    EXPECTED_SHA256_LINUX_X64, EXPECTED_SHA256_MACOS, PINNED_7ZIP_RELEASE_URL, PINNED_7ZIP_VERSION,
};
pub use format::ArchiveFormat;
pub use metadata::{ArchiveEntry, ArchiveMetadata};
pub use volume::{VolumeResolver, VolumeSet};
