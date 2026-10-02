//! Security and policy enforcement subsystem for Unarc.

pub mod path;
pub mod policy;
pub mod sandbox;

pub use path::{sanitize_relative_path, verify_boundary_containment};
pub use policy::{SecurityContext, SecurityPolicy};
pub use sandbox::{
    terminate_active_subprocess, ChildProcessGuard, ProcessSandboxPolicy, SandboxRunner,
    SandboxStatus, ScratchWorkspace,
};
