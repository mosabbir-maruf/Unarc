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
    #[must_use]
    pub fn style<'a>(&self, text: &'a str, ansi_code: &'a str) -> std::borrow::Cow<'a, str> {
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
            let display_message = if result.passed && result.message == "Everything is Ok" {
                "Integrity verified (all checksums match)"
            } else {
                &result.message
            };
            println!("  Status:   {display_message}");
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
                println!("  Extracted:   {}", format_byte_size(bytes));
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

            use std::io::IsTerminal;
            let wrap_width = if std::io::stdout().is_terminal() {
                crossterm::terminal::size()
                    .map(|(w, _)| w as usize)
                    .unwrap_or(80)
            } else {
                usize::MAX
            };
            let prefix_len = 42usize;
            let msg_max_width = wrap_width.saturating_sub(prefix_len).max(20);

            for check in &report.checks {
                let badge = if !check.passed {
                    self.style("FAIL", "\x1b[1;31m")
                } else if check.message.starts_with("DEGRADED:") {
                    self.style("WARN", "\x1b[1;33m")
                } else {
                    self.style("PASS", "\x1b[1;32m")
                };

                let lines = wrap_message(&check.message, msg_max_width);
                if lines.len() <= 1 {
                    println!("    [{badge}] {:<30} {}", check.name, check.message);
                } else {
                    println!("    [{badge}] {:<30} {}", check.name, lines[0]);
                    for line in &lines[1..] {
                        println!("{:prefix_len$}{line}", "");
                    }
                }
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

/// Formats an integer with standard thousands comma separators (e.g. 104,857,600).
fn format_integer_with_commas(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let rem = s.len() % 3;
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (i == rem || (i > rem && (i - rem) % 3 == 0)) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

/// Formats a byte quantity into a human-readable binary size with raw bytes in parentheses.
///
/// Example:
/// - `38` -> `"38 bytes"`
/// - `104857600` -> `"100.0 MB (104,857,600 bytes)"`
#[must_use]
pub fn format_byte_size(bytes: u64) -> String {
    let formatted_bytes = format_integer_with_commas(bytes);
    if bytes < 1024 {
        if bytes == 1 {
            "1 byte".to_string()
        } else {
            format!("{bytes} bytes")
        }
    } else if bytes < 1024 * 1024 {
        let kb = bytes as f64 / 1024.0;
        format!("{kb:.1} KB ({formatted_bytes} bytes)")
    } else if bytes < 1024 * 1024 * 1024 {
        let mb = bytes as f64 / (1024.0 * 1024.0);
        format!("{mb:.1} MB ({formatted_bytes} bytes)")
    } else {
        let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        format!("{gb:.2} GB ({formatted_bytes} bytes)")
    }
}

/// Wraps a message text at word boundaries to fit within `max_width`.
pub fn wrap_message(text: &str, max_width: usize) -> Vec<&str> {
    if text.len() <= max_width {
        return vec![text];
    }
    let mut lines = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_width {
            lines.push(remaining);
            break;
        }
        let slice = &remaining[..max_width];
        let break_idx = match slice.rfind(' ') {
            Some(idx) if idx > 0 => idx,
            _ => match remaining.find(' ') {
                Some(idx) => idx,
                None => {
                    lines.push(remaining);
                    break;
                }
            },
        };
        lines.push(remaining[..break_idx].trim());
        remaining = remaining[break_idx..].trim_start();
    }
    lines
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

    #[test]
    fn test_format_byte_size() {
        assert_eq!(format_byte_size(0), "0 bytes");
        assert_eq!(format_byte_size(1), "1 byte");
        assert_eq!(format_byte_size(38), "38 bytes");
        assert_eq!(format_byte_size(1023), "1023 bytes");
        assert_eq!(format_byte_size(1024), "1.0 KB (1,024 bytes)");
        assert_eq!(format_byte_size(104857600), "100.0 MB (104,857,600 bytes)");
        assert_eq!(
            format_byte_size(1073741824),
            "1.00 GB (1,073,741,824 bytes)"
        );
    }

    #[test]
    fn test_wrap_message() {
        let short = "Short message";
        assert_eq!(wrap_message(short, 20), vec!["Short message"]);

        let long = "This is a long message that needs wrapping across multiple terminal lines";
        let wrapped = wrap_message(long, 25);
        assert!(wrapped.len() >= 3);
        for line in &wrapped {
            assert!(line.len() <= 28);
        }
    }
}
