//! Platform abstraction and environment detection.

pub mod detector;
pub mod linux;
pub mod macos;

pub use detector::{PlatformCapabilities, PlatformInfo};
