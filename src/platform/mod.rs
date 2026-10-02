//! Platform abstraction and environment detection.

pub mod detector;
pub mod linux;
pub mod macos;
pub mod signals;

pub use detector::{PlatformCapabilities, PlatformInfo};
pub use signals::{check_interrupted, is_interrupted, reset_interrupted, set_interrupted};
