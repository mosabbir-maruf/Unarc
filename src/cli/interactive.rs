//! Terminal-native interactive Terminal UI (TUI) and suggestion engine.

use crate::archive::ArchiveFormat;
use crate::core::Application;
use crate::core::app::AppInfo;
use crate::error::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{IsTerminal, Write, stdin, stdout};
use std::path::{Path, PathBuf};

/// Clean ASCII wordmark with subtitle.
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
        description: "Securely extract archive with boundary verification",
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
        description: "Check or apply cryptographically verified self-update",
    },
    CommandSuggestion {
        command: "/config",
        description: "Display active zero-trust security configuration",
    },
    CommandSuggestion {
        command: "/help",
        description: "Display interactive help reference",
    },
    CommandSuggestion {
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

/// Renders a framed panel box with top title and wrapped/padded content lines.
#[must_use]
pub fn render_box(title: &str, lines: &[String], width: usize, color_enabled: bool) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len() + 2);
    let (tl, tr, bl, br, h, v) = if color_enabled {
        ("┌", "┐", "└", "┘", "─", "│")
    } else {
        ("+", "+", "+", "+", "-", "|")
    };

    // Calculate inner width available for content
    let inner_width = width.saturating_sub(6);

    // Top border with optional title
    let top = if title.is_empty() {
        format!("{tl}{}{tr}", h.repeat(width.saturating_sub(2)))
    } else {
        let title_clean = crate::cli::progress::sanitize_terminal_text(title);
        let title_len = title_clean.chars().count();
        let border_len = width.saturating_sub(title_len + 5);
        if color_enabled {
            format!("{tl}{h} \x1b[1m{title}\x1b[0m {}{tr}", h.repeat(border_len))
        } else {
            format!("{tl}{h} {title} {}{tr}", h.repeat(border_len))
        }
    };
    out.push(top);

    // Content lines
    for line in lines {
        let clean = crate::cli::progress::sanitize_terminal_text(line);
        let vis_len = clean.chars().count();
        if vis_len <= inner_width {
            let pad = inner_width - vis_len;
            out.push(format!("{v}  {line}{}  {v}", " ".repeat(pad)));
        } else {
            let truncated = truncate_chars(&clean, inner_width);
            out.push(format!("{v}  {truncated}  {v}"));
        }
    }

    // Bottom border
    let bottom = format!("{bl}{}{br}", h.repeat(width.saturating_sub(2)));
    out.push(bottom);

    out
}

/// Builds the compact status header bar and divider rule.
#[must_use]
pub fn build_header(info: &AppInfo, term_width: usize, color_enabled: bool) -> (String, String) {
    let version = &info.version;
    let engine_ver = &info.engine.pinned_version;

    let os_desc = if info.platform.is_apple_silicon {
        format!("{} (Apple Silicon)", info.platform.os)
    } else if info.platform.is_linux {
        format!("{} ({})", info.platform.os, info.platform.arch)
    } else {
        format!("{}-{}", info.platform.os, info.platform.arch)
    };

    let sandbox_status = if info.platform.capabilities.supports_sandbox_confinement {
        "Active"
    } else {
        "Standard"
    };

    let header_line = if term_width < 65 {
        if color_enabled {
            format!(
                " \x1b[1;37mUNARC\x1b[0m \x1b[1;36mv{version}\x1b[0m  \x1b[90m•\x1b[0m  {os_desc}  \x1b[90m•\x1b[0m  \x1b[32m7zz v{engine_ver}\x1b[0m"
            )
        } else {
            format!(" UNARC v{version}  |  {os_desc}  |  7zz v{engine_ver}")
        }
    } else if color_enabled {
        format!(
            " \x1b[1;37mUNARC\x1b[0m \x1b[1;36mv{version}\x1b[0m  \x1b[90m•\x1b[0m  {os_desc}  \x1b[90m•\x1b[0m  Engine: \x1b[32m7zz v{engine_ver}\x1b[0m  \x1b[90m•\x1b[0m  Sandbox: \x1b[32m{sandbox_status}\x1b[0m"
        )
    } else {
        format!(
            " UNARC v{version}  |  {os_desc}  |  Engine: 7zz v{engine_ver}  |  Sandbox: {sandbox_status}"
        )
    };

    let div_char = if color_enabled { "─" } else { "-" };
    let divider = if color_enabled {
        format!("\x1b[90m{}\x1b[0m", div_char.repeat(term_width))
    } else {
        div_char.repeat(term_width)
    };

    (header_line, divider)
}

