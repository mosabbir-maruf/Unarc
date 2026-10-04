//! Terminal-native interactive shell and suggestion engine.

use crate::core::Application;
use crate::error::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{stdin, stdout, IsTerminal, Write};
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

/// Runs the interactive shell.
pub fn run_interactive(
    app: &Application,
    quiet: bool,
    verbose: bool,
    json_mode: bool,
) -> Result<()> {
    let formatter = crate::cli::output::OutputFormatter::new(json_mode, quiet, verbose);

    if !quiet && !json_mode {
        println!("{ASCII_BANNER}");
        println!("Type '/' to view commands, or '/help' for usage guidance.");
    }

    // Non-TTY fallback for piped or automated execution
    if !stdin().is_terminal() || !stdout().is_terminal() {
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

/// RAII guard ensuring terminal raw mode is properly restored on exit.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Returns the styled interactive prompt prefix.
fn prompt_prefix() -> &'static str {
    if std::env::var_os("NO_COLOR").is_some() {
        "unarc › "
    } else {
        "unarc \x1b[1;36m›\x1b[0m "
    }
}

/// TTY interactive loop supporting raw mode, arrows, Tab completion, and suggestions.
fn run_terminal_loop(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
) -> Result<()> {
    let mut buffer = String::new();
    let mut selected_index = 0usize;

    print_prompt(&buffer, selected_index);

    enable_raw_mode().map_err(|e| {
        crate::error::UnarcError::Platform(crate::error::PlatformError::Unsupported {
            details: format!("Failed to enable terminal raw mode: {e}"),
        })
    })?;
    let _guard = RawModeGuard;

    loop {
        if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                // Exit shortcuts
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                {
                    print!("\r\x1b[2K{}\x1b[J\r\nExiting.\r\n", prompt_prefix());
                    let _ = stdout().flush();
                    break;
                }

                match key.code {
                    KeyCode::Char(c) => {
                        buffer.push(c);
                        selected_index = 0;
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Backspace => {
                        buffer.pop();
                        selected_index = 0;
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Up => {
                        selected_index = selected_index.saturating_sub(1);
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Down => {
                        let filtered = filter_suggestions(&buffer);
                        if !filtered.is_empty() && selected_index + 1 < filtered.len() {
                            selected_index += 1;
                        }
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Tab => {
                        let filtered = filter_suggestions(&buffer);
                        if !filtered.is_empty() {
                            let idx = selected_index.min(filtered.len() - 1);
                            buffer = filtered[idx].command.to_string();
                            selected_index = 0;
                        }
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Enter => {
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

                        // Clear suggestions below the prompt and move to a clean line
                        print!("\r\x1b[2K{}{cmd_to_run}\x1b[J\r\n", prompt_prefix());
                        let _ = stdout().flush();

                        // Temporarily disable raw mode to run commands cleanly
                        let _ = disable_raw_mode();

                        let trimmed = cmd_to_run.trim();
                        if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
                            println!("Exiting.");
                            break;
                        }

                        if !trimmed.is_empty() {
                            if let Err(e) = execute_interactive_command(app, formatter, trimmed) {
                                formatter.print_error(&e);
                                crate::platform::signals::reset_interrupted();
                            }
                            // Add exactly one blank line between completed command output and next prompt
                            println!();
                        }

                        // Re-enable raw mode and reset buffer
                        buffer.clear();
                        selected_index = 0;
                        let _ = enable_raw_mode();
                        print_prompt(&buffer, selected_index);
                    }
                    KeyCode::Esc => {
                        buffer.clear();
                        selected_index = 0;
                        print_prompt(&buffer, selected_index);
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

/// Truncates string to `max_len` unicode characters, appending an ellipsis if truncated.
fn truncate_chars(s: &str, max_len: usize) -> String {
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

/// Renders the prompt and active suggestions inline.
fn print_prompt(buffer: &str, selected_index: usize) {
    let mut out = stdout();
    let prompt = prompt_prefix();
    // Clear line, print prompt, and clear everything below from cursor to screen end
    print!("\r\x1b[2K{prompt}{buffer}\x1b[J");
    let _ = out.flush();

    if buffer.starts_with('/') {
        let filtered = filter_suggestions(buffer);
        if !filtered.is_empty() {
            let color = std::env::var_os("NO_COLOR").is_none();
            let term_width = crossterm::terminal::size()
                .map(|(w, _)| w as usize)
                .unwrap_or(80);
            let max_desc_width = term_width.saturating_sub(18).max(10);

            println!("\r");
            for (i, item) in filtered.iter().enumerate() {
                let desc = truncate_chars(item.description, max_desc_width);
                if i == selected_index {
                    if color {
                        print!(
                            "\x1b[2K  \x1b[1;36m›\x1b[0m \x1b[1;36m{:<10}\x1b[0m \x1b[90m{desc}\x1b[0m\r\n",
                            item.command
                        );
                    } else {
                        print!("\x1b[2K  › {:<10} {desc}\r\n", item.command);
                    }
                } else if color {
                    print!(
                        "\x1b[2K    \x1b[1m{:<10}\x1b[0m \x1b[90m{desc}\x1b[0m\r\n",
                        item.command
                    );
                } else {
                    print!("\x1b[2K    {:<10} {desc}\r\n", item.command);
                }
            }

            // Divider and navigation hints adapted to terminal width
            let divider_len = term_width.saturating_sub(4).min(60);
            let divider = if color {
                "─".repeat(divider_len)
            } else {
                "-".repeat(divider_len)
            };

            let nav_hint = if term_width < 60 {
                "↑/↓ pick • Tab auto • Enter run"
            } else {
                "↑/↓ navigate • Tab complete • Enter run • Esc cancel"
            };

            if color {
                print!("\x1b[2K\x1b[90m  {divider}\x1b[0m\r\n");
                print!("\x1b[2K\x1b[90m  {nav_hint}\x1b[0m\r\n");
            } else {
                print!("\x1b[2K  {divider}\r\n");
                print!("\x1b[2K  {nav_hint}\r\n");
            }

            // Move cursor back up to prompt line (items + divider + hint row + newline = filtered.len() + 3)
            let count = filtered.len() + 3;
            print!("\x1b[{count}A\r\x1b[2K{prompt}{buffer}");
            let _ = out.flush();
        }
    }
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
}
