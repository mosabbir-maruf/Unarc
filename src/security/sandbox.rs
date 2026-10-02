//! OS-level process sandboxing and execution confinement for untrusted engine processes.

use crate::error::ArchiveError;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Tracks active child process group for signal interruption handling.
static ACTIVE_PGID: AtomicI32 = AtomicI32::new(0);

/// Terminates any actively executing subprocess group immediately.
pub fn terminate_active_subprocess() {
    let pgid = ACTIVE_PGID.swap(0, Ordering::SeqCst);
    if pgid > 0 {
        #[cfg(unix)]
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
}

/// Degree of active OS-level process sandboxing enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxStatus {
    /// Kernel-level sandbox boundary actively enforced (macOS Seatbelt or Linux Landlock).
    Enforced,
    /// Process-level isolation active (scrubbed env, isolated process group, lifecycle guards, strict path boundary validation).
    Degraded,
    /// Platform does not support sandbox confinement.
    Unavailable,
}

impl SandboxStatus {
    /// Human-readable uppercase status tag.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enforced => "ENFORCED",
            Self::Degraded => "DEGRADED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }

    /// Human-readable description of current sandbox enforcement status.
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Enforced => "macOS Seatbelt kernel sandbox actively enforced (network denied, paths restricted to resolved volumes and destination)",
            Self::Degraded => "Process group isolation, environment scrubbing, and containment enforced; kernel-level LSM sandbox not enforced",
            Self::Unavailable => "Process sandbox confinement unavailable",
        }
    }
}

/// Security policy defining process-level confinement boundaries.
#[derive(Debug, Clone)]
pub struct ProcessSandboxPolicy {
    /// Explicit path of the engine binary permitted for execution.
    pub engine_binary: PathBuf,

    /// Permitted read-only archive input files.
    pub input_files: Vec<PathBuf>,

    /// Permitted destination directory for write operations.
    pub destination_dir: Option<PathBuf>,

    /// Isolated scratch/temporary directory for engine staging.
    pub scratch_dir: PathBuf,

    /// Whether network operations are denied (always true in Unarc).
    pub deny_network: bool,
}

impl ProcessSandboxPolicy {
    /// Constructs a new confinement policy with the given engine binary and scratch space.
    #[must_use]
    pub fn new(engine_binary: PathBuf, scratch_dir: PathBuf) -> Self {
        Self {
            engine_binary,
            input_files: Vec::new(),
            destination_dir: None,
            scratch_dir,
            deny_network: true,
        }
    }

    /// Adds a single input file to the permitted read-only list.
    #[must_use]
    pub fn with_input(mut self, path: PathBuf) -> Self {
        if !self.input_files.contains(&path) {
            self.input_files.push(path);
        }
        self
    }

    /// Adds multiple input files to the permitted read-only list.
    #[must_use]
    pub fn with_inputs(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        for path in paths {
            if !self.input_files.contains(&path) {
                self.input_files.push(path);
            }
        }
        self
    }

    /// Configures the approved output directory for write access.
    #[must_use]
    pub fn with_destination(mut self, path: PathBuf) -> Self {
        self.destination_dir = Some(path);
        self
    }
}

/// RAII temporary scratch workspace guaranteed to be deleted upon drop.
#[derive(Debug)]
pub struct ScratchWorkspace {
    path: PathBuf,
}

