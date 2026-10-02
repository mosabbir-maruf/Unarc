//! Presentation and output rendering.

use crate::archive::metadata::ArchiveMetadata;
use crate::core::app::AppInfo;
use crate::error::UnarcError;
use serde_json::json;
use std::path::Path;

/// Renders formatted output to stdout.
pub struct OutputFormatter {
    json_mode: bool,
    quiet: bool,
}

impl OutputFormatter {
    /// Creates a new output formatter.
    #[must_use]
    pub fn new(json_mode: bool, quiet: bool) -> Self {
        Self { json_mode, quiet }
    }

    /// Formats and displays archive inspection result.
    pub fn print_inspection(&self, path: &Path, metadata: &ArchiveMetadata) {
        if self.json_mode {
            let val = json!({
                "status": "success",
                "path": path.to_string_lossy(),
                "metadata": metadata,
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else if !self.quiet {
            println!("Archive Inspection: {}", path.display());
            println!("  Format:           {}", metadata.format);
            println!("  File Size:        {} bytes", metadata.file_size_bytes);
            println!("  Encrypted:        {}", metadata.is_encrypted);
            println!("  Solid:            {}", metadata.is_solid);
            if let Some(count) = metadata.entries_count {
                println!("  Entries Count:    {count}");
            }
            if let Some(uncomp) = metadata.total_uncompressed_bytes {
                println!("  Uncompressed:     {uncomp} bytes");
            }
        }
    }

    /// Formats and displays path validation result.
    pub fn print_validation(&self, original: &Path, sanitized: &Path, destination: Option<&Path>) {
        if self.json_mode {
            let val = json!({
                "status": "valid",
                "input": original.to_string_lossy(),
                "sanitized": sanitized.to_string_lossy(),
                "destination": destination.map(|d| d.to_string_lossy()),
            });
            println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else if !self.quiet {
            println!("Path Security Validation: PASS");
            println!("  Input:     {}", original.display());
            println!("  Sanitized: {}", sanitized.display());
            if let Some(dest) = destination {
                println!("  Resolved:  {}", dest.display());
            }
        }
    }

    /// Formats and displays application information and platform capability diagnostics.
    pub fn print_info(&self, info: &AppInfo) {
        if self.json_mode {
            println!("{}", serde_json::to_string_pretty(info).unwrap_or_default());
        } else if !self.quiet {
            println!("Unarc - Production-grade Security Archive Engine");
            println!("  Version:       {}", info.version);
            println!("  OS:            {}", info.platform.os);
            println!("  Architecture:  {}", info.platform.arch);
            println!("  Apple Silicon: {}", info.platform.is_apple_silicon);
            println!("  Linux:         {}", info.platform.is_linux);
            println!("  Security Capabilities:");
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
            println!("  Security Policy Default:");
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

    /// Formats and displays error message.
    pub fn print_error(&self, error: &UnarcError) {
        if self.json_mode {
            let val = json!({
                "status": "error",
                "exit_code": error.exit_code(),
                "message": error.to_string(),
            });
            eprintln!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
        } else {
            eprintln!("error: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_formatter_initialization() {
        let fmt = OutputFormatter::new(true, false);
        assert!(fmt.json_mode);
        assert!(!fmt.quiet);
    }
}
