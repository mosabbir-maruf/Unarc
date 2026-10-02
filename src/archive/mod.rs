//! Archive abstraction layer for format detection and metadata inspection.

pub mod backend;
pub mod format;
pub mod metadata;

pub use backend::ArchiveBackend;
pub use format::ArchiveFormat;
pub use metadata::{ArchiveEntry, ArchiveMetadata};