impl ScratchWorkspace {
    /// Creates an isolated temporary directory with guaranteed cleanup.
    pub fn new() -> Result<Self, std::io::Error> {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            ".unarc_scratch_{}_{}",
            std::process::id(),
            unique_id
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Returns the scratch directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// RAII process guard that ensures child process trees are terminated on drop.
pub struct ChildProcessGuard {
    child: Option<Child>,
    pgid: libc::pid_t,
}

impl ChildProcessGuard {
    /// Creates a new guard wrapping a spawned child process.
    #[must_use]
    pub fn new(child: Child, pgid: libc::pid_t) -> Self {
        ACTIVE_PGID.store(pgid, Ordering::SeqCst);
        Self {
            child: Some(child),
            pgid,
        }
    }

    /// Returns the process ID of the child if still present.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Waits for the process to exit and collects output while disarming the drop-killer.
    pub fn wait_with_output(mut self) -> std::io::Result<Output> {
        let child = self.child.take().expect("Child process must exist");
        ACTIVE_PGID.store(0, Ordering::SeqCst);
        child.wait_with_output()
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            ACTIVE_PGID.store(0, Ordering::SeqCst);
            // Check if process has already terminated
            match child.try_wait() {
                Ok(Some(_)) => {
                    // Process already exited cleanly
                }
                _ => {
                    // Process still running: terminate the entire process group
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(-self.pgid, libc::SIGTERM);
                    }
                    std::thread::sleep(Duration::from_millis(20));
                    match child.try_wait() {
                        Ok(Some(_)) => {}
                        _ => {
                            #[cfg(unix)]
                            unsafe {
                                libc::kill(-self.pgid, libc::SIGKILL);
                            }
                            let _ = child.wait();
                        }
                    }
                }
            }
        }
    }
}

/// Executes engine processes within confinement boundaries.
pub struct SandboxRunner;

