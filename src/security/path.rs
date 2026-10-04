//! Path sanitization and traversal prevention primitives.

use crate::error::SecurityError;
use std::path::{Component, Path, PathBuf};

/// Maximum permissible path length in bytes to prevent buffer and system exhaustion.
pub const DEFAULT_MAX_PATH_LENGTH: usize = 1024;

/// Maximum permissible directory nesting depth to prevent stack overflow or traversal bombs.
pub const DEFAULT_MAX_PATH_DEPTH: usize = 64;

/// Verifies and sanitizes a relative path ensuring it does not traverse parent directories
/// or escape expected containment boundaries.
pub fn sanitize_relative_path(path: &Path, max_depth: usize) -> Result<PathBuf, SecurityError> {
    let path_str = path.to_string_lossy();

    // Check for null bytes or illegal control characters
    if path_str.contains('\0') {
        return Err(SecurityError::InvalidPath {
            details: "Path contains embedded null byte".to_string(),
        });
    }

    if path_str.len() > DEFAULT_MAX_PATH_LENGTH {
        return Err(SecurityError::InvalidPath {
            details: format!(
                "Path length {} exceeds maximum allowed {}",
                path_str.len(),
                DEFAULT_MAX_PATH_LENGTH
            ),
        });
    }

    // Reject absolute paths
    if path.is_absolute() {
        return Err(SecurityError::AbsolutePathNotAllowed {
            path: path_str.to_string(),
        });
    }

    // Windows prefix check (e.g. C:, \\server\share) regardless of host platform
    for component in path.components() {
        if matches!(component, Component::Prefix(_)) {
            return Err(SecurityError::AbsolutePathNotAllowed {
                path: path_str.to_string(),
            });
        }
    }

    // Also explicitly check string for leading slash or backslash (cross-platform Zip standard)
    if path_str.starts_with('/') || path_str.starts_with('\\') {
        return Err(SecurityError::AbsolutePathNotAllowed {
            path: path_str.to_string(),
        });
    }

    let mut clean_components = Vec::new();
    let mut depth = 0usize;

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(SecurityError::AbsolutePathNotAllowed {
                    path: path_str.to_string(),
                });
            }
            Component::ParentDir => {
                // Strictly disallow any parent directory traversal in relative archive paths
                return Err(SecurityError::PathTraversal {
                    path: path_str.to_string(),
                });
            }
            Component::CurDir => {
                // Skip redundant current directory references (.)
            }
            Component::Normal(comp) => {
                let comp_str = comp.to_string_lossy();
                // Reject component with traversal patterns or reserved names
                if comp_str == ".." || comp_str.contains('/') || comp_str.contains('\\') {
                    return Err(SecurityError::PathTraversal {
                        path: path_str.to_string(),
                    });
                }
                depth += 1;
                if depth > max_depth {
                    return Err(SecurityError::PolicyViolation {
                        reason: format!("Path nesting depth {depth} exceeds limit {max_depth}"),
                    });
                }
                clean_components.push(comp);
            }
        }
    }

    if clean_components.is_empty() {
        return Err(SecurityError::InvalidPath {
            details: "Path resolves to empty or root-only target".to_string(),
        });
    }

    let mut result = PathBuf::new();
    for comp in clean_components {
        result.push(comp);
    }

    Ok(result)
}

/// Validates that joining `base_dir` and `relative_path` remains strictly contained within `base_dir`.
pub fn verify_boundary_containment(
    base_dir: &Path,
    relative_path: &Path,
    max_depth: usize,
) -> Result<PathBuf, SecurityError> {
    let clean_rel = sanitize_relative_path(relative_path, max_depth)?;
    let target = base_dir.join(&clean_rel);

    // Lexical containment verification
    if !target.starts_with(base_dir) {
        return Err(SecurityError::BoundaryEscaped {
            path: target.to_string_lossy().to_string(),
        });
    }

    Ok(target)
}

/// Formats an I/O error into a concise, human-readable cause string without redundant prefixes.
#[must_use]
pub fn format_os_error(err: &std::io::Error) -> String {
    #[cfg(unix)]
    if let Some(code) = err.raw_os_error() {
        if code == 1 {
            return "Operation not permitted".to_string();
        } else if code == 13 {
            return "Permission denied".to_string();
        } else if code == 30 {
            return "Read-only file system".to_string();
        }
    }
    err.to_string()
}

