//! Terminal-native interactive Terminal UI (TUI) and suggestion engine.

use crate::archive::ArchiveFormat;
use crate::core::Application;
use crate::core::app::AppInfo;
use crate::error::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{IsTerminal, Write, stdin, stdout};
use std::path::{Path, PathBuf};

/// Clean ASCII wordmark with subtitle (used for non-TTY / piped fallback).
pub const ASCII_BANNER: &str = r#"
 _   _ _   _   _    ____   ____ 
| | | | \ | | / \  |  _ \ / ___|
| | | |  \| |/ _ \ | |_) | |    
| |_| | |\  / ___ \|  _ <| |___ 
 \___/|_| \_/_/   \_\_| \_\\____|
     Secure Archive Utility
"#;

/// Available slash commands and descriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSuggestion {
    pub command: &'static str,
    pub description: &'static str,
}

pub const SUGGESTIONS: &[CommandSuggestion] = &[
    CommandSuggestion {
        command: "/extract",
        description: "Extract archive with boundary verification",
    },
    CommandSuggestion {
        command: "/test",
        description: "Test archive integrity without writing to disk",
    },
    CommandSuggestion {
        command: "/info",
        description: "Display platform, engine, and security details",
    },
    CommandSuggestion {
        command: "/doctor",
        description: "Run diagnostic health check and engine probe",
    },
    CommandSuggestion {
        command: "/update",
        description: "Check or apply cryptographically verified update",
    },
    CommandSuggestion {
        command: "/config",
        description: "Display active zero-trust security configuration",
    },
    CommandSuggestion {
        command: "/help",
        description: "Display interactive command reference",
    },
    CommandSuggestion {
        command: "/exit",
        description: "Exit interactive session",
    },
];

/// Quick Actions featured on the clean home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickAction {
    pub label: &'static str,
    pub command: &'static str,
    pub description: &'static str,
}

pub const QUICK_ACTIONS: &[QuickAction] = &[
    QuickAction {
        label: "Extract archive",
        command: "/extract",
        description: "Extract archive with boundary verification",
    },
    QuickAction {
        label: "Test integrity",
        command: "/test",
        description: "Test integrity without disk modifications",
    },
    QuickAction {
        label: "Archive info",
        command: "/info",
        description: "Display platform, engine, and security details",
    },
    QuickAction {
        label: "Security diagnostics",
        command: "/doctor",
        description: "Run diagnostic health check and engine probe",
    },
    QuickAction {
        label: "Check for updates",
        command: "/update",
        description: "Check or apply cryptographically verified update",
    },
    QuickAction {
        label: "Security config",
        command: "/config",
        description: "Display active zero-trust security configuration",
    },
    QuickAction {
        label: "Help & reference",
        command: "/help",
        description: "Display interactive command reference",
    },
    QuickAction {
        label: "Exit session",
        command: "/exit",
        description: "Exit interactive session",
    },
];

/// Filters suggestions according to prefix (case-insensitive).
#[must_use]
pub fn filter_suggestions(input: &str) -> Vec<&'static CommandSuggestion> {
    let clean = input.trim();
    if clean.is_empty() {
        return SUGGESTIONS.iter().collect();
    }

    SUGGESTIONS
        .iter()
        .filter(|s| {
            s.command.starts_with(clean)
                || s.command
                    .strip_prefix('/')
                    .is_some_and(|unprefixed| unprefixed.starts_with(clean.trim_start_matches('/')))
        })
        .collect()
}

/// Determines if an input string is targeting the command palette rather than a filesystem path.
/// Command palette inputs start with a leading slash and do not contain path separators (`/` or `\`)
/// or spaces afterwards (e.g. `/`, `/ex`, `/extract`, `/help`).
#[must_use]
pub fn is_command_palette_input(input: &str) -> bool {
    let clean = input.trim();
    if !clean.starts_with('/') {
        return false;
    }
    // Any second slash or backslash indicates a filesystem path (e.g. /Users/..., /tmp/...)
    if clean[1..].contains('/') || clean.contains('\\') {
        return false;
    }
    // Command palette inputs do not contain whitespace
    if clean.contains(' ') {
        return false;
    }
    true
}

/// Finds supported archive files directly inside a directory (single level, bounded non-recursive scan).
/// Returns a sorted list of paths so multi-part archives (.part01.rar, .001, etc.) have part 1 first.
#[must_use]
pub fn find_archives_in_directory(dir: &Path) -> Vec<PathBuf> {
    let mut archives = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for (i, entry) in entries.flatten().enumerate() {
            if i >= 256 {
                break;
            }
            let path = entry.path();
            if path.is_file() && ArchiveFormat::from_extension(&path).is_some() {
                archives.push(path);
                if archives.len() >= 64 {
                    break;
                }
            }
        }
    }
    archives.sort();
    archives
}

/// Resolves a target path: if it is a directory, finds the primary archive within it.
/// Returns Ok(archive_path) if resolved, or Err(display_name) if directory contains no archives.
pub fn resolve_archive_or_directory(path: &Path) -> std::result::Result<PathBuf, String> {
    if path.is_dir() {
        let archives = find_archives_in_directory(path);
        if let Some(first) = archives.first() {
            Ok(first.clone())
        } else {
            Err(path.display().to_string())
        }
    } else {
        Ok(path.to_path_buf())
    }
}

/// Resolves a raw candidate string to a `PathBuf`, unescaping drag-and-drop artifacts
/// and expanding `~` to the home directory if present.
#[must_use]
pub fn resolve_candidate_path(input: &str) -> PathBuf {
    let clean = clean_terminal_path(input);
    if clean.starts_with("~/") || clean == "~" {
        if let Ok(home) = std::env::var("HOME") {
            if clean == "~" {
                return PathBuf::from(home);
            }
            return PathBuf::from(home).join(&clean[2..]);
        }
    }
    PathBuf::from(clean)
}

/// Checks if an input string syntactically or structurally resembles a filesystem path.
#[must_use]
pub fn looks_like_path(s: &str) -> bool {
    let clean = s.trim();
    if clean.is_empty() {
        return false;
    }
    clean.starts_with('/')
        || clean.starts_with("./")
        || clean.starts_with("../")
        || clean.starts_with("~/")
        || clean.contains('/')
        || clean.contains('\\')
        || clean.ends_with(".zip")
        || clean.ends_with(".rar")
        || clean.ends_with(".7z")
        || clean.ends_with(".tar")
        || clean.ends_with(".gz")
        || clean.ends_with(".bz2")
        || clean.ends_with(".xz")
        || clean.ends_with(".zst")
        || clean.ends_with(".iso")
        || Path::new(clean).exists()
}

/// Classified status of an input candidate path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathCandidate {
    /// A single supported archive file.
    ArchiveFile {
        path: PathBuf,
        format: ArchiveFormat,
    },
    /// A directory containing one or more supported archive files.
    ArchiveInDirectory {
        directory: PathBuf,
        primary_archive: PathBuf,
        total_archives: usize,
        format: ArchiveFormat,
    },
    /// A directory containing no supported archive files.
    EmptyDirectory(PathBuf),
    /// A regular file with an unsupported format.
    UnsupportedFile(PathBuf),
    /// A path that resembles a filesystem path but does not exist on disk.
    NotFound(PathBuf),
    /// An input that is not a filesystem path (e.g. command or arbitrary text).
    NotAPath,
}

/// Inspects and classifies an input candidate string against the filesystem.
#[must_use]
pub fn inspect_path_candidate(input: &str) -> PathCandidate {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return PathCandidate::NotAPath;
    }

    // Command palette inputs (e.g. "/", "/help", "/extract") take precedence over paths
    if is_command_palette_input(trimmed) {
        return PathCandidate::NotAPath;
    }

    let path = resolve_candidate_path(trimmed);
    if path.is_file() {
        if let Some(format) = ArchiveFormat::from_extension(&path) {
            PathCandidate::ArchiveFile { path, format }
        } else {
            PathCandidate::UnsupportedFile(path)
        }
    } else if path.is_dir() {
        let archives = find_archives_in_directory(&path);
        if let Some(primary) = archives.first() {
            let format = ArchiveFormat::from_extension(primary).unwrap_or(ArchiveFormat::Zip);
            PathCandidate::ArchiveInDirectory {
                directory: path,
                primary_archive: primary.clone(),
                total_archives: archives.len(),
                format,
            }
        } else {
            PathCandidate::EmptyDirectory(path)
        }
    } else if looks_like_path(trimmed) {
        PathCandidate::NotFound(path)
    } else {
        PathCandidate::NotAPath
    }
}

/// Action options when an archive file is detected in the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveAction {
    Extract,
    Test,
    Cancel,
}

/// RAII guard ensuring terminal alternate screen and raw mode are restored on exit or panic.
struct AlternateScreenGuard;

impl Drop for AlternateScreenGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = crossterm::execute!(
            out,
            crossterm::cursor::Show,
            crossterm::terminal::LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

/// Returns the styled interactive prompt prefix.
#[must_use]
pub fn prompt_prefix() -> &'static str {
    if std::env::var_os("NO_COLOR").is_some() {
        "unarc › "
    } else {
        "unarc \x1b[1;36m›\x1b[0m "
    }
}

/// Truncates string to `max_len` unicode characters, appending an ellipsis if truncated.
#[must_use]
pub fn truncate_chars(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let mut res: String = s.chars().take(max_len - 1).collect();
        res.push('…');
        res
    }
}

/// Layout geometry and spacing system for the Unarc TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiLayout {
    pub term_width: usize,
    pub term_height: usize,
    pub content_width: usize,
    pub left_margin: usize,
    pub color_enabled: bool,
}

/// Backward compatibility alias for `TuiLayout`.
pub type LayoutGrid = TuiLayout;

impl TuiLayout {
    /// Constructs a responsive layout geometry tailored to current terminal dimensions.
    #[must_use]
    pub fn new(term_width: usize, term_height: usize, color_enabled: bool) -> Self {
        let content_width = if term_width >= 96 {
            80.min(term_width.saturating_sub(12))
        } else if term_width >= 72 {
            term_width.saturating_sub(6)
        } else if term_width >= 54 {
            term_width.saturating_sub(4)
        } else {
            term_width.saturating_sub(2).max(26)
        };

        let left_margin = (term_width.saturating_sub(content_width)) / 2;

        Self {
            term_width,
            term_height,
            content_width,
            left_margin,
            color_enabled,
        }
    }

    /// Renders content padded with the layout's left margin.
    #[must_use]
    pub fn row(&self, content: &str) -> String {
        if self.left_margin == 0 {
            content.to_string()
        } else {
            format!("{}{content}", " ".repeat(self.left_margin))
        }
    }

    /// Centers text horizontally across the full terminal window, measuring visible characters only.
    #[must_use]
    pub fn center(&self, visible_text: &str, styled_text: &str) -> String {
        let vis_len = visible_text.chars().count();
        if vis_len >= self.term_width {
            truncate_chars(visible_text, self.term_width)
        } else {
            let pad = (self.term_width - vis_len) / 2;
            format!("{}{styled_text}", " ".repeat(pad))
        }
    }

    /// Centers a plain string across the terminal.
    #[must_use]
    pub fn center_plain(&self, text: &str) -> String {
        self.center(text, text)
    }

    /// Renders a subtle divider rule spanning exactly the content width.
    #[must_use]
    pub fn divider(&self) -> String {
        let sym = if self.color_enabled { "─" } else { "-" };
        let rule = sym.repeat(self.content_width);
        if self.color_enabled {
            self.row(&format!("\x1b[90m{rule}\x1b[0m"))
        } else {
            self.row(&rule)
        }
    }

