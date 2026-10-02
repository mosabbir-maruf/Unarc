//! Linux-specific environment and container attributes.

/// Container environment indicator file.
pub const CONTAINER_ENV_PATH: &str = "/.dockerenv";

/// Checks if running within a containerized Linux environment.
#[must_use]
pub fn is_container_environment() -> bool {
    std::path::Path::new(CONTAINER_ENV_PATH).exists()
}