/// RAII guard ensuring temporary probe files are deleted on drop in all exit paths.
struct SafeProbeGuard<'a> {
    probe_path: &'a Path,
    created: bool,
}

impl Drop for SafeProbeGuard<'_> {
    fn drop(&mut self) {
        if self.created {
            let _ = std::fs::remove_file(self.probe_path);
        }
    }
}

/// Probes whether a directory is genuinely writable by the current process.
///
/// Creates a uniquely named temporary probe file and immediately unlinks it.
/// Uses `create_new(true)` to guarantee no pre-existing files are overwritten,
/// and an RAII drop guard to ensure cleanup even on panic or error.
pub fn probe_directory_writable(dir: &Path) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);

    let pid = std::process::id();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let probe_name = format!(".unarc_probe_{pid}_{nanos}_{count}.tmp");
    let probe_path = dir.join(probe_name);

    let mut guard = SafeProbeGuard {
        probe_path: &probe_path,
        created: false,
    };

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path)?;

    guard.created = true;
    drop(file);

    let remove_res = std::fs::remove_file(&probe_path);
    if remove_res.is_ok() {
        guard.created = false;
    }
    remove_res
}

/// Finds the nearest existing ancestor directory for a given path.
/// If `path` exists, returns `path.to_path_buf()`.
#[must_use]
pub fn find_nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    while !current.exists() {
        if let Some(parent) = current.parent() {
            if parent == current || parent.as_os_str().is_empty() {
                return PathBuf::from(".");
            }
            current = parent.to_path_buf();
        } else {
            return PathBuf::from(".");
        }
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_relative_paths() {
        let path = Path::new("documents/report.pdf");
        let sanitized = sanitize_relative_path(path, DEFAULT_MAX_PATH_DEPTH).unwrap();
        assert_eq!(sanitized, PathBuf::from("documents/report.pdf"));

        let single = Path::new("file.txt");
        assert_eq!(
            sanitize_relative_path(single, DEFAULT_MAX_PATH_DEPTH).unwrap(),
            PathBuf::from("file.txt")
        );

        let with_curdir = Path::new("./folder/./sub/./file.txt");
        assert_eq!(
            sanitize_relative_path(with_curdir, DEFAULT_MAX_PATH_DEPTH).unwrap(),
            PathBuf::from("folder/sub/file.txt")
        );
    }

    #[test]
    fn test_rejects_parent_traversal() {
        let cases = [
            "../secret.txt",
            "folder/../../secret.txt",
            "folder/../secret.txt",
            "sub/child/../../../etc/passwd",
        ];

        for case in cases {
            let res = sanitize_relative_path(Path::new(case), DEFAULT_MAX_PATH_DEPTH);
            assert!(
                matches!(res, Err(SecurityError::PathTraversal { .. })),
                "Expected PathTraversal for case: {case}, got: {res:?}"
            );
        }
    }

    #[test]
    fn test_rejects_absolute_paths() {
        let cases = ["/etc/passwd", "/tmp/file", "\\Windows\\System32"];

        for case in cases {
            let res = sanitize_relative_path(Path::new(case), DEFAULT_MAX_PATH_DEPTH);
            assert!(
                matches!(res, Err(SecurityError::AbsolutePathNotAllowed { .. })),
                "Expected AbsolutePathNotAllowed for case: {case}, got: {res:?}"
            );
        }
    }

    #[test]
    fn test_rejects_null_bytes() {
        let path = Path::new("safe\0file.txt");
        let res = sanitize_relative_path(path, DEFAULT_MAX_PATH_DEPTH);
        assert!(matches!(res, Err(SecurityError::InvalidPath { .. })));
    }

    #[test]
    fn test_enforces_depth_limit() {
        let deep_path = Path::new("a/b/c/d/e/f");
        let res = sanitize_relative_path(deep_path, 3);
        assert!(matches!(res, Err(SecurityError::PolicyViolation { .. })));

        let ok_res = sanitize_relative_path(deep_path, 10);
        assert!(ok_res.is_ok());
    }

    #[test]
    fn test_boundary_containment() {
        let base = Path::new("/var/destination");
        let valid_rel = Path::new("extracted/data.json");
        let res = verify_boundary_containment(base, valid_rel, DEFAULT_MAX_PATH_DEPTH).unwrap();
        assert_eq!(res, PathBuf::from("/var/destination/extracted/data.json"));
    }
}