    /// Renders a restrained, centered header divider rule.
    #[must_use]
    pub fn header_divider(&self) -> String {
        let sym = if self.color_enabled { "─" } else { "-" };
        let len = 24.min(self.content_width);
        let rule = sym.repeat(len);
        if self.color_enabled {
            self.center(&rule, &format!("\x1b[90m{rule}\x1b[0m"))
        } else {
            self.center_plain(&rule)
        }
    }

    /// Formats minimal, contextual key hints cleanly aligned on the content margin.
    /// Automatically fits as many key hints as possible within the content width.
    #[must_use]
    pub fn format_hints(&self, hints: &[(&str, &str)]) -> String {
        let sep = if self.content_width < 60 {
            "  "
        } else {
            "    "
        };
        let mut included = Vec::new();
        let mut current_len = 0;

        for (key, action) in hints {
            let item_plain = format!("{key}  {action}");
            let item_len = item_plain.chars().count();
            let added_len = if included.is_empty() {
                item_len
            } else {
                sep.len() + item_len
            };

            if current_len + added_len <= self.content_width {
                let styled = if self.color_enabled {
                    format!("\x1b[1;37m{key}\x1b[0m  \x1b[90m{action}\x1b[0m")
                } else {
                    item_plain
                };
                included.push(styled);
                current_len += added_len;
            } else if included.is_empty() {
                let trunc = truncate_chars(&item_plain, self.content_width);
                included.push(trunc);
                break;
            } else {
                break;
            }
        }

        let joined = included.join(sep);
        self.row(&joined)
    }
}

/// Renders the prominent UNARC brand and runtime metadata (centered).
#[must_use]
pub fn render_brand_header(info: &AppInfo, layout: &TuiLayout) -> Vec<String> {
    let version = &info.version;
    let engine_ver = &info.engine.pinned_version;

    let os_desc = if info.platform.is_apple_silicon {
        "macOS • arm64".to_string()
    } else {
        format!("{} • {}", info.platform.os, info.platform.arch)
    };

    let sandbox_status = if info.platform.capabilities.supports_sandbox_confinement {
        "ENFORCED"
    } else {
        "ACTIVE"
    };

    let mut rows = Vec::with_capacity(5);

    // Line 1: UNARC brand (visually dominant bold bright white with cyan version)
    let brand_vis = format!("UNARC {version}");
    let brand_styled = if layout.color_enabled {
        format!("\x1b[1;37mUNARC\x1b[0m \x1b[1;36m{version}\x1b[0m")
    } else {
        brand_vis.clone()
    };
    rows.push(layout.center(&brand_vis, &brand_styled));

    // Line 2: Developer / maintainer reference
    let author_vis = "by Mosabbir Maruf".to_string();
    let author_styled = if layout.color_enabled {
        "\x1b[90mby \x1b[37mMosabbir Maruf\x1b[0m".to_string()
    } else {
        author_vis.clone()
    };
    rows.push(layout.center(&author_vis, &author_styled));

    // Line 3: Compact platform metadata
    let os_styled = if layout.color_enabled {
        format!("\x1b[90m{os_desc}\x1b[0m")
    } else {
        os_desc.clone()
    };
    rows.push(layout.center(&os_desc, &os_styled));

    // Line 3: Engine and sandbox status (scaled responsively)
    let (engine_vis, engine_styled) = if layout.content_width < 34 {
        let vis = format!("7zz • {sandbox_status}");
        let styled = if layout.color_enabled {
            format!("\x1b[90m7zz • Sandbox \x1b[32m{sandbox_status}\x1b[0m")
        } else {
            vis.clone()
        };
        (vis, styled)
    } else if layout.content_width < 46 {
        let vis = format!("7zz {engine_ver} • {sandbox_status}");
        let styled = if layout.color_enabled {
            format!("\x1b[90m7zz {engine_ver} • \x1b[32m{sandbox_status}\x1b[0m")
        } else {
            vis.clone()
        };
        (vis, styled)
    } else {
        let vis = format!("7-Zip {engine_ver} • Sandbox {sandbox_status}");
        let styled = if layout.color_enabled {
            format!("\x1b[90m7-Zip {engine_ver} • Sandbox \x1b[32m{sandbox_status}\x1b[0m")
        } else {
            vis.clone()
        };
        (vis, styled)
    };
    rows.push(layout.center(&engine_vis, &engine_styled));

    // Line 4: Centered header separator
    rows.push(layout.header_divider());

    rows
}

/// Renders centered operation/state title and optional subtitle.
#[must_use]
pub fn render_operation_title(
    title: &str,
    subtitle: Option<&str>,
    layout: &TuiLayout,
) -> Vec<String> {
    let mut rows = Vec::with_capacity(2);

    let title_styled = if layout.color_enabled {
        format!("\x1b[1;37m{title}\x1b[0m")
    } else {
        title.to_string()
    };
    rows.push(layout.center(title, &title_styled));

    if let Some(sub) = subtitle {
        if !sub.is_empty() {
            let sub_styled = if layout.color_enabled {
                format!("\x1b[90m{sub}\x1b[0m")
            } else {
                sub.to_string()
            };
            rows.push(layout.center(sub, &sub_styled));
        }
    }

    rows
}

/// Backward-compatible grid header builder.
#[must_use]
pub fn build_grid_header(info: &AppInfo, layout: &TuiLayout, _color_enabled: bool) -> Vec<String> {
    render_brand_header(info, layout)
}

/// Backward-compatible header builder returning `(header_lines, divider)`.
#[must_use]
pub fn build_header(info: &AppInfo, term_width: usize, color_enabled: bool) -> (String, String) {
    let layout = TuiLayout::new(term_width, 24, color_enabled);
    let rows = render_brand_header(info, &layout);
    (rows.join("\n"), layout.divider())
}

/// Returns the human-readable operation title and optional slug for a given command.
#[must_use]
pub fn command_title_and_slug(cmd: &str) -> (&'static str, &'static str) {
    let trimmed = cmd.trim();
    if trimmed.starts_with("/info") || trimmed == "info" {
        ("Archive Information", "(/info)")
    } else if trimmed.starts_with("/doctor") || trimmed == "doctor" {
        ("Security Diagnostics", "(/doctor)")
    } else if trimmed.starts_with("/update") || trimmed == "update" {
        ("Self-Update", "(/update)")
    } else if trimmed.starts_with("/config") || trimmed == "config" {
        ("Security Configuration", "(/config)")
    } else if trimmed.starts_with("/help") || trimmed == "help" {
        ("Command Reference", "(/help)")
    } else if trimmed.starts_with("/extract") || trimmed.starts_with("extract") {
        ("Archive Extraction", "(/extract)")
    } else if trimmed.starts_with("/test") || trimmed.starts_with("test") {
        ("Archive Integrity Test", "(/test)")
    } else {
        ("Command Execution", "")
    }
}

/// Returns a human-friendly command title.
#[must_use]
pub fn command_title(cmd: &str) -> &'static str {
    let (title, _) = command_title_and_slug(cmd);
    title
}

/// View state passed to dashboard row generation.
#[derive(Debug, Clone, Copy)]
pub struct DashboardState<'a> {
    pub buffer: &'a str,
    pub selected_index: usize,
    pub home_selection: Option<usize>,
    pub archive_action: ArchiveAction,
    pub term_width: usize,
    pub term_height: usize,
    pub color_enabled: bool,
}

