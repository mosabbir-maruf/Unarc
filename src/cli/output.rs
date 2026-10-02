//! Presentation and output rendering.

use crate::archive::backend::{ArchiveExtractResult, ArchiveTestResult};
use crate::core::app::{AppInfo, DoctorReport, EngineInfo};
use crate::core::update::{UpdateApplyResult, UpdateCheckResult};
use crate::error::UnarcError;
use crate::security::SecurityPolicy;
use serde_json::json;

/// Renders formatted output to stdout.
pub struct OutputFormatter {
    json_mode: bool,
    quiet: bool,
    verbose: bool,
    color_enabled: bool,
}

impl OutputFormatter {
    /// Creates a new output formatter.
    #[must_use]
    pub fn new(json_mode: bool, quiet: bool, verbose: bool) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        Self {
            json_mode,
            quiet,
            verbose,
            color_enabled: !no_color,
        }
    }

    /// Returns whether quiet mode is enabled.
    #[must_use]
    pub fn is_quiet(&self) -> bool {
        self.quiet
    }

    /// Returns whether json mode is enabled.
    #[must_use]
    pub fn is_json(&self) -> bool {
        self.json_mode
    }

    /// Returns whether verbose mode is enabled.
    #[must_use]
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// Returns whether interactive progress indicators should be displayed.
    #[must_use]
    pub fn should_show_progress(&self) -> bool {
        use std::io::IsTerminal;
        !self.quiet && !self.json_mode && std::io::stdout().is_terminal()
    }

    /// Helper for conditional ANSI color styling.
    fn style<'a>(&self, text: &'a str, ansi_code: &'a str) -> std::borrow::Cow<'a, str> {
        if self.color_enabled {
            std::borrow::Cow::Owned(format!("{ansi_code}{text}\x1b[0m"))
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }

    /// Formats and displays version info.
    pub fn print_version(&self) {
        if self.json_mode {
            let val = json!({
                "name": "unarc",
                "version": env!("CARGO_PKG_VERSION")
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else {
            println!("unarc {}", env!("CARGO_PKG_VERSION"));
        }
    }

    /// Formats and displays archive test result.
    pub fn print_test_result(&self, result: &ArchiveTestResult) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(result).unwrap_or_default()
            );
        } else if !self.quiet {
            let status_badge = if result.passed {
                self.style("PASS", "\x1b[1;32m")
            } else {
                self.style("FAIL", "\x1b[1;31m")
            };
            println!("Archive Test: {} [{}]", result.path.display(), status_badge);
            println!("  Format:   {}", result.format);
            println!("  Status:   {}", result.message);
            if let Some(count) = result.entries_checked {
                println!("  Entries:  {count}");
            }
        }
    }

    /// Formats and displays archive extraction result.
    pub fn print_extract_result(&self, result: &ArchiveExtractResult) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(result).unwrap_or_default()
            );
        } else if !self.quiet {
            let status_badge = self.style("SUCCESS", "\x1b[1;32m");
            println!(
                "Archive Extract: {} [{}]",
                result.archive_path.display(),
                status_badge
            );
            println!("  Destination: {}", result.destination.display());
            println!("  Format:      {}", result.format);
            if let Some(entries) = result.entries_extracted {
                println!("  Entries:     {entries}");
            }
            if let Some(bytes) = result.total_bytes_extracted {
                println!("  Extracted:   {bytes} bytes");
            }
        }
    }

    /// Formats and displays application information and platform capability diagnostics.
    pub fn print_info(&self, info: &AppInfo) {
        if self.json_mode {
            println!("{}", serde_json::to_string_pretty(info).unwrap_or_default());
        } else if !self.quiet {
            println!("Unarc - Secure Archive Utility");
            println!("  Version:       {}", info.version);
            println!("  OS:            {}", info.platform.os);
            println!("  Architecture:  {}", info.platform.arch);
            println!("  Apple Silicon: {}", info.platform.is_apple_silicon);
            println!("  Linux:         {}", info.platform.is_linux);
            println!("  Engine Status:");
            println!("    Pinned 7-Zip:    v{}", info.engine.pinned_version);
            println!("    Available:       {}", info.engine.is_available);
            println!("    Expected SHA256: {}", info.engine.expected_sha256);
            if let Some(ref path) = info.engine.resolved_path {
                println!("    Binary Path:     {}", path.display());
            }
            if self.verbose {
                println!("  Platform Capabilities:");
                println!(
                    "    Quarantine XAttr Support:   {}",
                    info.platform.capabilities.supports_quarantine_xattr
                );
                println!(
                    "    POSIX Permission Support:   {}",
                    info.platform.capabilities.supports_posix_permissions
                );
                println!(
                    "    Sandbox Confinement:        {}",
                    info.platform.capabilities.supports_sandbox_confinement
                );
                println!("  Security Policy Defaults:");
                println!(
                    "    Allow Absolute Paths:       {}",
                    info.policy.allow_absolute_paths
                );
                println!(
                    "    Allow Symlinks:             {}",
                    info.policy.allow_symlinks
                );
                println!(
                    "    Max Path Depth:             {}",
                    info.policy.max_path_depth
                );
            }
        }
    }

    /// Formats and displays doctor health check.
    pub fn print_doctor(&self, report: &DoctorReport) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_default()
            );
        } else if !self.quiet {
            let status = if report.healthy {
                self.style("HEALTHY", "\x1b[1;32m")
            } else {
                self.style("ATTENTION NEEDED", "\x1b[1;31m")
            };
            println!("System Diagnostics Doctor: [{status}]");
            println!(
                "  Platform:       {}-{}",
                report.platform.os, report.platform.arch
            );
            println!("  Pinned Engine:  v{}", report.engine.pinned_version);
            println!("  Diagnostic Probes:");
            for check in &report.checks {
                let badge = if !check.passed {
                    self.style("FAIL", "\x1b[1;31m")
                } else if check.message.starts_with("DEGRADED:") {
                    self.style("WARN", "\x1b[1;33m")
                } else {
                    self.style("PASS", "\x1b[1;32m")
                };
                println!("    [{badge}] {:<30} {}", check.name, check.message);
            }
        }
    }

    /// Explicitly communicates deferred feature status without placeholder architecture.
    pub fn print_deferred_command(&self, cmd: &str, explanation: &str) {
        if self.json_mode {
            let val = json!({
                "status": "not_available_yet",
                "command": cmd,
                "message": explanation
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else if !self.quiet {
            println!("Command '{cmd}' is not available yet.");
            println!("  Notice: {explanation}");
        }
    }

    /// Formats and displays security configuration.
    pub fn print_config(&self, policy: &SecurityPolicy) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(policy).unwrap_or_default()
            );
        } else if !self.quiet {
            println!("Security Configuration (Zero-Trust Enforcement):");
            println!("  Allow Absolute Paths:  {}", policy.allow_absolute_paths);
            println!("  Allow Symlinks:        {}", policy.allow_symlinks);
            println!("  Max Path Depth:        {}", policy.max_path_depth);
            println!("  Max Path Length:       {} bytes", policy.max_path_length);
        }
    }

    /// Formats and displays engine update status.
    pub fn print_engine_update(&self, engine: &EngineInfo) {
        if self.json_mode {
            let val = json!({
                "status": "hermetic",
                "pinned_version": engine.pinned_version,
                "release_url": engine.release_url,
                "note": "Unarc uses pinned, verified engines. Dynamic runtime updates are disabled for security."
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else if !self.quiet {
            println!("Engine Update Status:");
            println!("  Pinned Version:   v{}", engine.pinned_version);
            println!("  Source Release:   {}", engine.release_url);
            println!("  Policy:           Hermetic & pinned at build-time. Dynamic runtime updates are disabled.");
        }
    }

    /// Formats and displays outcome of self-update check.
    pub fn print_update_check(&self, result: &UpdateCheckResult) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(result).unwrap_or_default()
            );
        } else if !self.quiet {
            let status_badge = if result.update_available {
                self.style("UPDATE AVAILABLE", "\x1b[1;36m")
            } else {
                self.style("UP TO DATE", "\x1b[1;32m")
            };
            println!("Self-Update Check: [{status_badge}]");
            println!("  Current Version:  v{}", result.current_version);
            println!("  Latest Version:   v{}", result.latest_version);
            println!(
                "  Target Platform:  {}-{}",
                result.manifest.target_os, result.manifest.target_arch
            );
            println!(
                "  Bundled 7-Zip:    v{}",
                result.manifest.bundled_7zz_version
            );
            println!("  Artifact SHA-256: {}", result.manifest.artifact_sha256);
            if result.update_available {
                println!("\nRun 'unarc update' to download, verify, and apply this update.");
            }
        }
    }

    /// Formats and displays outcome of applied self-update.
    pub fn print_update_apply(&self, result: &UpdateApplyResult) {
        if self.json_mode {
            println!(
                "{}",
                serde_json::to_string_pretty(result).unwrap_or_default()
            );
        } else if !self.quiet {
            let status_badge = self.style("SUCCESS", "\x1b[1;32m");
            println!("Self-Update Installed: [{status_badge}]");
            println!("  Previous Version: v{}", result.previous_version);
            println!("  New Version:      v{}", result.new_version);
            println!("  Installed Binary: {}", result.binary_path.display());
            println!(
                "  Verification:     Ed25519 signature & SHA-256 checksum verified authentic."
            );
        }
    }

    /// Formats and displays error message.
    pub fn print_error(&self, error: &UnarcError) {
        let code = error.code();
        if self.json_mode {
            let val = json!({
                "status": "error",
                "error_code": code.as_str(),
                "exit_code": error.exit_code(),
                "message": error.to_string(),
            });
            eprintln!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else {
            eprintln!("error [{}]: {error}", code.as_str());
            if self.verbose {
                eprintln!("  Diagnostic detail: exit_code={}", error.exit_code());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_formatter_initialization() {
        let fmt = OutputFormatter::new(true, false, false);
        assert!(fmt.json_mode);
        assert!(!fmt.quiet);
    }
}