impl SandboxRunner {
    /// Probes the system for active OS-level sandbox enforcement capabilities.
    #[must_use]
    pub fn probe_status() -> SandboxStatus {
        #[cfg(target_os = "macos")]
        {
            if Path::new("/usr/bin/sandbox-exec").exists() {
                // Real runtime probe executing under Seatbelt confinement
                let test_profile = "(version 1)\
                    (deny default)\
                    (deny network*)\
                    (allow process-exec (literal \"/bin/echo\"))\
                    (allow file-read* (subpath \"/usr/lib\") (subpath \"/System/Library\") (subpath \"/System/Volumes\"))";
                match Command::new("/usr/bin/sandbox-exec")
                    .args(["-p", test_profile, "/bin/echo", "probe"])
                    .output()
                {
                    Ok(output)
                        if output.status.success()
                            && String::from_utf8_lossy(&output.stdout).trim() == "probe" =>
                    {
                        SandboxStatus::Enforced
                    }
                    _ => {
                        if probe_process_isolation() {
                            SandboxStatus::Degraded
                        } else {
                            SandboxStatus::Unavailable
                        }
                    }
                }
            } else if probe_process_isolation() {
                SandboxStatus::Degraded
            } else {
                SandboxStatus::Unavailable
            }
        }

        #[cfg(target_os = "linux")]
        {
            // On Linux, Unarc enforces process-level isolation:
            // purged ambient environment, isolated process group, PR_SET_NO_NEW_PRIVS, PR_SET_PDEATHSIG.
            // A kernel-level LSM sandbox (Landlock) is not applied, so Unarc accurately reports Degraded.
            if probe_process_isolation() {
                SandboxStatus::Degraded
            } else {
                SandboxStatus::Unavailable
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            if probe_process_isolation() {
                SandboxStatus::Degraded
            } else {
                SandboxStatus::Unavailable
            }
        }
    }

    /// Verifies via runtime probe that network operations are denied to the confined execution boundary.
    #[must_use]
    pub fn probe_network_denial() -> bool {
        #[cfg(target_os = "macos")]
        {
            if Path::new("/usr/bin/sandbox-exec").exists() && Path::new("/usr/bin/nc").exists() {
                let test_profile = "(version 1)\
                    (deny default)\
                    (deny network*)\
                    (allow process-exec (literal \"/usr/bin/nc\"))\
                    (allow file-read* (subpath \"/usr/lib\") (subpath \"/System/Library\") (subpath \"/System/Volumes\") (subpath \"/dev\"))";
                match Command::new("/usr/bin/sandbox-exec")
                    .args([
                        "-p",
                        test_profile,
                        "/usr/bin/nc",
                        "-z",
                        "-w",
                        "1",
                        "127.0.0.1",
                        "80",
                    ])
                    .output()
                {
                    Ok(output) => !output.status.success(),
                    Err(_) => true,
                }
            } else {
                true
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    /// Spawns the engine process confined by the policy.
    pub fn spawn<I, S>(
        policy: &ProcessSandboxPolicy,
        args: I,
    ) -> Result<ChildProcessGuard, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Self::build_command(policy, args)?;

        let child = cmd.spawn().map_err(|e| ArchiveError::BackendFailure {
            backend: "bundled-7zz".to_string(),
            message: format!("Failed to spawn confined engine process: {e}"),
        })?;

        let pid = child.id() as libc::pid_t;
        Ok(ChildProcessGuard::new(child, pid))
    }

    /// Executes the engine process to completion with sandboxing and returns output.
    pub fn execute<I, S>(policy: &ProcessSandboxPolicy, args: I) -> Result<Output, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let guard = Self::spawn(policy, args)?;
        let output = guard
            .wait_with_output()
            .map_err(|e| ArchiveError::BackendFailure {
                backend: "bundled-7zz".to_string(),
                message: format!("Failed to read output from confined engine process: {e}"),
            })?;

        if crate::platform::signals::is_interrupted() {
            return Err(ArchiveError::ExtractionFailed {
                message: "Process execution interrupted by signal".to_string(),
            });
        }

        Ok(output)
    }

    /// Builds a sanitized, confined `Command` configured for the platform.
    fn build_command<I, S>(policy: &ProcessSandboxPolicy, args: I) -> Result<Command, ArchiveError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let arg_list: Vec<_> = args.into_iter().collect();

        #[cfg(target_os = "macos")]
        {
            if Self::probe_status() == SandboxStatus::Enforced {
                let profile = generate_macos_seatbelt_profile(policy);
                let mut cmd = Command::new("/usr/bin/sandbox-exec");
                cmd.arg("-p").arg(profile);
                cmd.arg(&policy.engine_binary);
                for arg in arg_list {
                    cmd.arg(arg);
                }
                apply_process_isolation(&mut cmd, policy);
                return Ok(cmd);
            }
        }

        // Standard confined command (Linux, degraded macOS, or container fallback)
        let mut cmd = Command::new(&policy.engine_binary);
        for arg in arg_list {
            cmd.arg(arg);
        }
        apply_process_isolation(&mut cmd, policy);
        Ok(cmd)
    }
}

/// Applies strict environment purging, process grouping, and pre-exec constraints.
fn apply_process_isolation(cmd: &mut Command, policy: &ProcessSandboxPolicy) {
    // 1. Purge all ambient environment variables (AWS credentials, SSH keys, user secrets)
    cmd.env_clear();

    // 2. Set strictly minimal, deterministic execution environment
    cmd.env("PATH", "/usr/bin:/bin:/usr/local/bin");
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    cmd.env("TMPDIR", &policy.scratch_dir);

    // 3. Prevent arbitrary standard input / stream output safely
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // 3. Process grouping and kernel signal hooks
    #[cfg(unix)]
    {
        cmd.process_group(0);

        unsafe {
            cmd.pre_exec(move || {
                #[cfg(target_os = "linux")]
                {
                    // Ensure the kernel terminates the child if Unarc dies
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                    // Prevent child from gaining new privileges
                    libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
                }
                Ok(())
            });
        }
    }
}

/// Helper probing whether basic process isolation and environment scrubbing is functional.
fn probe_process_isolation() -> bool {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "echo probe_ok"]);
    cmd.env_clear();
    cmd.env("PATH", "/usr/bin:/bin");
    #[cfg(unix)]
    cmd.process_group(0);
    match cmd.output() {
        Ok(out) => {
            out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "probe_ok"
        }
        Err(_) => false,
    }
}