/// Generates all rows, cursor position (x, y), and cursor visibility for the dashboard.
#[must_use]
pub fn generate_dashboard_rows(
    app_info: &AppInfo,
    state: &DashboardState<'_>,
) -> (Vec<String>, (usize, usize), bool) {
    let layout = TuiLayout::new(state.term_width, state.term_height, state.color_enabled);
    let compact = state.term_height < 22;

    let mut screen_rows: Vec<String> = Vec::with_capacity(20);

    // 1. Prominent UNARC Brand & Metadata (Centered)
    let header_rows = render_brand_header(app_info, &layout);
    screen_rows.extend(header_rows);

    if !compact {
        screen_rows.push(String::new());
    }

    // 2. Check Candidate Status
    let candidate_status = inspect_path_candidate(state.buffer);
    let is_archive_detected = matches!(
        candidate_status,
        PathCandidate::ArchiveFile { .. } | PathCandidate::ArchiveInDirectory { .. }
    );

    // 3. Operation / State Title (Centered)
    match &candidate_status {
        PathCandidate::ArchiveFile { format, .. } => {
            let sub = format!("({format} Archive)");
            screen_rows.extend(render_operation_title(
                "Archive Detected",
                Some(&sub),
                &layout,
            ));
        }
        PathCandidate::ArchiveInDirectory {
            total_archives,
            format,
            ..
        } => {
            let sub = if *total_archives > 1 {
                format!("({total_archives} Archives in Folder)")
            } else {
                format!("({format} Archive in Folder)")
            };
            screen_rows.extend(render_operation_title(
                "Archive Detected",
                Some(&sub),
                &layout,
            ));
        }
        PathCandidate::EmptyDirectory(_) => {
            screen_rows.extend(render_operation_title(
                "Directory Detected",
                Some("(No Archives Found)"),
                &layout,
            ));
        }
        PathCandidate::UnsupportedFile(_) => {
            screen_rows.extend(render_operation_title(
                "File Detected",
                Some("(Unsupported Format)"),
                &layout,
            ));
        }
        PathCandidate::NotFound(_) => {
            screen_rows.extend(render_operation_title(
                "Path Not Found",
                Some("(Verify Path)"),
                &layout,
            ));
        }
        PathCandidate::NotAPath => {
            if is_command_palette_input(state.buffer) {
                screen_rows.extend(render_operation_title(
                    "Command Palette",
                    Some("(/help for reference)"),
                    &layout,
                ));
            } else {
                screen_rows.extend(render_operation_title(
                    "Archive Utility",
                    Some("(Drop archive or enter path)"),
                    &layout,
                ));
            }
        }
    }

    if !compact {
        screen_rows.push(String::new());
    }

    // 4. Left-Aligned Real Interaction Content
    let mut cursor_x: usize = layout.left_margin + 2;
    let mut cursor_y: usize = if compact { 7 } else { 9 };
    let mut hide_cursor = false;

    match &candidate_status {
        PathCandidate::ArchiveFile { path, format } => {
            hide_cursor = true;
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let size_str = crate::cli::output::format_byte_size(size_bytes);

            let sanitized_name = crate::cli::progress::sanitize_terminal_text(&filename);
            let sanitized_path =
                crate::cli::progress::sanitize_terminal_text(&path.display().to_string());
            let max_val_w = layout.content_width.saturating_sub(14);
            let disp_name = truncate_chars(&sanitized_name, max_val_w);
            let disp_path = crate::cli::progress::truncate_filename(&sanitized_path, max_val_w);

            let section_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mArchive Details\x1b[0m")
            } else {
                layout.row("Archive Details")
            };
            screen_rows.push(section_lbl);

            for (lbl, val) in [
                ("File:", disp_name.as_str()),
                ("Format:", &format!("{format} Archive")),
                ("Size:", size_str.as_str()),
                ("Location:", disp_path.as_str()),
            ] {
                let line = if state.color_enabled {
                    format!("  \x1b[90m{:<10}\x1b[0m \x1b[1;37m{val}\x1b[0m", lbl)
                } else {
                    format!("  {:<10} {val}", lbl)
                };
                screen_rows.push(layout.row(&line));
            }

            if !compact {
                screen_rows.push(String::new());
            }

            let action_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mAction\x1b[0m")
            } else {
                layout.row("Action")
            };
            screen_rows.push(action_lbl);

            for (act, label) in [
                (ArchiveAction::Extract, "[E]xtract archive"),
                (ArchiveAction::Test, "[T]est integrity"),
                (ArchiveAction::Cancel, "[C]ancel"),
            ] {
                let is_focused = state.archive_action == act;
                let row_str = if is_focused {
                    if state.color_enabled {
                        format!("  \x1b[1;36m›\x1b[0m \x1b[1;37m{label}\x1b[0m")
                    } else {
                        format!("  > {label}")
                    }
                } else if state.color_enabled {
                    format!("    \x1b[90m{label}\x1b[0m")
                } else {
                    format!("    {label}")
                };
                screen_rows.push(layout.row(&row_str));
            }
        }
        PathCandidate::ArchiveInDirectory {
            directory,
            primary_archive,
            total_archives,
            format,
        } => {
            hide_cursor = true;
            let filename = primary_archive
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let size_bytes = std::fs::metadata(primary_archive)
                .map(|m| m.len())
                .unwrap_or(0);
            let size_str = crate::cli::output::format_byte_size(size_bytes);

            let sanitized_name = crate::cli::progress::sanitize_terminal_text(&filename);
            let sanitized_dir =
                crate::cli::progress::sanitize_terminal_text(&directory.display().to_string());
            let max_val_w = layout.content_width.saturating_sub(14);
            let disp_name = truncate_chars(&sanitized_name, max_val_w);
            let disp_dir = crate::cli::progress::truncate_filename(&sanitized_dir, max_val_w);

            let section_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mArchive Details\x1b[0m")
            } else {
                layout.row("Archive Details")
            };
            screen_rows.push(section_lbl);

            let format_desc = if *total_archives > 1 {
                format!("{format} Archive ({total_archives} found)")
            } else {
                format!("{format} Archive")
            };

            for (lbl, val) in [
                ("File:", disp_name.as_str()),
                ("Format:", format_desc.as_str()),
                ("Size:", size_str.as_str()),
                ("Folder:", disp_dir.as_str()),
            ] {
                let line = if state.color_enabled {
                    format!("  \x1b[90m{:<10}\x1b[0m \x1b[1;37m{val}\x1b[0m", lbl)
                } else {
                    format!("  {:<10} {val}", lbl)
                };
                screen_rows.push(layout.row(&line));
            }

            if !compact {
                screen_rows.push(String::new());
            }

            let action_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mAction\x1b[0m")
            } else {
                layout.row("Action")
            };
            screen_rows.push(action_lbl);

            for (act, label) in [
                (ArchiveAction::Extract, "[E]xtract archive"),
                (ArchiveAction::Test, "[T]est integrity"),
                (ArchiveAction::Cancel, "[C]ancel"),
            ] {
                let is_focused = state.archive_action == act;
                let row_str = if is_focused {
                    if state.color_enabled {
                        format!("  \x1b[1;36m›\x1b[0m \x1b[1;37m{label}\x1b[0m")
                    } else {
                        format!("  > {label}")
                    }
                } else if state.color_enabled {
                    format!("    \x1b[90m{label}\x1b[0m")
                } else {
                    format!("    {label}")
                };
                screen_rows.push(layout.row(&row_str));
            }
        }
        PathCandidate::EmptyDirectory(dir) => {
            let max_val_w = layout.content_width.saturating_sub(14);
            let disp_dir =
                crate::cli::progress::truncate_filename(&dir.display().to_string(), max_val_w);

            let section_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mDirectory Details\x1b[0m")
            } else {
                layout.row("Directory Details")
            };
            screen_rows.push(section_lbl);
            for (lbl, val) in [
                ("Folder:", disp_dir.as_str()),
                ("Status:", "No supported archive files found in folder"),
            ] {
                let line = if state.color_enabled {
                    format!("  \x1b[90m{:<10}\x1b[0m \x1b[1;37m{val}\x1b[0m", lbl)
                } else {
                    format!("  {:<10} {val}", lbl)
                };
                screen_rows.push(layout.row(&line));
            }
            if !compact {
                screen_rows.push(String::new());
            }
            let hint_lbl = if state.color_enabled {
                layout.row(
                    "\x1b[90mHint: Drop an archive file (.zip, .rar, .7z, etc.) or a folder containing one.\x1b[0m",
                )
            } else {
                layout.row(
                    "Hint: Drop an archive file (.zip, .rar, .7z, etc.) or a folder containing one.",
                )
            };
            screen_rows.push(hint_lbl);
        }
        PathCandidate::UnsupportedFile(file) => {
            let max_val_w = layout.content_width.saturating_sub(14);
            let disp_file =
                crate::cli::progress::truncate_filename(&file.display().to_string(), max_val_w);

            let section_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mFile Details\x1b[0m")
            } else {
                layout.row("File Details")
            };
            screen_rows.push(section_lbl);
            for (lbl, val) in [
                ("File:", disp_file.as_str()),
                ("Status:", "Not a supported archive format"),
            ] {
                let line = if state.color_enabled {
                    format!("  \x1b[90m{:<10}\x1b[0m \x1b[1;37m{val}\x1b[0m", lbl)
                } else {
                    format!("  {:<10} {val}", lbl)
                };
                screen_rows.push(layout.row(&line));
            }
            if !compact {
                screen_rows.push(String::new());
            }
            let hint_lbl = if state.color_enabled {
                layout.row(
                    "\x1b[90mSupported formats: .7z, .zip, .rar, .tar, .gz, .bz2, .xz, .zst, .iso, .cab, .arj, .lzh, .wim\x1b[0m",
                )
            } else {
                layout.row(
                    "Supported formats: .7z, .zip, .rar, .tar, .gz, .bz2, .xz, .zst, .iso, .cab, .arj, .lzh, .wim",
                )
            };
            screen_rows.push(hint_lbl);
        }
        PathCandidate::NotFound(path) => {
            let max_val_w = layout.content_width.saturating_sub(14);
            let disp_path =
                crate::cli::progress::truncate_filename(&path.display().to_string(), max_val_w);

            let section_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mPath Details\x1b[0m")
            } else {
                layout.row("Path Details")
            };
            screen_rows.push(section_lbl);
            for (lbl, val) in [
                ("Path:", disp_path.as_str()),
                ("Status:", "File or directory does not exist"),
            ] {
                let line = if state.color_enabled {
                    format!("  \x1b[90m{:<10}\x1b[0m \x1b[1;37m{val}\x1b[0m", lbl)
                } else {
                    format!("  {:<10} {val}", lbl)
                };
                screen_rows.push(layout.row(&line));
            }
            if !compact {
                screen_rows.push(String::new());
            }
            let hint_lbl = if state.color_enabled {
                layout.row(
                    "\x1b[90mHint: Please verify the path or drop a file directly into the terminal.\x1b[0m",
                )
            } else {
                layout
                    .row("Hint: Please verify the path or drop a file directly into the terminal.")
            };
            screen_rows.push(hint_lbl);
        }
        PathCandidate::NotAPath if is_command_palette_input(state.buffer) => {
            let max_disp_buf = layout.content_width.saturating_sub(6);
            let disp_buf = truncate_chars(state.buffer, max_disp_buf);

            let prompt_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mCommand:\x1b[0m")
            } else {
                layout.row("Command:")
            };
            screen_rows.push(prompt_lbl);

            let prompt_sym = if state.color_enabled {
                "\x1b[1;36m›\x1b[0m"
            } else {
                ">"
            };
            screen_rows.push(layout.row(&format!("{prompt_sym} {disp_buf}")));
            cursor_x = layout.left_margin + 2 + disp_buf.chars().count();
            cursor_y = screen_rows.len().saturating_sub(1);

            if !compact {
                screen_rows.push(String::new());
            }

            let cmd_lbl = if state.color_enabled {
                layout.row("\x1b[90mSuggestions:\x1b[0m")
            } else {
                layout.row("Suggestions:")
            };
            screen_rows.push(cmd_lbl);

            let filtered = filter_suggestions(state.buffer);
            if filtered.is_empty() {
                let empty_msg = if state.color_enabled {
                    layout.row(
                        "  \x1b[90mNo matching commands found. Type '/help' for reference.\x1b[0m",
                    )
                } else {
                    layout.row("  No matching commands found. Type '/help' for reference.")
                };
                screen_rows.push(empty_msg);
            } else {
                let max_desc_w = layout.content_width.saturating_sub(18);
                for (i, item) in filtered.iter().enumerate() {
                    let desc = truncate_chars(item.description, max_desc_w);
                    let is_selected = i == state.selected_index;
                    let line = if is_selected {
                        if state.color_enabled {
                            format!(
                                "  \x1b[1;36m›\x1b[0m \x1b[1;36m{:<10}\x1b[0m  \x1b[1;37m{desc}\x1b[0m",
                                item.command
                            )
                        } else {
                            format!("  > {:<10}  {desc}", item.command)
                        }
                    } else if state.color_enabled {
                        format!(
                            "    \x1b[37m{:<10}\x1b[0m  \x1b[90m{desc}\x1b[0m",
                            item.command
                        )
                    } else {
                        format!("    {:<10}  {desc}", item.command)
                    };
                    screen_rows.push(layout.row(&line));
                }
            }
        }
        PathCandidate::NotAPath => {
            // Home Screen / General Input State
            let prompt_lbl = if state.color_enabled {
                layout.row("\x1b[1;37mEnter archive path:\x1b[0m")
            } else {
                layout.row("Enter archive path:")
            };
            screen_rows.push(prompt_lbl);

            let max_disp_buf = layout.content_width.saturating_sub(6);
            let disp_buf = truncate_chars(state.buffer, max_disp_buf);
            let prompt_sym = if state.color_enabled {
                "\x1b[1;36m›\x1b[0m"
            } else {
                ">"
            };
            screen_rows.push(layout.row(&format!("{prompt_sym} {disp_buf}")));

            cursor_x = layout.left_margin + 2 + disp_buf.chars().count();
            cursor_y = screen_rows.len().saturating_sub(1);

            if !compact {
                screen_rows.push(String::new());
            }

            if state.buffer.trim().is_empty() {
                let qa_lbl = if state.color_enabled {
                    layout.row("\x1b[90mQuick Actions:\x1b[0m")
                } else {
                    layout.row("Quick Actions:")
                };
                screen_rows.push(qa_lbl);

                let max_label_w = if layout.content_width < 54 {
                    layout.content_width.saturating_sub(16).min(18)
                } else {
                    22
                };

                for (i, action) in QUICK_ACTIONS.iter().enumerate() {
                    let is_selected = state.home_selection == Some(i);
                    let label_disp = truncate_chars(action.label, max_label_w);
                    let line = if is_selected {
                        if state.color_enabled {
                            format!(
                                "  \x1b[1;36m›\x1b[0m \x1b[1;37m{:<width$}\x1b[0m \x1b[90m{}\x1b[0m",
                                label_disp,
                                action.command,
                                width = max_label_w
                            )
                        } else {
                            format!(
                                "  > {:<width$} {}",
                                label_disp,
                                action.command,
                                width = max_label_w
                            )
                        }
                    } else if state.color_enabled {
                        format!(
                            "    \x1b[37m{:<width$}\x1b[0m \x1b[90m{}\x1b[0m",
                            label_disp,
                            action.command,
                            width = max_label_w
                        )
                    } else {
                        format!(
                            "    {:<width$} {}",
                            label_disp,
                            action.command,
                            width = max_label_w
                        )
                    };
                    screen_rows.push(layout.row(&line));
                }
            } else {
                let hint1 = if layout.content_width < 50 {
                    "Enter to extract path"
                } else {
                    "Press Enter to inspect or extract path"
                };
                let hint2 = if layout.content_width < 50 {
                    "'/' for commands"
                } else {
                    "Type '/' to open command palette"
                };
                let h1_line = if state.color_enabled {
                    format!("  \x1b[90m{hint1}\x1b[0m")
                } else {
                    format!("  {hint1}")
                };
                screen_rows.push(layout.row(&h1_line));
                let h2_line = if state.color_enabled {
                    format!("  \x1b[90m{hint2}\x1b[0m")
                } else {
                    format!("  {hint2}")
                };
                screen_rows.push(layout.row(&h2_line));
            }
        }
    }

    if !compact {
        screen_rows.push(String::new());
    }

    // 5. Divider & Contextual Footer
    screen_rows.push(layout.divider());

    let footer_row = if is_archive_detected {
        layout.format_hints(&[
            ("Enter", "Run"),
            ("E", "Extract"),
            ("T", "Test"),
            ("Esc", "Cancel"),
            ("Ctrl+C", "Exit"),
        ])
    } else if matches!(
        candidate_status,
        PathCandidate::EmptyDirectory(_)
            | PathCandidate::UnsupportedFile(_)
            | PathCandidate::NotFound(_)
    ) {
        layout.format_hints(&[
            ("Esc", "Clear"),
            ("Backspace", "Edit path"),
            ("Ctrl+C", "Exit"),
        ])
    } else if is_command_palette_input(state.buffer) {
        layout.format_hints(&[
            ("Enter", "Run"),
            ("Tab", "Complete"),
            ("↑↓", "Navigate"),
            ("Esc", "Clear"),
            ("Ctrl+C", "Exit"),
        ])
    } else if state.buffer.trim().is_empty() {
        layout.format_hints(&[
            ("Enter", "Select"),
            ("↑↓", "Navigate"),
            ("/", "Commands"),
            ("Ctrl+C", "Exit"),
        ])
    } else {
        layout.format_hints(&[
            ("Enter", "Run"),
            ("/", "Commands"),
            ("Esc", "Clear"),
            ("Ctrl+C", "Exit"),
        ])
    };

    screen_rows.push(footer_row);

    (screen_rows, (cursor_x, cursor_y), hide_cursor)
}