/// Returns a human-friendly command title banner.
#[must_use]
pub fn command_title(cmd: &str) -> &'static str {
    let trimmed = cmd.trim();
    if trimmed.starts_with("/info") || trimmed == "info" {
        "SYSTEM INFORMATION (/info)"
    } else if trimmed.starts_with("/doctor") || trimmed == "doctor" {
        "SYSTEM DIAGNOSTICS & HEALTH CHECK (/doctor)"
    } else if trimmed.starts_with("/update") || trimmed == "update" {
        "SELF-UPDATE STATUS (/update)"
    } else if trimmed.starts_with("/config") || trimmed == "config" {
        "SECURITY CONFIGURATION (/config)"
    } else if trimmed.starts_with("/help") || trimmed == "help" {
        "COMMAND REFERENCE (/help)"
    } else if trimmed.starts_with("/extract") || trimmed.starts_with("extract") {
        "ARCHIVE EXTRACTION (/extract)"
    } else if trimmed.starts_with("/test") || trimmed.starts_with("test") {
        "ARCHIVE INTEGRITY TEST (/test)"
    } else {
        "COMMAND EXECUTION"
    }
}

/// Renders the complete interactive TUI dashboard.
fn render_dashboard(
    app_info: &AppInfo,
    buffer: &str,
    selected_index: usize,
    archive_action: ArchiveAction,
    color_enabled: bool,
) -> std::io::Result<()> {
    let mut out = stdout();
    let (term_width_u16, term_height_u16) = crossterm::terminal::size().unwrap_or((80, 24));
    let term_width = term_width_u16 as usize;
    let term_height = term_height_u16 as usize;
    let box_width = term_width.saturating_sub(4).clamp(40, 96);
    let inner_width = box_width.saturating_sub(6);
    let compact = term_height < 24;

    let mut screen_rows: Vec<String> = Vec::new();

    // 1. Header & Divider
    let (h1, h2) = build_header(app_info, term_width, color_enabled);
    screen_rows.push(h1);
    screen_rows.push(h2);

    if !compact {
        screen_rows.push(String::new());
    }

    // 2. Archive Input / Command Box
    let prompt_sym = if color_enabled {
        "\x1b[1;36m›\x1b[0m "
    } else {
        "> "
    };
    let max_disp_buf = inner_width.saturating_sub(4);
    let display_buffer = if buffer.chars().count() > max_disp_buf {
        truncate_chars(buffer, max_disp_buf)
    } else {
        buffer.to_string()
    };
    let input_line = format!("{prompt_sym}{display_buffer}");
    let input_box = render_box(
        "Archive Input / Command",
        &[input_line],
        box_width,
        color_enabled,
    );
    screen_rows.extend(input_box);

    if !compact {
        screen_rows.push(String::new());
    }

    // 3. Contextual Content Area
    let candidate_str = clean_terminal_path(buffer.trim());
    let candidate_path = PathBuf::from(&candidate_str);
    let is_archive_detected = !candidate_str.is_empty()
        && candidate_path.is_file()
        && ArchiveFormat::from_extension(&candidate_path).is_some();

    if is_archive_detected {
        let filename = candidate_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let format_name = ArchiveFormat::from_extension(&candidate_path).unwrap();
        let size_bytes = std::fs::metadata(&candidate_path)
            .map(|m| m.len())
            .unwrap_or(0);
        let size_str = crate::cli::output::format_byte_size(size_bytes);

        let sanitized_name = crate::cli::progress::sanitize_terminal_text(&filename);
        let sanitized_path = crate::cli::progress::sanitize_terminal_text(&candidate_str);
        let max_val_w = box_width.saturating_sub(18);
        let disp_name = truncate_chars(&sanitized_name, max_val_w);
        let disp_path = crate::cli::progress::truncate_filename(&sanitized_path, max_val_w);

        let (btn_extract, btn_test, btn_cancel) = match archive_action {
            ArchiveAction::Extract => (
                if color_enabled {
                    "\x1b[1;36m[› Extract (default)]\x1b[0m"
                } else {
                    "[> Extract (default)]"
                },
                if color_enabled {
                    "\x1b[90m[T] Test Integrity\x1b[0m"
                } else {
                    "[T] Test Integrity"
                },
                if color_enabled {
                    "\x1b[90m[C] Cancel\x1b[0m"
                } else {
                    "[C] Cancel"
                },
            ),
            ArchiveAction::Test => (
                if color_enabled {
                    "\x1b[90m[E] Extract\x1b[0m"
                } else {
                    "[E] Extract"
                },
                if color_enabled {
                    "\x1b[1;36m[› Test Integrity]\x1b[0m"
                } else {
                    "[> Test Integrity]"
                },
                if color_enabled {
                    "\x1b[90m[C] Cancel\x1b[0m"
                } else {
                    "[C] Cancel"
                },
            ),
            ArchiveAction::Cancel => (
                if color_enabled {
                    "\x1b[90m[E] Extract\x1b[0m"
                } else {
                    "[E] Extract"
                },
                if color_enabled {
                    "\x1b[90m[T] Test Integrity\x1b[0m"
                } else {
                    "[T] Test Integrity"
                },
                if color_enabled {
                    "\x1b[1;36m[› Cancel]\x1b[0m"
                } else {
                    "[> Cancel]"
                },
            ),
        };

        let card_lines = vec![
            format!("File:      {disp_name}"),
            format!("Location:  {disp_path}"),
            format!("Format:    {format_name} Archive"),
            format!("Size:      {size_str}"),
            String::new(),
            format!("Action:    {btn_extract}   {btn_test}   {btn_cancel}"),
        ];
        let card_box = render_box("Archive Detected", &card_lines, box_width, color_enabled);
        screen_rows.extend(card_box);
    } else if buffer.starts_with('/') && !buffer.contains(' ') {
        let filtered = filter_suggestions(buffer);
        let mut palette_lines = Vec::new();

        if filtered.is_empty() {
            palette_lines
                .push("  No matching commands found. Type '/help' for reference.".to_string());
        } else {
            let max_desc_w = box_width.saturating_sub(22);
            for (i, item) in filtered.iter().enumerate() {
                let desc = truncate_chars(item.description, max_desc_w);
                if i == selected_index {
                    if color_enabled {
                        palette_lines.push(format!(
                            "\x1b[1;36m›\x1b[0m \x1b[1;36m{:<10}\x1b[0m \x1b[1m{desc}\x1b[0m",
                            item.command
                        ));
                    } else {
                        palette_lines.push(format!("> {:<10} {desc}", item.command));
                    }
                } else if color_enabled {
                    palette_lines.push(format!(
                        "  \x1b[1m{:<10}\x1b[0m \x1b[90m{desc}\x1b[0m",
                        item.command
                    ));
                } else {
                    palette_lines.push(format!("  {:<10} {desc}", item.command));
                }
            }
        }

        let palette_box = render_box("Commands", &palette_lines, box_width, color_enabled);
        screen_rows.extend(palette_box);
    } else if buffer.trim().is_empty() {
        let quick_start = vec![
            "• Drag & drop archive file from Finder directly into this terminal".to_string(),
            "• Type '/' to open command palette (/extract, /test, /doctor, /info)".to_string(),
            "• Type '/help' for complete interactive command reference".to_string(),
            "• Direct extraction: type '/extract <path>' or drop file and press Enter".to_string(),
        ];
        let quick_box = render_box("Quick Start", &quick_start, box_width, color_enabled);
        screen_rows.extend(quick_box);
    } else {
        let notice = vec![
            "Press Enter to run as command or path.".to_string(),
            "Type '/' to open command palette.".to_string(),
        ];
        let notice_box = render_box("Notice", &notice, box_width, color_enabled);
        screen_rows.extend(notice_box);
    }

    if !compact {
        screen_rows.push(String::new());
    }

    // 4. Footer Hint Bar
    let nav_hint = if is_archive_detected {
        if term_width < 70 {
            "Enter Run • Tab Toggle Action • Esc Clear • Ctrl+C Exit"
        } else {
            "Enter Confirm  •  Tab Switch Action  •  [E]xtract  •  [T]est  •  Esc Clear"
        }
    } else if buffer.starts_with('/') {
        if term_width < 70 {
            "↑/↓ Pick • Tab Auto • Enter Run • Esc Clear • Ctrl+C Exit"
        } else {
            "↑/↓ Navigate  •  Tab Complete  •  Enter Run  •  Esc Clear  •  Ctrl+C Exit"
        }
    } else if term_width < 70 {
        "Type / for Commands • Drag archive • Ctrl+C Exit"
    } else {
        "Type '/' for Command Palette  •  Drag & drop archive to extract  •  Ctrl+C Exit"
    };

    let footer_rule_char = if color_enabled { "─" } else { "-" };
    let footer_div = if color_enabled {
        format!("\x1b[90m{}\x1b[0m", footer_rule_char.repeat(term_width))
    } else {
        footer_rule_char.repeat(term_width)
    };

    screen_rows.push(footer_div);
    if color_enabled {
        screen_rows.push(format!(" \x1b[90m{nav_hint}\x1b[0m"));
    } else {
        screen_rows.push(format!(" {nav_hint}"));
    }

    // Calculate cursor position inside the input box
    let cursor_x = 5 + buffer.chars().count().min(max_disp_buf);
    let cursor_y = if compact { 3 } else { 4 };

    // Move to top and overwrite lines cleanly with line-clearing
    crossterm::queue!(out, crossterm::cursor::MoveTo(0, 0))?;
    for row in screen_rows {
        write!(out, "\r\x1b[2K{row}\r\n")?;
    }
    crossterm::queue!(
        out,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown)
    )?;
    crossterm::queue!(
        out,
        crossterm::cursor::MoveTo(cursor_x as u16, cursor_y as u16),
        crossterm::cursor::Show
    )?;
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
    let term_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80);
    let color_enabled = std::env::var_os("NO_COLOR").is_none();

    // Clear alternate screen and move to top-left
    crossterm::queue!(
        out,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;

    // Draw header and command title
    let (h1, h2) = build_header(app_info, term_width, color_enabled);
    writeln!(out, "{h1}\r")?;
    writeln!(out, "{h2}\r")?;

    let title = command_title(cmd_str);
    if color_enabled {
        writeln!(out, "  \x1b[1;36m{title}\x1b[0m\r")?;
        let div = "─".repeat(term_width.min(80));
        writeln!(out, "  \x1b[90m{div}\x1b[0m\r\n")?;
    } else {
        writeln!(out, "  {title}\r")?;
        let div = "-".repeat(term_width.min(80));
        writeln!(out, "  {div}\r\n")?;
    }
    out.flush()?;

    // Temporarily disable raw mode so sub-prompts, password masking, and read_line function normally
    let _ = disable_raw_mode();

    let cmd_res = execute_interactive_command(app, formatter, cmd_str);
    if let Err(e) = cmd_res {
        formatter.print_error(&e);
        crate::platform::signals::reset_interrupted();
    }

    // Print footer and prompt
    let divider = if color_enabled {
        format!("\x1b[90m{}\x1b[0m", "─".repeat(term_width.min(80)))
    } else {
        "-".repeat(term_width.min(80))
    };
    println!("\r\n  {divider}");
    if color_enabled {
        println!("  \x1b[1mPress [Enter] or [Esc] to return to dashboard\x1b[0m");
    } else {
        println!("  Press [Enter] or [Esc] to return to dashboard");
    }
    let _ = out.flush();

    // Re-enable raw mode to wait for dismissal key
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

/// TTY interactive loop supporting raw mode, full-screen alternate buffer, palette, and archive drag-and-drop.
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
    let mut archive_action = ArchiveAction::Extract;

    render_dashboard(
        &app_info,
        &buffer,
        selected_index,
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
                    let candidate_str = clean_terminal_path(buffer.trim());
                    let candidate_path = PathBuf::from(&candidate_str);
                    let is_archive_detected = !candidate_str.is_empty()
                        && candidate_path.is_file()
                        && crate::archive::ArchiveFormat::from_extension(&candidate_path).is_some();

                    if is_archive_detected {
                        match key.code {
                            KeyCode::Tab | KeyCode::Right => {
                                archive_action = match archive_action {
                                    ArchiveAction::Extract => ArchiveAction::Test,
                                    ArchiveAction::Test => ArchiveAction::Cancel,
                                    ArchiveAction::Cancel => ArchiveAction::Extract,
                                };
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Left => {
                                archive_action = match archive_action {
                                    ArchiveAction::Extract => ArchiveAction::Cancel,
                                    ArchiveAction::Test => ArchiveAction::Extract,
                                    ArchiveAction::Cancel => ArchiveAction::Test,
                                };
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Char('e') | KeyCode::Char('E') => {
                                let continue_app = execute_command_in_tui(
                                    app,
                                    formatter,
                                    &format!("/extract \"{candidate_str}\""),
                                    &app_info,
                                )?;
                                if !continue_app {
                                    break;
                                }
                                buffer.clear();
                                selected_index = 0;
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Char('t') | KeyCode::Char('T') => {
                                let continue_app = execute_command_in_tui(
                                    app,
                                    formatter,
                                    &format!("/test \"{candidate_str}\""),
                                    &app_info,
                                )?;
                                if !continue_app {
                                    break;
                                }
                                buffer.clear();
                                selected_index = 0;
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Esc => {
                                buffer.clear();
                                selected_index = 0;
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                                continue;
                            }
                            KeyCode::Enter => {
                                match archive_action {
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
                                archive_action = ArchiveAction::Extract;
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
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
                            selected_index = 0;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Backspace => {
                            buffer.pop();
                            selected_index = 0;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Up => {
                            if buffer.starts_with('/') {
                                selected_index = selected_index.saturating_sub(1);
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                            }
                        }
                        KeyCode::Down => {
                            if buffer.starts_with('/') {
                                let filtered = filter_suggestions(&buffer);
                                if !filtered.is_empty() && selected_index + 1 < filtered.len() {
                                    selected_index += 1;
                                }
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                            }
                        }
                        KeyCode::Tab => {
                            if buffer.starts_with('/') {
                                let filtered = filter_suggestions(&buffer);
                                if !filtered.is_empty() {
                                    let idx = selected_index.min(filtered.len() - 1);
                                    buffer = filtered[idx].command.to_string();
                                    selected_index = 0;
                                }
                                render_dashboard(
                                    &app_info,
                                    &buffer,
                                    selected_index,
                                    archive_action,
                                    color_enabled,
                                )?;
                            }
                        }
                        KeyCode::Enter => {
                            let trimmed = buffer.trim();
                            if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
                                break;
                            }

                            let filtered = filter_suggestions(&buffer);
                            let cmd_to_run = if buffer.starts_with('/')
                                && !buffer.contains(' ')
                                && !filtered.is_empty()
                            {
                                let idx = selected_index.min(filtered.len() - 1);
                                filtered[idx].command.to_string()
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
                            archive_action = ArchiveAction::Extract;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        KeyCode::Esc => {
                            buffer.clear();
                            selected_index = 0;
                            archive_action = ArchiveAction::Extract;
                            render_dashboard(
                                &app_info,
                                &buffer,
                                selected_index,
                                archive_action,
                                color_enabled,
                            )?;
                        }
                        _ => {}
                    }
                }
                Ok(Event::Resize(_, _)) => {
                    render_dashboard(
                        &app_info,
                        &buffer,
                        selected_index,
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

    // Check if input is a dragged filesystem archive path
    let cleaned_candidate = clean_terminal_path(trimmed);
    let candidate_path = Path::new(&cleaned_candidate);
    let is_supported_archive = candidate_path.is_file()
        && crate::archive::ArchiveFormat::from_extension(candidate_path).is_some();

    if is_supported_archive {
        let display_path = crate::cli::progress::sanitize_terminal_text(&cleaned_candidate);
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
            return run_extract_interactive(app, formatter, Some(cleaned_candidate), None);
        } else if choice_trimmed.eq_ignore_ascii_case("t")
            || choice_trimmed.eq_ignore_ascii_case("test")
        {
            return run_test_interactive(app, formatter, Some(cleaned_candidate));
        } else {
            if !formatter.is_quiet() {
                println!("{}", formatter.style("Operation cancelled.", "\x1b[90m"));
            }
            return Ok(());
        }
    }

    let sanitized_unknown = crate::cli::progress::sanitize_terminal_text(trimmed);
    println!("Unrecognized command: '{sanitized_unknown}'. Type '/help' for available commands.");
    Ok(())
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
            print!("Enter archive path: ");
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
    let archive_path = PathBuf::from(archive_str);

    let show_progress = formatter.should_show_progress();
    if !show_progress && !formatter.is_quiet() {
        println!("Testing archive integrity, please wait...");
    }
    let res = crate::cli::run_test_with_prompt(
        app,
        &archive_path,
        &crate::cli::TerminalPasswordPrompter,
        show_progress,
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
            print!("Enter archive path: ");
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
    let archive_path = PathBuf::from(archive_str);

    let dest_opt_buf: Option<PathBuf> = match inline_dest {
        Some(dest) if !dest.trim().is_empty() => Some(PathBuf::from(clean_terminal_path(&dest))),
        Some(_) => None,
        None => {
            print!("Enter destination directory (leave empty for default): ");
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
    fn test_render_box_unicode_and_ascii() {
        let content = vec!["Hello World".to_string()];
        let u_box = render_box("Title", &content, 40, true);
        assert_eq!(u_box.len(), 3);
        assert!(u_box[0].starts_with("┌─"));
        assert!(u_box[0].ends_with('┐'));
        assert!(u_box[1].starts_with("│  "));
        assert!(u_box[1].ends_with("  │"));
        assert!(u_box[2].starts_with("└─"));
        assert!(u_box[2].ends_with('┘'));

        let a_box = render_box("Title", &content, 40, false);
        assert_eq!(a_box.len(), 3);
        assert!(a_box[0].starts_with("+-"));
        assert!(a_box[0].ends_with('+'));
        assert!(a_box[1].starts_with("|  "));
        assert!(a_box[1].ends_with("  |"));
        assert!(a_box[2].starts_with("+-"));
        assert!(a_box[2].ends_with('+'));
    }

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("hello", 10), "hello");
        assert_eq!(truncate_chars("hello world", 5), "hell…");
        assert_eq!(truncate_chars("abc", 2), "ab");
    }

    #[test]
    fn test_command_title_mapping() {
        assert_eq!(command_title("/info"), "SYSTEM INFORMATION (/info)");
        assert_eq!(
            command_title("/doctor"),
            "SYSTEM DIAGNOSTICS & HEALTH CHECK (/doctor)"
        );
        assert_eq!(command_title("/update"), "SELF-UPDATE STATUS (/update)");
        assert_eq!(command_title("/config"), "SECURITY CONFIGURATION (/config)");
        assert_eq!(command_title("/help"), "COMMAND REFERENCE (/help)");
        assert_eq!(
            command_title("/extract /tmp/a.zip"),
            "ARCHIVE EXTRACTION (/extract)"
        );
        assert_eq!(
            command_title("/test /tmp/a.zip"),
            "ARCHIVE INTEGRITY TEST (/test)"
        );
    }
}