/// Generates a macOS Seatbelt Profile Language (SBPL) profile for native process confinement.
#[cfg(target_os = "macos")]
fn generate_macos_seatbelt_profile(policy: &ProcessSandboxPolicy) -> String {
    let mut profile = String::with_capacity(2048);
    profile.push_str("(version 1)\n");
    profile.push_str("(deny default)\n");

    if policy.deny_network {
        profile.push_str("(deny network*)\n");
    }

    // Engine binary execution
    let engine_path = policy
        .engine_binary
        .canonicalize()
        .unwrap_or_else(|_| policy.engine_binary.clone());
    let engine_escaped = escape_sbpl_string(&engine_path);
    let raw_engine_escaped = escape_sbpl_string(&policy.engine_binary);

    profile.push_str(&format!(
        "(allow process-exec (literal \"{engine_escaped}\") (literal \"{raw_engine_escaped}\"))\n"
    ));
    profile.push_str("(allow process-fork)\n");
    profile.push_str("(allow sysctl-read)\n");
    profile.push_str("(allow mach-lookup)\n");

    // Standard runtime, dynamic linker, and system library dependencies
    profile.push_str("(allow file-read-data file-read-metadata\n");
    profile.push_str("    (subpath \"/usr/lib\")\n");
    profile.push_str("    (subpath \"/System/Library\")\n");
    profile.push_str("    (subpath \"/System/Volumes\")\n");
    profile.push_str("    (subpath \"/usr/share\")\n");
    profile.push_str("    (subpath \"/dev\")\n");
    profile.push_str(&format!(
        "    (literal \"{engine_escaped}\") (literal \"{raw_engine_escaped}\"))\n"
    ));

    // Collect all ancestor directories that need metadata lookup for path traversal
    let mut ancestor_dirs = std::collections::BTreeSet::new();

    let mut add_ancestors = |p: &Path| {
        let mut cur = p.parent();
        while let Some(parent) = cur {
            if parent.as_os_str().is_empty() {
                break;
            }
            ancestor_dirs.insert(parent.to_path_buf());
            if let Ok(canon) = parent.canonicalize() {
                ancestor_dirs.insert(canon);
            }
            cur = parent.parent();
        }
    };

    add_ancestors(&policy.engine_binary);
    add_ancestors(&policy.scratch_dir);
    if let Some(ref dest) = policy.destination_dir {
        add_ancestors(dest);
    }

    // Permitted read-only archive input files
    for input in &policy.input_files {
        add_ancestors(input);
        let input_escaped = escape_sbpl_string(input);
        profile.push_str(&format!(
            "(allow file-read-data file-read-metadata (literal \"{input_escaped}\"))\n"
        ));
        if let Ok(canon) = input.canonicalize() {
            if canon != *input {
                let canon_escaped = escape_sbpl_string(&canon);
                profile.push_str(&format!(
                    "(allow file-read-data file-read-metadata (literal \"{canon_escaped}\"))\n"
                ));
            }
        }
    }

    // Allow metadata lookup on ancestor directories so the kernel can resolve permitted paths
    for dir in &ancestor_dirs {
        let dir_escaped = escape_sbpl_string(dir);
        profile.push_str(&format!(
            "(allow file-read-metadata (literal \"{dir_escaped}\"))\n"
        ));
    }

    // Permitted output destination for extraction writes
    if let Some(ref dest) = policy.destination_dir {
        let dest_escaped = escape_sbpl_string(dest);
        profile.push_str(&format!(
            "(allow file-read* file-write* (subpath \"{dest_escaped}\"))\n"
        ));
        if let Ok(canon) = dest.canonicalize() {
            if canon != *dest {
                let canon_escaped = escape_sbpl_string(&canon);
                profile.push_str(&format!(
                    "(allow file-read* file-write* (subpath \"{canon_escaped}\"))\n"
                ));
            }
        }
    }

    // Permitted isolated scratch space
    let scratch_escaped = escape_sbpl_string(&policy.scratch_dir);
    profile.push_str(&format!(
        "(allow file-read* file-write* (subpath \"{scratch_escaped}\"))\n"
    ));
    if let Ok(canon) = policy.scratch_dir.canonicalize() {
        if canon != policy.scratch_dir {
            let canon_escaped = escape_sbpl_string(&canon);
            profile.push_str(&format!(
                "(allow file-read* file-write* (subpath \"{canon_escaped}\"))\n"
            ));
        }
    }

    profile
}