/// Renders the complete interactive TUI dashboard.
fn render_dashboard(
    app_info: &AppInfo,
    buffer: &str,
    selected_index: usize,
    home_selection: Option<usize>,
    archive_action: ArchiveAction,
    color_enabled: bool,
) -> std::io::Result<()> {
    let mut out = stdout();
    let (term_width_u16, term_height_u16) = crossterm::terminal::size().unwrap_or((80, 24));
    let term_width = term_width_u16 as usize;
    let term_height = term_height_u16 as usize;

    let state = DashboardState {
        buffer,
        selected_index,
        home_selection,
        archive_action,
        term_width,
        term_height,
        color_enabled,
    };

    let (screen_rows, (cursor_x, cursor_y), hide_cursor) =
        generate_dashboard_rows(app_info, &state);

    // Draw all rows cleanly
    crossterm::queue!(out, crossterm::cursor::MoveTo(0, 0))?;
    for row in screen_rows {
        write!(out, "\r\x1b[2K{row}\r\n")?;
    }
    crossterm::queue!(
        out,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
    )?;

    if hide_cursor {
        crossterm::queue!(out, crossterm::cursor::Hide)?;
    } else {
        crossterm::queue!(
            out,
            crossterm::cursor::MoveTo(cursor_x as u16, cursor_y as u16),
            crossterm::cursor::Show
        )?;
    }

    out.flush()?;
    Ok(())
}

