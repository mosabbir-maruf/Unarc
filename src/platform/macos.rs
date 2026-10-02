//! macOS-specific security and filesystem attributes.

/// Extended attribute key for Apple Quarantine.
pub const APPLE_QUARANTINE_XATTR: &str = "com.apple.quarantine";

/// Extended attribute key for legacy Apple Resource Forks.
pub const APPLE_RESOURCE_FORK_XATTR: &str = "com.apple.ResourceFork";

/// Apple Silicon CPU architecture identifier.
pub const APPLE_SILICON_ARCH: &str = "aarch64";