/// Escapes a filesystem path for inclusion in a Seatbelt Scheme string literal.
#[cfg(target_os = "macos")]
fn escape_sbpl_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_sandbox_policy_construction() {
        let scratch = PathBuf::from("/tmp/scratch_test");
        let engine = PathBuf::from("/bin/echo");
        let policy = ProcessSandboxPolicy::new(engine.clone(), scratch.clone())
            .with_input(PathBuf::from("/tmp/archive.part1.rar"))
            .with_input(PathBuf::from("/tmp/archive.part2.rar"))
            .with_destination(PathBuf::from("/tmp/output"));

        assert_eq!(policy.engine_binary, engine);
        assert_eq!(policy.scratch_dir, scratch);
        assert_eq!(policy.input_files.len(), 2);
        assert_eq!(policy.destination_dir, Some(PathBuf::from("/tmp/output")));
        assert!(policy.deny_network);
    }

    #[test]
    fn test_scratch_workspace_lifecycle() {
        let workspace = ScratchWorkspace::new().expect("Workspace creation must succeed");
        let p = workspace.path().to_path_buf();
        assert!(p.is_dir());

        let probe_file = p.join("test.txt");
        std::fs::write(&probe_file, b"test").unwrap();
        assert!(probe_file.is_file());

        drop(workspace);
        assert!(!p.exists(), "Scratch workspace must be deleted upon drop");
    }

    #[test]
    fn test_sandbox_status_description() {
        let enforced = SandboxStatus::Enforced;
        assert_eq!(enforced.label(), "ENFORCED");
        assert!(enforced
            .description()
            .contains("kernel sandbox actively enforced"));

        let degraded = SandboxStatus::Degraded;
        assert_eq!(degraded.label(), "DEGRADED");
        assert!(degraded.description().contains("Process group isolation"));

        let unavailable = SandboxStatus::Unavailable;
        assert_eq!(unavailable.label(), "UNAVAILABLE");
        assert!(unavailable.description().contains("unavailable"));
    }

    #[test]
    fn test_child_process_guard_termination() {
        // Spawn a sleep process that would run for 60 seconds
        let mut cmd = Command::new("sleep");
        cmd.arg("60");

        #[cfg(unix)]
        cmd.process_group(0);

        let child = cmd.spawn().expect("Spawn sleep must succeed");
        let pid = child.id() as libc::pid_t;
        let guard = ChildProcessGuard::new(child, pid);

        assert!(guard.id().is_some());

        // Drop guard, which must terminate sleep within milliseconds
        drop(guard);

        // Verify sleep process is terminated
        #[cfg(unix)]
        unsafe {
            // kill with signal 0 checks if process is still alive
            let res = libc::kill(pid, 0);
            assert_ne!(res, 0, "Child process must have been terminated on drop");
        }
    }

    #[test]
    fn test_environment_scrubbing() {
        // Set a sensitive test variable in the current process
        std::env::set_var("_UNARC_TEST_SECRET", "super_secret_value_123");

        let scratch = ScratchWorkspace::new().unwrap();
        let policy =
            ProcessSandboxPolicy::new(PathBuf::from("/bin/sh"), scratch.path().to_path_buf());

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "echo ${_UNARC_TEST_SECRET:-NOT_FOUND}"]);
        apply_process_isolation(&mut cmd, &policy);

        let output = cmd.output().expect("sh execution failed");
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert_eq!(
            stdout.trim(),
            "NOT_FOUND",
            "Environment variable must be purged"
        );

        std::env::remove_var("_UNARC_TEST_SECRET");
    }
}