/// Executes an interactive command in a dedicated execution view inside alternate screen buffer.
fn execute_command_in_tui(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    cmd_str: &str,
    app_info: &AppInfo,
) -> Result<bool> {
    let mut out = stdout();
    let (term_width_u16, term_height_u16) = crossterm::terminal::size().unwrap_or((80, 24));
    let term_width = term_width_u16 as usize;
    let term_height = term_height_u16 as usize;
    let color_enabled = std::env::var_os("NO_COLOR").is_none();
    let layout = TuiLayout::new(term_width, term_height, color_enabled);
    let compact = term_height < 22;

    crossterm::queue!(
        out,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;

    // 1. Draw Brand Header
    let header_rows = render_brand_header(app_info, &layout);
    for row in header_rows {
        writeln!(out, "{row}\r")?;
    }
    if !compact {
        writeln!(out, "\r")?;
    }

    // 2. Draw Centered Operation Title
    let (title, slug) = command_title_and_slug(cmd_str);
    let title_rows = render_operation_title(title, Some(slug), &layout);
    for row in title_rows {
        writeln!(out, "{row}\r")?;
    }
    if !compact {
        writeln!(out, "\r")?;
    }
    out.flush()?;

    // 3. Temporarily disable raw mode so standard streams, read_line, and progress work seamlessly
    let _ = disable_raw_mode();

    let cmd_res = run_command_in_tui_layout(app, formatter, cmd_str, &layout);
    if let Err(ref e) = cmd_res {
        let err_msg = format!("error: {e}");
        println!("{}", layout.row(&err_msg));
        if let Some(hint) = crate::cli::output::error_hint_for_error(e) {
            println!("{}", layout.row(&format!("Hint: {hint}")));
        }
        crate::platform::signals::reset_interrupted();
    }

    // 4. Print minimal contextual footer
    println!("\r\n{}", layout.divider());
    println!(
        "{}",
        layout.format_hints(&[
            ("Enter", "Return to dashboard"),
            ("Esc", "Back"),
            ("Ctrl+C", "Exit")
        ])
    );
    let _ = out.flush();

    // 5. Re-enable raw mode to wait for dismissal
    let _ = enable_raw_mode();

    loop {
        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                    {
                        return Ok(false);
                    }
                    match key.code {
                        KeyCode::Enter
                        | KeyCode::Esc
                        | KeyCode::Char('q')
                        | KeyCode::Char('Q')
                        | KeyCode::Char(' ') => {
                            return Ok(true);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Dispatches an interactive command for rendering within the centered TUI layout grid.
fn run_command_in_tui_layout(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    cmd_str: &str,
    layout: &TuiLayout,
) -> Result<()> {
    let trimmed = cmd_str.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    if trimmed == "/info" || trimmed == "info" {
        let info = app.app_info();
        println!(
            "{}",
            layout.row(&format!(
                "Unarc - Secure Archive Utility [v{}]",
                info.version
            ))
        );
        println!("{}", layout.row("Platform:"));
        let os_name = if info.platform.is_apple_silicon {
            "macos (Apple Silicon)".to_string()
        } else {
            format!("{} ({})", info.platform.os, info.platform.arch)
        };
        println!(
            "{}",
            layout.row(&format!("  {:<20} {}", "Operating System:", os_name))
        );
        println!(
            "{}",
            layout.row(&format!("  {:<20} {}", "Architecture:", info.platform.arch))
        );
        println!("{}", layout.row("Engine Status:"));
        println!(
            "{}",
            layout.row(&format!(
                "  {:<20} v{}",
                "Pinned 7-Zip:", info.engine.pinned_version
            ))
        );
        println!(
            "{}",
            layout.row(&format!(
                "  {:<20} {}",
                "Available:", info.engine.is_available
            ))
        );
        let max_w = layout.content_width.saturating_sub(24);
        let disp_hash = truncate_chars(&info.engine.expected_sha256, max_w);
        println!(
            "{}",
            layout.row(&format!("  {:<20} {}", "Expected SHA-256:", disp_hash))
        );
        if let Some(ref path) = info.engine.resolved_path {
            let disp_path =
                crate::cli::progress::truncate_filename(&path.display().to_string(), max_w);
            println!(
                "{}",
                layout.row(&format!("  {:<20} {}", "Binary Path:", disp_path))
            );
        }
        return Ok(());
    }

    if trimmed == "/doctor" || trimmed == "doctor" {
        let doc = app.doctor_check();
        println!("{}", layout.row("Diagnostic Health Probes:"));
        let max_desc_w = layout.content_width.saturating_sub(32);
        for check in &doc.checks {
            let badge = if check.passed {
                if layout.color_enabled {
                    "\x1b[1;32mPASS\x1b[0m"
                } else {
                    "PASS"
                }
            } else if layout.color_enabled {
                "\x1b[1;31mFAIL\x1b[0m"
            } else {
                "FAIL"
            };
            let disp_msg = truncate_chars(&check.message, max_desc_w);
            println!(
                "{}",
                layout.row(&format!("  [{badge}] {:<24} {disp_msg}", check.name))
            );
        }
        let status_str = if doc.healthy {
            if layout.color_enabled {
                "\x1b[1;32mAll security & health checks passed\x1b[0m"
            } else {
                "All security & health checks passed"
            }
        } else if layout.color_enabled {
            "\x1b[1;31mOne or more diagnostic checks failed\x1b[0m"
        } else {
            "One or more diagnostic checks failed"
        };
        println!();
        println!("{}", layout.row(&format!("Result: {status_str}")));
        return Ok(());
    }

    if trimmed == "/config" || trimmed == "config" {
        let policy = app.policy();
        println!("{}", layout.row("Zero-Trust Security Configuration:"));
        println!(
            "{}",
            layout.row(&format!(
                "  {:<26} {} bytes",
                "Max Path Length:", policy.max_path_length
            ))
        );
        println!(
            "{}",
            layout.row(&format!(
                "  {:<26} {}",
                "Max Path Depth:", policy.max_path_depth
            ))
        );
        println!(
            "{}",
            layout.row(&format!(
                "  {:<26} {}",
                "Allow Absolute Paths:", policy.allow_absolute_paths
            ))
        );
        println!(
            "{}",
            layout.row(&format!(
                "  {:<26} {}",
                "Allow Symlinks:", policy.allow_symlinks
            ))
        );
        println!(
            "{}",
            layout.row(&format!(
                "  {:<26} {}",
                "Strict Kernel Sandbox:", policy.require_kernel_sandbox
            ))
        );
        return Ok(());
    }

    if trimmed == "/help" || trimmed == "help" {
        println!("{}", layout.row("Command Reference:"));
        let max_w = layout.content_width.saturating_sub(18);
        for item in SUGGESTIONS {
            let desc = truncate_chars(item.description, max_w);
            if layout.color_enabled {
                println!(
                    "{}",
                    layout.row(&format!(
                        "  \x1b[1;36m{:<12}\x1b[0m  \x1b[37m{desc}\x1b[0m",
                        item.command
                    ))
                );
            } else {
                println!("{}", layout.row(&format!("  {:<12}  {desc}", item.command)));
            }
        }
        return Ok(());
    }

    if trimmed == "/update" || trimmed == "update" {
        let manager = crate::core::update::UpdateManager::default();
        println!(
            "{}",
            layout.row("Checking for cryptographically signed updates...")
        );
        match manager.check_for_update(None) {
            Ok(check_res) => {
                println!(
                    "{}",
                    layout.row(&format!(
                        "  {:<18} {}",
                        "Current Version:", check_res.current_version
                    ))
                );
                println!(
                    "{}",
                    layout.row(&format!(
                        "  {:<18} {}",
                        "Latest Version:", check_res.latest_version
                    ))
                );
                if check_res.update_available {
                    println!();
                    println!(
                        "{}",
                        layout.row("An update is available. Download and apply? [y/N]:")
                    );
                    print!("{}", layout.row("› "));
                    let _ = stdout().flush();
                    let mut ans = String::new();
                    stdin().read_line(&mut ans)?;
                    if ans.trim().eq_ignore_ascii_case("y") {
                        println!("{}", layout.row("Applying update..."));
                        match manager.apply_update(None, None) {
                            Ok(apply_res) => {
                                println!(
                                    "{}",
                                    layout.row(&format!(
                                        "✓ Updated successfully to v{}",
                                        apply_res.new_version
                                    ))
                                );
                            }
                            Err(e) => {
                                println!("{}", layout.row(&format!("Update error: {e}")));
                            }
                        }
                    }
                } else {
                    println!();
                    println!("{}", layout.row("Result: Unarc is currently up to date."));
                }
            }
            Err(e) => {
                println!("{}", layout.row(&format!("Update check failed: {e}")));
            }
        }
        return Ok(());
    }

    if trimmed == "/extract"
        || trimmed.starts_with("/extract ")
        || trimmed == "extract"
        || trimmed.starts_with("extract ")
    {
        let remainder = if let Some(rest) = trimmed.strip_prefix("/extract") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("extract") {
            rest.trim()
        } else {
            ""
        };
        let (arch, dest) = parse_archive_and_dest(remainder);
        return run_extract_in_tui_layout(app, formatter, arch, dest, layout);
    }

    if trimmed == "/test"
        || trimmed.starts_with("/test ")
        || trimmed == "test"
        || trimmed.starts_with("test ")
    {
        let remainder = if let Some(rest) = trimmed.strip_prefix("/test") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("test") {
            rest.trim()
        } else {
            ""
        };
        let (arch, _) = parse_archive_and_dest(remainder);
        return run_test_in_tui_layout(app, formatter, arch, layout);
    }

    // Centered layout fallback for archive paths, directory inspection, and commands
    match inspect_path_candidate(trimmed) {
        PathCandidate::ArchiveFile { path, .. }
        | PathCandidate::ArchiveInDirectory {
            primary_archive: path,
            ..
        } => {
            let p_str = path.to_string_lossy().to_string();
            run_extract_in_tui_layout(app, formatter, Some(p_str), None, layout)
        }
        PathCandidate::EmptyDirectory(dir) => {
            let max_w = layout.content_width.saturating_sub(14);
            let disp = crate::cli::progress::truncate_filename(&dir.display().to_string(), max_w);
            println!(
                "{}",
                layout.row(&format!("Directory contains no supported archives: {disp}"))
            );
            println!(
                "{}",
                layout.row("Hint: Drag or specify an archive file (.zip, .rar, .7z, etc.) or a folder containing one.")
            );
            Ok(())
        }
        PathCandidate::UnsupportedFile(file) => {
            let max_w = layout.content_width.saturating_sub(14);
            let disp = crate::cli::progress::truncate_filename(&file.display().to_string(), max_w);
            println!("{}", layout.row(&format!("Unsupported file type: {disp}")));
            println!(
                "{}",
                layout.row("Supported formats: .7z, .zip, .rar, .tar, .gz, .bz2, .xz, .zst, .iso, .cab, .arj, .lzh, .wim")
            );
            Ok(())
        }
        PathCandidate::NotFound(path) => {
            let max_w = layout.content_width.saturating_sub(14);
            let disp = crate::cli::progress::truncate_filename(&path.display().to_string(), max_w);
            println!("{}", layout.row(&format!("Path not found: '{disp}'")));
            println!(
                "{}",
                layout.row("Hint: Please verify the file or directory exists.")
            );
            Ok(())
        }
        PathCandidate::NotAPath => {
            let sanitized = crate::cli::progress::sanitize_terminal_text(trimmed);
            println!(
                "{}",
                layout.row(&format!(
                    "Unrecognized command: '{sanitized}'. Type '/help' for available commands."
                ))
            );
            Ok(())
        }
    }
}

fn run_test_in_tui_layout(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    inline_archive: Option<String>,
    layout: &TuiLayout,
) -> Result<()> {
    let archive_str = match inline_archive {
        Some(path) if !path.trim().is_empty() => clean_terminal_path(&path),
        _ => {
            println!("{}", layout.row("Enter archive path:"));
            print!("{}", layout.row("› "));
            let _ = stdout().flush();
            let mut line = String::new();
            stdin().read_line(&mut line)?;
            let clean = clean_terminal_path(&line);
            if clean.is_empty() {
                println!("{}", layout.row("Operation cancelled."));
                return Ok(());
            }
            clean
        }
    };
    let raw_archive_path = PathBuf::from(archive_str);
    let archive_path = match resolve_archive_or_directory(&raw_archive_path) {
        Ok(p) => p,
        Err(dir_disp) => {
            let max_w = layout.content_width.saturating_sub(14);
            let disp = crate::cli::progress::truncate_filename(&dir_disp, max_w);
            println!(
                "{}",
                layout.row(&format!("Directory contains no supported archives: {disp}"))
            );
            println!(
                "{}",
                layout.row("Hint: Drag or specify an archive file (.zip, .rar, .7z, etc.) or a folder containing one.")
            );
            return Ok(());
        }
    };
    let max_w = layout.content_width.saturating_sub(14);
    let disp_path =
        crate::cli::progress::truncate_filename(&archive_path.display().to_string(), max_w);
    println!("{}", layout.row(&format!("Testing archive: {disp_path}")));

    let show_progress = formatter.should_show_progress();
    let res = crate::cli::run_test_with_prompt(
        app,
        &archive_path,
        &crate::cli::TerminalPasswordPrompter,
        show_progress,
        layout.left_margin,
    )?;

    let status_badge = if res.passed {
        if layout.color_enabled {
            "\x1b[1;32mPASS\x1b[0m"
        } else {
            "PASS"
        }
    } else if layout.color_enabled {
        "\x1b[1;31mFAIL\x1b[0m"
    } else {
        "FAIL"
    };
    println!();
    println!(
        "{}",
        layout.row(&format!("Result: [{status_badge}] {}", res.message))
    );
    println!(
        "{}",
        layout.row(&format!("  {:<12} {}", "Format:", res.format))
    );
    if let Some(count) = res.entries_checked {
        println!("{}", layout.row(&format!("  {:<12} {}", "Entries:", count)));
    }
    Ok(())
}

fn run_extract_in_tui_layout(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    inline_archive: Option<String>,
    inline_dest: Option<String>,
    layout: &TuiLayout,
) -> Result<()> {
    let archive_str = match inline_archive {
        Some(path) if !path.trim().is_empty() => clean_terminal_path(&path),
        _ => {
            println!("{}", layout.row("Enter archive path:"));
            print!("{}", layout.row("› "));
            let _ = stdout().flush();
            let mut archive_line = String::new();
            stdin().read_line(&mut archive_line)?;
            let clean_archive = clean_terminal_path(&archive_line);
            if clean_archive.is_empty() {
                println!("{}", layout.row("Operation cancelled."));
                return Ok(());
            }
            clean_archive
        }
    };
    let raw_archive_path = PathBuf::from(archive_str);
    let archive_path = match resolve_archive_or_directory(&raw_archive_path) {
        Ok(p) => p,
        Err(dir_disp) => {
            let max_w = layout.content_width.saturating_sub(14);
            let disp = crate::cli::progress::truncate_filename(&dir_disp, max_w);
            println!(
                "{}",
                layout.row(&format!("Directory contains no supported archives: {disp}"))
            );
            println!(
                "{}",
                layout.row("Hint: Drag or specify an archive file (.zip, .rar, .7z, etc.) or a folder containing one.")
            );
            return Ok(());
        }
    };

    let dest_opt_buf: Option<PathBuf> = match inline_dest {
        Some(dest) if !dest.trim().is_empty() => Some(PathBuf::from(clean_terminal_path(&dest))),
        Some(_) => None,
        None => {
            println!(
                "{}",
                layout.row("Enter destination directory (leave empty for current directory):")
            );
            print!("{}", layout.row("› "));
            let _ = stdout().flush();
            let mut dest_line = String::new();
            stdin().read_line(&mut dest_line)?;
            let clean_dest = clean_terminal_path(&dest_line);
            if clean_dest.is_empty() {
                None
            } else {
                Some(PathBuf::from(clean_dest))
            }
        }
    };

    let dest_opt: Option<&Path> = dest_opt_buf.as_deref();
    let max_w = layout.content_width.saturating_sub(14);
    let disp_path =
        crate::cli::progress::truncate_filename(&archive_path.display().to_string(), max_w);
    println!(
        "{}",
        layout.row(&format!("Extracting archive: {disp_path}"))
    );

    let show_progress = formatter.should_show_progress();
    let res = crate::cli::run_extract_with_prompt(
        app,
        &archive_path,
        dest_opt,
        &crate::cli::TerminalPasswordPrompter,
        show_progress,
        layout.left_margin,
    )?;

    let status_badge = if layout.color_enabled {
        "\x1b[1;32mSUCCESS\x1b[0m"
    } else {
        "SUCCESS"
    };
    println!();
    println!(
        "{}",
        layout.row(&format!("Result: [{status_badge}] Extraction complete"))
    );
    let disp_dest =
        crate::cli::progress::truncate_filename(&res.destination.display().to_string(), max_w);
    println!(
        "{}",
        layout.row(&format!("  {:<14} {}", "Destination:", disp_dest))
    );
    println!(
        "{}",
        layout.row(&format!("  {:<14} {}", "Format:", res.format))
    );
    if let Some(entries) = res.entries_extracted {
        println!(
            "{}",
            layout.row(&format!("  {:<14} {}", "Entries:", entries))
        );
    }
    Ok(())
}

/// Runs the interactive shell.
pub fn run_interactive(
    app: &Application,
    quiet: bool,
    verbose: bool,
    json_mode: bool,
) -> Result<()> {
    let formatter = crate::cli::output::OutputFormatter::new(json_mode, quiet, verbose);

    // Non-TTY fallback for piped or automated execution
    if !stdin().is_terminal() || !stdout().is_terminal() || json_mode || quiet {
        if !quiet && !json_mode {
            println!("{ASCII_BANNER}");
            println!("Type '/' to view commands, or '/help' for usage guidance.");
        }
        return run_non_tty(app, &formatter);
    }

    run_terminal_loop(app, &formatter)
}

/// Non-TTY line-based loop.
fn run_non_tty(app: &Application, formatter: &crate::cli::output::OutputFormatter) -> Result<()> {
    let mut line = String::new();
    while stdin().read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }
        if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
            break;
        }

        let cmd = trimmed.to_string();
        line.clear();
        execute_interactive_command(app, formatter, &cmd)?;
    }

    Ok(())
}

/// TTY interactive loop supporting raw mode, full-screen alternate buffer, palette, and drag-and-drop.
fn run_terminal_loop(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
) -> Result<()> {
    let mut out = stdout();
    crossterm::execute!(
        out,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Show
    )?;
    enable_raw_mode().map_err(|e| {
        crate::error::UnarcError::Platform(crate::error::PlatformError::Unsupported {
            details: format!("Failed to enable terminal raw mode: {e}"),
        })
    })?;
    let _guard = AlternateScreenGuard;

    let app_info = app.app_info();
    let color_enabled = std::env::var_os("NO_COLOR").is_none();

    let mut buffer = String::new();
    let mut selected_index = 0usize;
    let mut home_selection: Option<usize> = None;
    let mut archive_action = ArchiveAction::Extract;

    render_dashboard(
        &app_info,
        &buffer,
        selected_index,
        home_selection,
        archive_action,
        color_enabled,
    )?;

    loop {
        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }

                    // Global exit shortcut
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                    {
                        break;
                    }

                    // Check archive detection state
                    let candidate_status = inspect_path_candidate(&buffer);
                    let target_archive: Option<PathBuf> = match &candidate_status {
                        PathCandidate::ArchiveFile { path, .. } => Some(path.clone()),
                        PathCandidate::ArchiveInDirectory {
                            primary_archive, ..
                        } => Some(primary_archive.clone()),
                        _ => None,
                    };

                    if let Some(archive_path) = target_archive {
                        let candidate_str = archive_path.to_string_lossy().to_string();
                        match key.code {
                            KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                                archive_action = match archive_action {
                                    ArchiveAction::Extract => ArchiveAction::Test,
                                    ArchiveAction::Test => ArchiveAction::Cancel,
                                    ArchiveAction::Cancel => ArchiveAction::Extract,
                                };
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    home_selection,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Up | KeyCode::Left => {
                                archive_action = match archive_action {
                                    ArchiveAction::Extract => ArchiveAction::Cancel,
                                    ArchiveAction::Test => ArchiveAction::Extract,
                                    ArchiveAction::Cancel => ArchiveAction::Test,
                                };
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    home_selection,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Char('e')
                            | KeyCode::Char('E')
                            | KeyCode::Char('t')
                            | KeyCode::Char('T')
                            | KeyCode::Char('c')
                            | KeyCode::Char('C')
                            | KeyCode::Esc
                            | KeyCode::Enter => {
                                let act = match key.code {
                                    KeyCode::Char('e') | KeyCode::Char('E') => {
                                        ArchiveAction::Extract
                                    }
                                    KeyCode::Char('t') | KeyCode::Char('T') => ArchiveAction::Test,
                                    KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                                        ArchiveAction::Cancel
                                    }
                                    _ => archive_action,
                                };
                                match act {
                                    ArchiveAction::Extract => {
                                        let continue_app = execute_command_in_tui(
                                            app,
                                            formatter,
                                            &format!("/extract \"{candidate_str}\""),
                                            &app_info,
                                        )?;
                                        if !continue_app {
                                            break;
                                        }
                                    }
                                    ArchiveAction::Test => {
                                        let continue_app = execute_command_in_tui(
                                            app,
                                            formatter,
                                            &format!("/test \"{candidate_str}\""),
                                            &app_info,
                                        )?;
                                        if !continue_app {
                                            break;
                                        }
                                    }
                                    ArchiveAction::Cancel => {}
                                }
                                buffer.clear();
                                selected_index = 0;
                                home_selection = None;
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    home_selection,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Backspace => {
                                buffer.pop();
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    home_selection,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Char(c) => {
                                buffer.push(c);
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    home_selection,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            _ => {}
                        }
                    }

                    // Command Palette / Default text input handling
                    match key.code {
                        KeyCode::Char(c) => {
                            buffer.push(c);
                            home_selection = None;
                            selected_index = 0;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Backspace => {
                            buffer.pop();
                            home_selection = None;
                            selected_index = 0;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Up => {
                            if is_command_palette_input(&buffer) {
                                selected_index = selected_index.saturating_sub(1);
                            } else if buffer.trim().is_empty() {
                                home_selection = match home_selection {
                                    Some(0) => None,
                                    Some(n) => Some(n - 1),
                                    None => Some(QUICK_ACTIONS.len().saturating_sub(1)),
                                };
                            }
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Down => {
                            if is_command_palette_input(&buffer) {
                                let filtered = filter_suggestions(&buffer);
                                if !filtered.is_empty() && selected_index + 1 < filtered.len() {
                                    selected_index += 1;
                                }
                            } else if buffer.trim().is_empty() {
                                home_selection = match home_selection {
                                    None => Some(0),
                                    Some(n) if n + 1 < QUICK_ACTIONS.len() => Some(n + 1),
                                    Some(_) => None,
                                };
                            }
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Tab => {
                            if is_command_palette_input(&buffer) {
                                let filtered = filter_suggestions(&buffer);
                                if !filtered.is_empty() {
                                    let idx = selected_index.min(filtered.len() - 1);
                                    buffer = filtered[idx].command.to_string();
                                    selected_index = 0;
                                }
                            } else if buffer.trim().is_empty() {
                                if let Some(idx) = home_selection {
                                    buffer = QUICK_ACTIONS[idx].command.to_string();
                                    home_selection = None;
                                }
                            }
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Enter => {
                            let trimmed = buffer.trim();
                            if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
                                break;
                            }

                            let cmd_to_run = if trimmed.is_empty() {
                                if let Some(idx) = home_selection {
                                    QUICK_ACTIONS[idx].command.to_string()
                                } else {
                                    String::new()
                                }
                            } else if is_command_palette_input(&buffer) {
                                let filtered = filter_suggestions(&buffer);
                                if !filtered.is_empty() {
                                    let idx = selected_index.min(filtered.len() - 1);
                                    filtered[idx].command.to_string()
                                } else {
                                    buffer.clone()
                                }
                            } else {
                                buffer.clone()
                            };

                            let trimmed_cmd = cmd_to_run.trim();
                            if trimmed_cmd == "/exit"
                                || trimmed_cmd == "exit"
                                || trimmed_cmd == "quit"
                            {
                                break;
                            }

                            if !trimmed_cmd.is_empty() {
                                let continue_app =
                                    execute_command_in_tui(app, formatter, trimmed_cmd, &app_info)?;
                                if !continue_app {
                                    break;
                                }
                            }

                            buffer.clear();
                            selected_index = 0;
                            home_selection = None;
                            archive_action = ArchiveAction::Extract;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Esc => {
                            buffer.clear();
                            selected_index = 0;
                            home_selection = None;
                            archive_action = ArchiveAction::Extract;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                home_selection,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Paste(text)) => {
                    buffer.push_str(&text);
                    render_dashboard(
                        &app_info,
                        &buffer,
                        selected_index,
                        home_selection,
                        archive_action,
                        color_enabled,
                    )?;
                }
                Ok(Event::Resize(_, _)) => {
                    render_dashboard(
                        &app_info,
                        &buffer,
                        selected_index,
                        home_selection,
                        archive_action,
                        color_enabled,
                    )?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Dispatches an interactive command to the core application logic.
pub fn execute_interactive_command(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    cmd_str: &str,
) -> Result<()> {
    let trimmed = cmd_str.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    if trimmed == "/info" || trimmed == "info" {
        let info = app.app_info();
        formatter.print_info(&info);
        return Ok(());
    }

    if trimmed == "/doctor" || trimmed == "doctor" {
        let doc = app.doctor_check();
        formatter.print_doctor(&doc);
        return Ok(());
    }

    if trimmed == "/update" || trimmed == "update" {
        let manager = crate::core::update::UpdateManager::default();
        println!("Checking for updates...");
        match manager.check_for_update(None) {
            Ok(check_res) => {
                formatter.print_update_check(&check_res);
                if check_res.update_available {
                    print!("Proceed with download and verified installation? [y/N]: ");
                    let _ = stdout().flush();
                    let mut answer = String::new();
                    let _ = stdin().read_line(&mut answer);
                    if answer.trim().eq_ignore_ascii_case("y") {
                        println!("Applying self-update...");
                        match manager.apply_update(None, None) {
                            Ok(apply_res) => formatter.print_update_apply(&apply_res),
                            Err(e) => formatter.print_error(&e),
                        }
                    }
                }
                return Ok(());
            }
            Err(e) => {
                formatter.print_error(&e);
                return Ok(());
            }
        }
    }

    if trimmed == "/config" || trimmed == "config" {
        formatter.print_config(app.policy());
        return Ok(());
    }

    if trimmed == "/help" || trimmed == "help" {
        formatter.print_help();
        return Ok(());
    }

    if trimmed == "/extract"
        || trimmed.starts_with("/extract ")
        || trimmed == "extract"
        || trimmed.starts_with("extract ")
    {
        let remainder = if let Some(rest) = trimmed.strip_prefix("/extract") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("extract") {
            rest.trim()
        } else {
            ""
        };
        let (arch, dest) = parse_archive_and_dest(remainder);
        return run_extract_interactive(app, formatter, arch, dest);
    }

    if trimmed == "/test"
        || trimmed.starts_with("/test ")
        || trimmed == "test"
        || trimmed.starts_with("test ")
    {
        let remainder = if let Some(rest) = trimmed.strip_prefix("/test") {
            rest.trim()
        } else if let Some(rest) = trimmed.strip_prefix("test") {
            rest.trim()
        } else {
            ""
        };
        let (arch, _) = parse_archive_and_dest(remainder);
        return run_test_interactive(app, formatter, arch);
    }

    // Check if input is a dragged filesystem archive path, directory, or unrecognized command
    match inspect_path_candidate(trimmed) {
        PathCandidate::ArchiveFile { path, .. } => {
            prompt_and_run_interactive_archive(app, formatter, &path.to_string_lossy())
        }
        PathCandidate::ArchiveInDirectory {
            primary_archive,
            total_archives,
            ..
        } => {
            if total_archives > 1 && !formatter.is_quiet() {
                println!(
                    "Found {total_archives} archive parts in folder. Primary: {}",
                    primary_archive.display()
                );
            }
            prompt_and_run_interactive_archive(app, formatter, &primary_archive.to_string_lossy())
        }
        PathCandidate::EmptyDirectory(dir) => {
            let sanitized =
                crate::cli::progress::sanitize_terminal_text(&dir.display().to_string());
            println!("Directory contains no supported archives: '{sanitized}'.");
            println!(
                "Hint: Drag or specify an archive file (.zip, .rar, .7z, etc.) or a folder containing one."
            );
            Ok(())
        }
        PathCandidate::UnsupportedFile(file) => {
            let sanitized =
                crate::cli::progress::sanitize_terminal_text(&file.display().to_string());
            println!("Unsupported file type: '{sanitized}'.");
            println!(
                "Supported formats: .7z, .zip, .rar, .tar, .gz, .bz2, .xz, .zst, .iso, .cab, .arj, .lzh, .wim"
            );
            Ok(())
        }
        PathCandidate::NotFound(path) => {
            let sanitized =
                crate::cli::progress::sanitize_terminal_text(&path.display().to_string());
            println!("Path not found: '{sanitized}'.");
            println!("Hint: Please verify the file or directory exists.");
            Ok(())
        }
        PathCandidate::NotAPath => {
            let sanitized_unknown = crate::cli::progress::sanitize_terminal_text(trimmed);
            println!(
                "Unrecognized command: '{sanitized_unknown}'. Type '/help' for available commands."
            );
            Ok(())
        }
    }
}

fn prompt_and_run_interactive_archive(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    candidate_path: &str,
) -> Result<()> {
    let display_path = crate::cli::progress::sanitize_terminal_text(candidate_path);
    println!("Archive detected: {display_path}");
    let e_act = formatter.style("[E]xtract", "\x1b[1m");
    let t_act = formatter.style("[T]est integrity", "\x1b[1m");
    let c_act = formatter.style("[C]ancel", "\x1b[1m");
    print!("  {e_act} (default)   {t_act}   {c_act}: ");
    let _ = stdout().flush();

    let mut choice = String::new();
    stdin().read_line(&mut choice)?;
    let choice_trimmed = choice.trim();

    if choice_trimmed.is_empty()
        || choice_trimmed.eq_ignore_ascii_case("e")
        || choice_trimmed.eq_ignore_ascii_case("extract")
    {
        run_extract_interactive(app, formatter, Some(candidate_path.to_string()), None)
    } else if choice_trimmed.eq_ignore_ascii_case("t")
        || choice_trimmed.eq_ignore_ascii_case("test")
    {
        run_test_interactive(app, formatter, Some(candidate_path.to_string()))
    } else {
        if !formatter.is_quiet() {
            println!("{}", formatter.style("Operation cancelled.", "\x1b[90m"));
        }
        Ok(())
    }
}

/// Unescapes backslash-escaped characters commonly inserted by macOS terminal drag-and-drop
/// (e.g. spaces, brackets, parentheses, quotes).
#[must_use]
pub fn unescape_terminal_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ' '
                    || next == '('
                    || next == ')'
                    || next == '['
                    || next == ']'
                    || next == '{'
                    || next == '}'
                    || next == '\''
                    || next == '"'
                    || next == '&'
                    || next == '$'
                    || next == '!'
                    || next == '#'
                    || next == ';'
                    || next == '`'
                    || next == '\\'
                {
                    chars.next();
                    out.push(next);
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// Cleans raw terminal input paths by stripping surrounding single/double quotes,
/// leading/trailing whitespace, and unescaping drag-and-drop artifacts.
#[must_use]
pub fn clean_terminal_path(raw: &str) -> String {
    let mut s = raw.trim();
    while (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
        || (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
    {
        s = &s[1..s.len() - 1];
        s = s.trim();
    }
    let trimmed = s.trim_matches(|c| c == '\'' || c == '"').trim();
    if trimmed.contains('\\') {
        let unescaped = unescape_terminal_path(trimmed);
        if Path::new(&unescaped).exists() || !Path::new(trimmed).exists() {
            return unescaped;
        }
    }
    trimmed.to_string()
}

/// Splits an argument string into tokens, respecting single and double quotes and escaped spaces.
#[must_use]
pub fn parse_command_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = args.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '\\' if !in_single_quote && !in_double_quote => {
                if let Some(&next) = chars.peek() {
                    if next == ' '
                        || next == '\''
                        || next == '"'
                        || next == '\\'
                        || next == '('
                        || next == ')'
                    {
                        chars.next();
                        current.push(next);
                        continue;
                    }
                }
                current.push('\\');
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(clean_terminal_path(&current));
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(clean_terminal_path(&current));
    }
    tokens
}

/// Extracts archive path and optional destination path from command remainder.
#[must_use]
pub fn parse_archive_and_dest(args: &str) -> (Option<String>, Option<String>) {
    let tokens = parse_command_args(args);
    let mut iter = tokens.into_iter();
    let archive = iter.next();
    let dest = iter.next();
    (archive, dest)
}

/// Prompts for archive path (if not supplied) and executes integrity test.
fn run_test_interactive(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    inline_archive: Option<String>,
) -> Result<()> {
    let archive_str = match inline_archive {
        Some(path) if !path.trim().is_empty() => clean_terminal_path(&path),
        _ => {
            println!("Enter archive path:");
            print!("› ");
            let _ = stdout().flush();
            let mut line = String::new();
            stdin().read_line(&mut line)?;
            let clean = clean_terminal_path(&line);
            if clean.is_empty() {
                if !formatter.is_quiet() {
                    println!("{}", formatter.style("Operation cancelled.", "\x1b[90m"));
                }
                return Ok(());
            }
            clean
        }
    };
    let raw_archive_path = PathBuf::from(archive_str);
    let archive_path = match resolve_archive_or_directory(&raw_archive_path) {
        Ok(p) => p,
        Err(dir_disp) => {
            if !formatter.is_quiet() {
                println!("Directory contains no supported archives: '{dir_disp}'.");
            }
            return Ok(());
        }
    };

    let show_progress = formatter.should_show_progress();
    if !show_progress && !formatter.is_quiet() {
        println!("Testing archive integrity, please wait...");
    }
    let res = crate::cli::run_test_with_prompt(
        app,
        &archive_path,
        &crate::cli::TerminalPasswordPrompter,
        show_progress,
        0,
    )?;
    formatter.print_test_result(&res);
    Ok(())
}

/// Prompts for archive/output path (if not supplied) and executes extraction.
fn run_extract_interactive(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
    inline_archive: Option<String>,
    inline_dest: Option<String>,
) -> Result<()> {
    let archive_str = match inline_archive {
        Some(path) if !path.trim().is_empty() => clean_terminal_path(&path),
        _ => {
            println!("Enter archive path:");
            print!("› ");
            let _ = stdout().flush();
            let mut archive_line = String::new();
            stdin().read_line(&mut archive_line)?;
            let clean_archive = clean_terminal_path(&archive_line);
            if clean_archive.is_empty() {
                if !formatter.is_quiet() {
                    println!("{}", formatter.style("Operation cancelled.", "\x1b[90m"));
                }
                return Ok(());
            }
            clean_archive
        }
    };
    let raw_archive_path = PathBuf::from(archive_str);
    let archive_path = match resolve_archive_or_directory(&raw_archive_path) {
        Ok(p) => p,
        Err(dir_disp) => {
            if !formatter.is_quiet() {
                println!("Directory contains no supported archives: '{dir_disp}'.");
            }
            return Ok(());
        }
    };

    let dest_opt_buf: Option<PathBuf> = match inline_dest {
        Some(dest) if !dest.trim().is_empty() => Some(PathBuf::from(clean_terminal_path(&dest))),
        Some(_) => None,
        None => {
            println!("Enter destination directory (leave empty for current directory):");
            print!("› ");
            let _ = stdout().flush();
            let mut dest_line = String::new();
            stdin().read_line(&mut dest_line)?;
            let clean_dest = clean_terminal_path(&dest_line);
            if clean_dest.is_empty() {
                None
            } else {
                Some(PathBuf::from(clean_dest))
            }
        }
    };

    let dest_opt: Option<&Path> = dest_opt_buf.as_deref();

    let show_progress = formatter.should_show_progress();
    if !show_progress && !formatter.is_quiet() {
        println!("Extracting archive, please wait...");
    }
    let res = crate::cli::run_extract_with_prompt(
        app,
        &archive_path,
        dest_opt,
        &crate::cli::TerminalPasswordPrompter,
        show_progress,
        0,
    )?;
    formatter.print_extract_result(&res);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_terminal_path() {
        assert_eq!(
            clean_terminal_path("'/Users/foo/My Archive.rar'"),
            "/Users/foo/My Archive.rar"
        );
        assert_eq!(
            clean_terminal_path("\"/Users/foo/My Archive.rar\""),
            "/Users/foo/My Archive.rar"
        );
        assert_eq!(
            clean_terminal_path("  '/Volumes/PS5/untitled folder'  "),
            "/Volumes/PS5/untitled folder"
        );
        assert_eq!(
            clean_terminal_path("/Users/plain/path.zip"),
            "/Users/plain/path.zip"
        );
        assert_eq!(
            clean_terminal_path(r"/Volumes/PS5/untitled\ folder/test.zip"),
            "/Volumes/PS5/untitled folder/test.zip"
        );
        assert_eq!(
            clean_terminal_path(r"'/Volumes/PS5/untitled\ folder/test.zip'"),
            "/Volumes/PS5/untitled folder/test.zip"
        );
    }

    #[test]
    fn test_unescape_terminal_path() {
        assert_eq!(
            unescape_terminal_path(r"/Users/foo/My\ Archive.rar"),
            "/Users/foo/My Archive.rar"
        );
        assert_eq!(
            unescape_terminal_path(r"/Volumes/Backups/Project\ \(1\)\ \[v2\].zip"),
            "/Volumes/Backups/Project (1) [v2].zip"
        );
        assert_eq!(
            unescape_terminal_path(r"/tmp/test\ \#1\&2\;3.tar.gz"),
            "/tmp/test #1&2;3.tar.gz"
        );
        assert_eq!(
            unescape_terminal_path(r"/Users/test/Архив\ 1.zip"),
            "/Users/test/Архив 1.zip"
        );
        assert_eq!(
            unescape_terminal_path("/Users/plain/path.zip"),
            "/Users/plain/path.zip"
        );
    }

    #[test]
    fn test_parse_command_args() {
        let empty = parse_command_args("");
        assert!(empty.is_empty());

        let single = parse_command_args("/tmp/archive.zip");
        assert_eq!(single, vec!["/tmp/archive.zip"]);

        let multiple = parse_command_args("'/tmp/my archive.zip' /output/dir");
        assert_eq!(multiple, vec!["/tmp/my archive.zip", "/output/dir"]);

        let double_quoted = parse_command_args("\"/Volumes/Ext/data.tar.gz\" \"/tmp/out dir\"");
        assert_eq!(
            double_quoted,
            vec!["/Volumes/Ext/data.tar.gz", "/tmp/out dir"]
        );
    }

    #[test]
    fn test_parse_archive_and_dest() {
        let (arch, dest) = parse_archive_and_dest("");
        assert_eq!(arch, None);
        assert_eq!(dest, None);

        let (arch, dest) = parse_archive_and_dest("/path/to/archive.zip");
        assert_eq!(arch, Some("/path/to/archive.zip".to_string()));
        assert_eq!(dest, None);

        let (arch, dest) = parse_archive_and_dest("'/path/a b.zip' /dest");
        assert_eq!(arch, Some("/path/a b.zip".to_string()));
        assert_eq!(dest, Some("/dest".to_string()));
    }

    #[test]
    fn test_filter_suggestions() {
        let all = filter_suggestions("");
        assert_eq!(all.len(), 8);

        let slash_ex = filter_suggestions("/ex");
        assert_eq!(slash_ex.len(), 2);
        assert!(slash_ex.iter().any(|s| s.command == "/extract"));
        assert!(slash_ex.iter().any(|s| s.command == "/exit"));

        let test_cmd = filter_suggestions("test");
        assert_eq!(test_cmd.len(), 1);
        assert_eq!(test_cmd[0].command, "/test");

        let doctor_cmd = filter_suggestions("/d");
        assert_eq!(doctor_cmd.len(), 1);
        assert_eq!(doctor_cmd[0].command, "/doctor");
    }

    #[test]
    fn test_prompt_prefix() {
        let p = prompt_prefix();
        assert!(p.starts_with("unarc "));
        assert!(p.contains('›'));
    }

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("hello world", 5), "hell…");
        assert_eq!(truncate_chars("abc", 2), "ab");
    }

    #[test]
    fn test_command_title_mapping() {
        assert_eq!(command_title("/info"), "Archive Information");
        assert_eq!(command_title("/doctor"), "Security Diagnostics");
        assert_eq!(command_title("/update"), "Self-Update");
        assert_eq!(command_title("/config"), "Security Configuration");
        assert_eq!(command_title("/help"), "Command Reference");
        assert_eq!(command_title("/extract /tmp/a.zip"), "Archive Extraction");
        assert_eq!(command_title("/test /tmp/a.zip"), "Archive Integrity Test");

        let (t, slug) = command_title_and_slug("/test");
        assert_eq!(t, "Archive Integrity Test");
        assert_eq!(slug, "(/test)");

        let (t2, slug2) = command_title_and_slug("/doctor");
        assert_eq!(t2, "Security Diagnostics");
        assert_eq!(slug2, "(/doctor)");
    }

    #[test]
    fn test_tui_layout_alignment() {
        // Test standard 80 columns
        let layout80 = TuiLayout::new(80, 24, false);
        assert_eq!(layout80.content_width, 74);
        assert_eq!(layout80.left_margin, 3);
        let div = layout80.divider();
        assert_eq!(div.trim_start().len(), 74);
        assert!(div.starts_with("   "));

        // Test wide 120 columns (capped at 80 cols and centered)
        let layout120 = TuiLayout::new(120, 30, false);
        assert_eq!(layout120.content_width, 80);
        assert_eq!(layout120.left_margin, 20);
        let div120 = layout120.divider();
        assert_eq!(div120.trim_start().len(), 80);
        assert_eq!(div120.chars().take(20).collect::<String>(), " ".repeat(20));

        // Test narrow 50 columns
        let layout50 = TuiLayout::new(50, 20, false);
        assert_eq!(layout50.content_width, 48);
        assert_eq!(layout50.left_margin, 1);

        // Test centered plain text
        let centered = layout80.center_plain("Archive Integrity Test");
        let vis_len = "Archive Integrity Test".len();
        let expected_pad = (80 - vis_len) / 2;
        assert_eq!(
            centered.chars().take(expected_pad).collect::<String>(),
            " ".repeat(expected_pad)
        );
    }

    #[test]
    fn test_quick_actions_coverage() {
        assert_eq!(QUICK_ACTIONS.len(), 8);
        assert_eq!(QUICK_ACTIONS[0].command, "/extract");
        assert_eq!(QUICK_ACTIONS[1].command, "/test");
        assert_eq!(QUICK_ACTIONS[2].command, "/info");
        assert_eq!(QUICK_ACTIONS[3].command, "/doctor");
        assert_eq!(QUICK_ACTIONS[4].command, "/update");
        assert_eq!(QUICK_ACTIONS[5].command, "/config");
        assert_eq!(QUICK_ACTIONS[6].command, "/help");
        assert_eq!(QUICK_ACTIONS[7].command, "/exit");
    }

    #[test]
    fn test_no_color_compliance() {
        let layout = TuiLayout::new(80, 24, false);
        let div_nocolor = layout.divider();
        assert!(!div_nocolor.contains("\x1b["));
        assert!(div_nocolor.contains('-'));
        assert!(!div_nocolor.contains('─'));
    }

    #[test]
    fn test_dashboard_home_screen_alignment_across_widths() {
        let app = Application::default();
        let info = app.app_info();

        for &width in &[36, 40, 50, 60, 80, 100, 120, 140, 160] {
            let state = DashboardState {
                buffer: "",
                selected_index: 0,
                home_selection: None,
                archive_action: ArchiveAction::Extract,
                term_width: width,
                term_height: 24,
                color_enabled: false,
            };
            let (rows, _, hide) = generate_dashboard_rows(&info, &state);

            assert!(!rows.is_empty());
            assert!(!hide);
            let layout = TuiLayout::new(width, 24, false);

            for (idx, row) in rows.iter().enumerate() {
                if row.is_empty() {
                    continue;
                }
                let clean_len = crate::cli::progress::sanitize_terminal_text(row)
                    .chars()
                    .count();
                assert!(
                    clean_len <= width,
                    "Row {idx} at width {width} exceeds terminal width: len={clean_len}, content='{row}'"
                );
            }

            // Verify centered UNARC brand row is centered
            let brand_row = &rows[0];
            let brand_clean = crate::cli::progress::sanitize_terminal_text(brand_row);
            let brand_text = format!("UNARC {}", info.version);
            let expected_pad = (width - brand_text.len()) / 2;
            assert!(
                brand_clean.starts_with(&" ".repeat(expected_pad)),
                "Brand row not centered at width {width}: '{brand_clean}'"
            );

            // Verify centered operation title is centered
            let title_row = rows
                .iter()
                .find(|r| r.contains("Archive Utility"))
                .expect("Title row present");
            let title_clean = crate::cli::progress::sanitize_terminal_text(title_row);
            let expected_title_pad = (width - "Archive Utility".len()) / 2;
            assert!(
                title_clean.starts_with(&" ".repeat(expected_title_pad)),
                "Title row not centered at width {width}: '{title_clean}'"
            );

            // Verify left-aligned content rows start with layout.left_margin
            let prompt_row = rows
                .iter()
                .find(|r| r.contains("Enter archive path:"))
                .expect("Prompt row present");
            let margin_pad = " ".repeat(layout.left_margin);
            assert!(
                prompt_row.starts_with(&margin_pad),
                "Prompt row does not start with margin at width {width}: '{prompt_row}'"
            );
        }
    }

    #[test]
    fn test_dashboard_command_palette() {
        let app = Application::default();
        let info = app.app_info();

        // 1. Initial palette '/'
        let state1 = DashboardState {
            buffer: "/",
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows, _, _) = generate_dashboard_rows(&info, &state1);
        let joined = rows.join("\n");
        assert!(joined.contains("Command Palette"));
        assert!(joined.contains("Suggestions:"));
        assert!(joined.contains("/extract"));
        assert!(joined.contains("/test"));
        assert!(joined.contains("/info"));
        assert!(joined.contains("/doctor"));
        assert!(joined.contains("/update"));
        assert!(joined.contains("/config"));
        assert!(joined.contains("/help"));
        assert!(joined.contains("/exit"));
        assert!(joined.contains("> /extract"));

        // 2. Filtered palette '/ex'
        let state2 = DashboardState {
            buffer: "/ex",
            selected_index: 1,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows_filtered, _, _) = generate_dashboard_rows(&info, &state2);
        let joined_filtered = rows_filtered.join("\n");
        assert!(joined_filtered.contains("/extract"));
        assert!(joined_filtered.contains("> /exit"));
        assert!(!joined_filtered.contains("/doctor"));
    }

    #[test]
    fn test_dashboard_archive_detected() {
        let app = Application::default();
        let info = app.app_info();

        let temp_dir = std::env::temp_dir();
        let archive_file = temp_dir.join("test_interactive_pkg.zip");
        let _ = std::fs::write(&archive_file, b"PK\x05\x06dummyzipdata");

        let arch_str = archive_file.to_string_lossy();
        let state1 = DashboardState {
            buffer: &arch_str,
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows, _, hide_cursor) = generate_dashboard_rows(&info, &state1);

        let joined = rows.join("\n");
        assert!(hide_cursor);
        assert!(joined.contains("Archive Detected"));
        assert!(joined.contains("Archive Details"));
        assert!(joined.contains("test_interactive_pkg.zip"));
        assert!(joined.contains("ZIP Archive"));
        assert!(joined.contains("> [E]xtract archive"));
        assert!(joined.contains("  [T]est integrity"));
        assert!(joined.contains("  [C]ancel"));

        let state2 = DashboardState {
            buffer: &arch_str,
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Test,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows_test, _, _) = generate_dashboard_rows(&info, &state2);
        let joined_test = rows_test.join("\n");
        assert!(joined_test.contains("  [E]xtract archive"));
        assert!(joined_test.contains("> [T]est integrity"));

        let _ = std::fs::remove_file(archive_file);
    }

    #[test]
    fn test_dashboard_no_color_guarantee() {
        let app = Application::default();
        let info = app.app_info();

        let state_home = DashboardState {
            buffer: "",
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (home_rows, _, _) = generate_dashboard_rows(&info, &state_home);
        for row in &home_rows {
            assert!(!row.contains("\x1b["), "Row contains ANSI escape: {row}");
        }

        let state_cmd = DashboardState {
            buffer: "/",
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (cmd_rows, _, _) = generate_dashboard_rows(&info, &state_cmd);
        for row in &cmd_rows {
            assert!(!row.contains("\x1b["), "Row contains ANSI escape: {row}");
        }
    }

    #[test]
    fn test_drag_and_drop_complex_paths() {
        let unescaped = unescape_terminal_path(
            r"/Volumes/Macintosh\ HD/Users/dev/Archive\ \[2024\]\ \(v1\)\ \{backup\}\ &special.zip",
        );
        assert_eq!(
            unescaped,
            "/Volumes/Macintosh HD/Users/dev/Archive [2024] (v1) {backup} &special.zip"
        );

        let cleaned = clean_terminal_path("  \"/Volumes/Macintosh HD/My File.tar.gz\"  ");
        assert_eq!(cleaned, "/Volumes/Macintosh HD/My File.tar.gz");
    }

    #[test]
    fn test_render_visual_inspection() {
        let app = Application::default();
        let info = app.app_info();

        for width in [60, 80, 100, 120] {
            println!("\n========== RENDERED DASHBOARD AT {width} COLS ==========");
            let state = DashboardState {
                buffer: "",
                selected_index: 0,
                home_selection: None,
                archive_action: ArchiveAction::Extract,
                term_width: width,
                term_height: 24,
                color_enabled: false,
            };
            let (rows, _, _) = generate_dashboard_rows(&info, &state);
            for r in &rows {
                println!("{r}");
            }
        }

        println!("\n========== COMMAND PALETTE (BUFFER: '/') AT 80 COLS ==========");
        let state_cmd = DashboardState {
            buffer: "/",
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows_cmd, _, _) = generate_dashboard_rows(&info, &state_cmd);
        for r in &rows_cmd {
            println!("{r}");
        }

        println!("\n========== ARCHIVE DETECTED AT 80 COLS ==========");
        let temp_dir = std::env::temp_dir();
        let archive_file = temp_dir.join("project_backup.zip");
        let _ = std::fs::write(&archive_file, b"PK\x05\x06testpayload1234567890");
        let arch_str = archive_file.to_string_lossy();
        let state_arch = DashboardState {
            buffer: &arch_str,
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows_arch, _, _) = generate_dashboard_rows(&info, &state_arch);
        for r in &rows_arch {
            println!("{r}");
        }
        let _ = std::fs::remove_file(archive_file);
    }

    #[test]
    fn test_is_command_palette_input() {
        assert!(is_command_palette_input("/"));
        assert!(is_command_palette_input("/help"));
        assert!(is_command_palette_input("/extract"));
        assert!(is_command_palette_input("/test"));
        assert!(is_command_palette_input("/doctor"));
        assert!(is_command_palette_input("/config"));
        assert!(is_command_palette_input("/update"));
        assert!(is_command_palette_input("/exit"));
        assert!(is_command_palette_input("/ex"));

        // Filesystem paths must NOT be classified as command palette inputs
        assert!(!is_command_palette_input(
            "/Users/mosabbirmaruf/Downloads/Wolverine"
        ));
        assert!(!is_command_palette_input("/tmp/archive.zip"));
        assert!(!is_command_palette_input("/var/log"));
        assert!(!is_command_palette_input("/a/b"));

        // Commands with arguments contain whitespace
        assert!(!is_command_palette_input("/extract archive.zip"));
        assert!(!is_command_palette_input("/test data.tar.gz"));

        // Empty or non-slash inputs
        assert!(!is_command_palette_input(""));
        assert!(!is_command_palette_input("help"));
        assert!(!is_command_palette_input("extract"));
    }

    #[test]
    fn test_inspect_path_candidate_directory_and_files() {
        let temp_dir = std::env::temp_dir().join(format!("unarc_test_dir_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        // Empty directory
        match inspect_path_candidate(&temp_dir.to_string_lossy()) {
            PathCandidate::EmptyDirectory(dir) => assert_eq!(dir, temp_dir),
            other => panic!("Expected EmptyDirectory, got: {other:?}"),
        }

        // Add multi-part archives to directory
        let part1 = temp_dir.join("game.part01.rar");
        let part2 = temp_dir.join("game.part02.rar");
        let _ = std::fs::write(&part1, b"Rar!\x1a\x07\x00dummyrar");
        let _ = std::fs::write(&part2, b"Rar!\x1a\x07\x00dummyrar");

        match inspect_path_candidate(&temp_dir.to_string_lossy()) {
            PathCandidate::ArchiveInDirectory {
                primary_archive,
                total_archives,
                format,
                ..
            } => {
                assert_eq!(primary_archive, part1);
                assert_eq!(total_archives, 2);
                assert_eq!(format, ArchiveFormat::Rar);
            }
            other => panic!("Expected ArchiveInDirectory, got: {other:?}"),
        }

        // Single file archive
        match inspect_path_candidate(&part1.to_string_lossy()) {
            PathCandidate::ArchiveFile { path, format } => {
                assert_eq!(path, part1);
                assert_eq!(format, ArchiveFormat::Rar);
            }
            other => panic!("Expected ArchiveFile, got: {other:?}"),
        }

        // Unsupported regular file
        let text_file = temp_dir.join("notes.txt");
        let _ = std::fs::write(&text_file, b"some text");
        match inspect_path_candidate(&text_file.to_string_lossy()) {
            PathCandidate::UnsupportedFile(path) => assert_eq!(path, text_file),
            other => panic!("Expected UnsupportedFile, got: {other:?}"),
        }

        // Non-existent path
        let non_existent = temp_dir.join("does_not_exist_xyz");
        match inspect_path_candidate(&non_existent.to_string_lossy()) {
            PathCandidate::NotFound(path) => assert_eq!(path, non_existent),
            other => panic!("Expected NotFound, got: {other:?}"),
        }

        // Arbitrary command string
        assert_eq!(
            inspect_path_candidate("unknown_cmd"),
            PathCandidate::NotAPath
        );
        assert_eq!(inspect_path_candidate("/help"), PathCandidate::NotAPath);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_detected_dashboard_alignment() {
        let app = Application::default();
        let info = app.app_info();

        let temp_dir = std::env::temp_dir().join(format!("unarc_empty_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let state = DashboardState {
            buffer: &temp_dir.to_string_lossy(),
            selected_index: 0,
            home_selection: None,
            archive_action: ArchiveAction::Extract,
            term_width: 80,
            term_height: 24,
            color_enabled: false,
        };
        let (rows, _, _) = generate_dashboard_rows(&info, &state);
        let joined = rows.join("\n");
        assert!(joined.contains("Directory Detected"));
        assert!(joined.contains("No supported archive files found in folder"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
