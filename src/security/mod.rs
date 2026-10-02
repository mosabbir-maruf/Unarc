pub mod integrity;
pub mod path;
pub mod policy;
pub mod sandbox;

pub use integrity::{
    compute_sha256, compute_sha256_bytes, expected_engine_binary_sha256,
    verify_bundled_engine_integrity, verify_executable_format, IntegrityManifest, ReleaseManifest,
    ReleaseSignatureVerifier, EXPECTED_ENGINE_BINARY_SHA256_LINUX_ARM64,
    EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64, EXPECTED_ENGINE_BINARY_SHA256_MACOS,
};
pub use path::{sanitize_relative_path, verify_boundary_containment};
pub use policy::{SecurityContext, SecurityPolicy};
pub use sandbox::{
    terminate_active_subprocess, ChildProcessGuard, ProcessSandboxPolicy, SandboxRunner,
    SandboxStatus, ScratchWorkspace,
};
