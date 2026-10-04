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

    /// Prints a key-value row with standard 2-space indentation and aligned spacing.
    pub fn print_row(&self, label: &str, value: &str) {
        if !self.quiet {
            if self.color_enabled {
                println!("  \x1b[1m{label:<22}\x1b[0m {value}");
            } else {
                println!("  {label:<22} {value}");
            }
        }
    }

    /// Prints a nested key-value row with 4-space indentation.
    pub fn print_subrow(&self, label: &str, value: &str) {
        if !self.quiet {
            if self.color_enabled {
                println!("    \x1b[1m{label:<20}\x1b[0m {value}");
            } else {
                println!("    {label:<20} {value}");
            }
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
            let sanitized_path =
                crate::cli::progress::sanitize_terminal_text(&result.path.display().to_string());
            println!("Archive Test: {sanitized_path} [{status_badge}]");
            let display_message = if result.passed && result.message == "Everything is Ok" {
                "Integrity verified (all checksums match)"
            } else {
                &result.message
            };
            let sanitized_status = crate::cli::progress::sanitize_terminal_text(display_message);
            self.print_row("Format:", &result.format.to_string());
            self.print_row("Status:", &sanitized_status);
            if let Some(count) = result.entries_checked {
                self.print_row("Entries:", &count.to_string());
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
            let sanitized_path = crate::cli::progress::sanitize_terminal_text(
                &result.archive_path.display().to_string(),
            );
            let sanitized_dest = crate::cli::progress::sanitize_terminal_text(
                &result.destination.display().to_string(),
            );
            println!("Archive Extract: {sanitized_path} [{status_badge}]");
            self.print_row("Destination:", &sanitized_dest);
            self.print_row("Format:", &result.format.to_string());
            if let Some(entries) = result.entries_extracted {
                self.print_row("Entries:", &entries.to_string());
            }
            if let Some(bytes) = result.total_bytes_extracted {
                self.print_row("Extracted:", &format_byte_size(bytes));
            }
        }
    }

    /// Formats and displays application information and platform capability diagnostics.
    pub fn print_info(&self, info: &AppInfo) {
        if self.json_mode {
            println!("{}", serde_json::to_string_pretty(info).unwrap_or_default());
        } else if !self.quiet {
            let version = &info.version;
            println!("Unarc — Secure Archive Utility [v{version}]");
            println!("  Platform:");
            let os_desc = if info.platform.is_apple_silicon {
                format!("{} (Apple Silicon)", info.platform.os)
            } else if info.platform.is_linux {
                format!("{} (Linux)", info.platform.os)
            } else {
                info.platform.os.clone()
            };
            self.print_subrow("Operating System:", &os_desc);
            self.print_subrow("Architecture:", &info.platform.arch);

            println!("  Engine Status:");
            self.print_subrow("Pinned 7-Zip:", &format!("v{}", info.engine.pinned_version));
            self.print_subrow("Available:", &info.engine.is_available.to_string());
            self.print_subrow("Expected SHA-256:", &info.engine.expected_sha256);
            if let Some(ref path) = info.engine.resolved_path {
                let sanitized_path =
                    crate::cli::progress::sanitize_terminal_text(&path.display().to_string());
                self.print_subrow("Binary Path:", &sanitized_path);
            }

            if self.verbose {
                println!("  Platform Capabilities:");
                self.print_subrow(
                    "Quarantine XAttr:",
                    &info
                        .platform
                        .capabilities
                        .supports_quarantine_xattr
                        .to_string(),
                );
                self.print_subrow(
                    "POSIX Permissions:",
                    &info
                        .platform
                        .capabilities
                        .supports_posix_permissions
                        .to_string(),
                );
                self.print_subrow(
                    "Sandbox Confinement:",
                    &info
                        .platform
                        .capabilities
                        .supports_sandbox_confinement
                        .to_string(),
                );
                println!("  Security Policy Defaults:");
                self.print_subrow(
                    "Allow Absolute Paths:",
                    &info.policy.allow_absolute_paths.to_string(),
                );
                self.print_subrow("Allow Symlinks:", &info.policy.allow_symlinks.to_string());
                self.print_subrow("Max Path Depth:", &info.policy.max_path_depth.to_string());
                self.print_subrow(
                    "Max Path Length:",
                    &format!("{} bytes", info.policy.max_path_length),
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

                let sanitized_msg = crate::cli::progress::sanitize_terminal_text(&check.message);
                let sanitized_name = crate::cli::progress::sanitize_terminal_text(&check.name);
                let lines = wrap_message(&sanitized_msg, msg_max_width);
                if lines.len() <= 1 {
                    println!("    [{badge}] {:<30} {}", sanitized_name, lines[0]);
                } else {
                    println!("    [{badge}] {:<30} {}", sanitized_name, lines[0]);
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
            self.print_row(
                "Allow Absolute Paths:",
                &policy.allow_absolute_paths.to_string(),
            );
            self.print_row("Allow Symlinks:", &policy.allow_symlinks.to_string());
            self.print_row("Max Path Depth:", &policy.max_path_depth.to_string());
            self.print_row(
                "Max Path Length:",
                &format!("{} bytes", policy.max_path_length),
            );
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
            self.print_row("Pinned Version:", &format!("v{}", engine.pinned_version));
            self.print_row("Source Release:", &engine.release_url);
            self.print_row(
                "Policy:",
                "Hermetic & pinned at build-time. Dynamic runtime updates are disabled.",
            );
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
            self.print_row("Current Version:", &format!("v{}", result.current_version));
            self.print_row("Latest Version:", &format!("v{}", result.latest_version));
            self.print_row(
                "Target Platform:",
                &format!(
                    "{}-{}",
                    result.manifest.target_os, result.manifest.target_arch
                ),
            );
            self.print_row(
                "Bundled 7-Zip:",
                &format!("v{}", result.manifest.bundled_7zz_version),
            );
            self.print_row("Artifact SHA-256:", &result.manifest.artifact_sha256);
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
            let sanitized_path = crate::cli::progress::sanitize_terminal_text(
                &result.binary_path.display().to_string(),
            );
            println!("Self-Update Installed: [{status_badge}]");
            self.print_row(
                "Previous Version:",
                &format!("v{}", result.previous_version),
            );
            self.print_row("New Version:", &format!("v{}", result.new_version));
            self.print_row("Installed Binary:", &sanitized_path);
            self.print_row(
                "Verification:",
                "Ed25519 signature & SHA-256 checksum verified authentic.",
            );
        }
    }

    /// Formats and displays interactive slash commands help reference.
    pub fn print_help(&self) {
        if self.json_mode {
            let commands: Vec<_> = crate::cli::interactive::SUGGESTIONS
                .iter()
                .map(|s| {
                    json!({
                        "command": s.command,
                        "description": s.description
                    })
                })
                .collect();
            let val = json!({
                "commands": commands
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else if !self.quiet {
            println!("Interactive Command Reference:");
            for item in crate::cli::interactive::SUGGESTIONS {
                if self.color_enabled {
                    println!("  \x1b[1m{:<12}\x1b[0m {}", item.command, item.description);
                } else {
                    println!("  {:<12} {}", item.command, item.description);
                }
            }
            println!("\n  Tips:");
            self.print_subrow(
                "Direct run:",
                "Type '/extract <path>' or drag-and-drop an archive file.",
            );
            self.print_subrow(
                "Navigation:",
                "Use Up/Down arrows to select, Tab to autocomplete, Esc to clear.",
            );
            self.print_subrow(
                "CLI help:",
                "Run 'unarc --help' from the shell for standard command-line flags.",
            );
        }
    }

    /// Formats and displays error message with contextual hints.
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
            let raw_msg = error.to_string();
            let sanitized_msg = raw_msg
                .lines()
                .map(crate::cli::progress::sanitize_terminal_text)
                .collect::<Vec<_>>()
                .join("\n");
            if self.color_enabled {
                eprintln!(
                    "\x1b[1;31merror\x1b[0m \x1b[90m[{}]\x1b[0m: {sanitized_msg}",
                    code.as_str()
                );
            } else {
                eprintln!("error [{}]: {sanitized_msg}", code.as_str());
            }
            if let Some(hint) = error_hint_for_error(error) {
                if sanitized_msg.contains('\n') {
                    eprintln!();
                }
                if self.color_enabled {
                    eprintln!("  \x1b[1mHint:\x1b[0m {hint}");
                } else {
                    eprintln!("  Hint: {hint}");
                }
            }
            if self.verbose {
                eprintln!("  Diagnostic detail: exit_code={}", error.exit_code());
            }
        }
    }
}

/// Helper detecting common macOS TCC-protected locations (e.g. /Volumes, Desktop, Documents, Downloads).
fn is_macos_tcc_location(path_str: &str) -> bool {
    let path = std::path::Path::new(path_str);
    if path.starts_with("/Volumes") {
        return true;
    }
    if let Ok(home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(&home);
        if path.starts_with(home_path.join("Desktop"))
            || path.starts_with(home_path.join("Documents"))
            || path.starts_with(home_path.join("Downloads"))
        {
            return true;
        }
    }
    false
}

/// Returns an actionable user-facing hint tailored to the specific error, if available.
#[must_use]
pub fn error_hint_for_error(error: &UnarcError) -> Option<String> {
    let code = error.code();
    if code == crate::error::ErrorCode::PermissionDenied {
        let is_macos_privacy = match error {
            UnarcError::Archive(crate::error::ArchiveError::PermissionDenied {
                path,
                cause,
                ..
            }) => {
                cause.contains("Operation not permitted")
                    || cause.contains("os error 1")
                    || path.starts_with("/Volumes")
                    || is_macos_tcc_location(path)
            }
            UnarcError::Security(crate::error::SecurityError::PermissionDenied { operation }) => {
                operation.contains("Operation not permitted")
                    || operation.contains("os error 1")
                    || operation.contains("/Volumes")
            }
            UnarcError::Platform(crate::error::PlatformError::PermissionDenied { operation }) => {
                operation.contains("Operation not permitted")
                    || operation.contains("os error 1")
                    || operation.contains("/Volumes")
            }
            UnarcError::Io(err) => {
                #[cfg(unix)]
                let is_eperm = err.raw_os_error() == Some(1);
                #[cfg(not(unix))]
                let is_eperm = false;
                is_eperm || err.to_string().contains("Operation not permitted")
            }
            _ => false,
        };

        if is_macos_privacy {
            return Some(
                "Grant Unarc access to this location in macOS Privacy & Security, then retry."
                    .to_string(),
            );
        }
    }

    error_hint(code).map(|s| s.to_string())
}

/// Returns an actionable user-facing hint for a given error code, if available.
#[must_use]
pub fn error_hint(code: crate::error::ErrorCode) -> Option<&'static str> {
    use crate::error::ErrorCode::*;
    match code {
        InputNotFound => Some("Verify that the archive path exists and is spelled correctly."),
        InputNotFile => Some("Target path is a directory or special device, not an archive file."),
        UnsupportedFormat => Some("The archive format is unrecognized or unsupported."),
        MissingVolume => Some(
            "Ensure all multi-part volume files (.part1, .part2, .z01, etc.) are in the same folder.",
        ),
        InvalidVolume => Some("A multi-part volume appears corrupt, mismatched, or truncated."),
        CorruptArchive => Some("The archive header or data failed verification."),
        PasswordRequired => {
            Some("Provide the archive password via the interactive terminal prompt.")
        }
        InvalidPassword => Some("The provided password could not decrypt the archive."),
        OutputInvalid => Some(
            "Check that destination is a valid directory path and not an existing non-directory file.",
        ),
        PathTraversal => Some(
            "Refused extraction due to unsafe paths breakout (../) outside destination directory.",
        ),
        UnsafeEntry => Some(
            "Refused extraction of unsafe archive content (e.g. absolute symlink or device node).",
        ),
        SecurityPolicyViolation => {
            Some("Operation violates active security policy boundaries (e.g. symlinks disallowed).")
        }
        PermissionDenied => Some(
            "Check filesystem permissions for read access to the archive and write access to destination.",
        ),
        ExtractionFailed => Some("Decompression aborted or failed during archive processing."),
        EngineFailed => {
            Some("Run 'unarc doctor' to verify engine binary integrity and platform diagnostics.")
        }
        Interrupted => Some("Operation was cancelled by user signal."),
        CliError => Some("Run 'unarc --help' to see valid command syntax and options."),
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

    #[test]
    fn test_error_hints_coverage() {
        use crate::error::ErrorCode::*;
        let all_codes = [
            InputNotFound,
            InputNotFile,
            UnsupportedFormat,
            MissingVolume,
            InvalidVolume,
            CorruptArchive,
            PasswordRequired,
            InvalidPassword,
            OutputInvalid,
            PathTraversal,
            UnsafeEntry,
            SecurityPolicyViolation,
            PermissionDenied,
            ExtractionFailed,
            EngineFailed,
            Interrupted,
            CliError,
        ];
        for code in all_codes {
            assert!(
                error_hint(code).is_some(),
                "Error code {code:?} missing actionable hint"
            );
        }
    }

    #[test]
    fn test_output_formatter_row_printing() {
        let fmt = OutputFormatter::new(false, false, false);
        // Just verify print_row and print_subrow do not panic
        fmt.print_row("Test Label:", "Test Value");
        fmt.print_subrow("Sub Label:", "Sub Value");
    }

    #[test]
    fn test_error_hint_for_error_macos_privacy() {
        let macos_err: UnarcError = crate::error::ArchiveError::permission_denied(
            "destination",
            "/Volumes/PS5",
            "Operation not permitted",
        )
        .into();
        let hint = error_hint_for_error(&macos_err).unwrap();
        assert_eq!(
            hint,
            "Grant Unarc access to this location in macOS Privacy & Security, then retry."
        );

        let standard_err: UnarcError = crate::error::ArchiveError::permission_denied(
            "destination",
            "/tmp/dest",
            "Permission denied",
        )
        .into();
        let hint_std = error_hint_for_error(&standard_err).unwrap();
        assert_eq!(
            hint_std,
            "Check filesystem permissions for read access to the archive and write access to destination."
        );
    }
}
