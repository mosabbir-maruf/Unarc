//! Terminal-native interactive shell and suggestion engine.

use crate::core::Application;
use crate::error::ArchiveError;
use crate::error::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{stdin, stdout, BufRead, IsTerminal, Write};
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
        description: "Engine updater (deferred to future phase)",
    },
    CommandSuggestion {
        command: "/config",
        description: "Runtime configuration (deferred to future phase)",
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
    let input = stdin();
    let reader = input.lock();

    for line in reader.lines() {
        let text = match line {
            Ok(t) => t,
            Err(_) => break,
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
            break;
        }

        execute_interactive_command(app, formatter, trimmed)?;
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
                    println!("\r\nExiting.");
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
                        let cmd_to_run = if buffer.starts_with('/') && !filtered.is_empty() {
                            let idx = selected_index.min(filtered.len() - 1);
                            filtered[idx].command.to_string()
                        } else {
                            buffer.clone()
                        };

                        // Temporarily disable raw mode to run commands cleanly
                        let _ = disable_raw_mode();
                        println!("\r");

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

/// Renders the prompt and active suggestions inline.
fn print_prompt(buffer: &str, selected_index: usize) {
    let mut out = stdout();
    // Clear line and return to start
    print!("\r\x1b[2Kunarc> {buffer}");
    let _ = out.flush();

    if buffer.starts_with('/') {
        let filtered = filter_suggestions(buffer);
        if !filtered.is_empty() {
            println!("\r");
            for (i, item) in filtered.iter().enumerate() {
                if i == selected_index {
                    print!(
                        "\x1b[2K  > \x1b[1;36m{:<10}\x1b[0m - {}\r\n",
                        item.command, item.description
                    );
                } else {
                    print!("\x1b[2K    {:<10} - {}\r\n", item.command, item.description);
                }
            }
            // Move cursor back up to prompt line
            let count = filtered.len() + 1;
            print!("\x1b[{count}A\r\x1b[2Kunarc> {buffer}");
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
    match cmd_str {
        "/info" | "info" => {
            let info = app.app_info();
            formatter.print_info(&info);
            Ok(())
        }
        "/doctor" | "doctor" => {
            let doc = app.doctor_check();
            formatter.print_doctor(&doc);
            Ok(())
        }
        "/update" | "update" => {
            formatter.print_deferred_command(
                "/update",
                "Unarc utilizes build-time pinned engines. Dynamic runtime updating is deferred to a future phase.",
            );
            Ok(())
        }
        "/config" | "config" => {
            formatter.print_deferred_command(
                "/config",
                "Runtime configuration editing is deferred to a future phase. Currently enforcing zero-trust policy defaults.",
            );
            Ok(())
        }
        "/help" | "help" => {
            println!("Interactive Commands Reference:");
            for item in SUGGESTIONS {
                println!("  {:<10} {}", item.command, item.description);
            }
            Ok(())
        }
        cmd if cmd.starts_with("/extract") || cmd.starts_with("extract") => {
            prompt_and_run_extract(app, formatter)
        }
        cmd if cmd.starts_with("/test") || cmd.starts_with("test") => {
            prompt_and_run_test(app, formatter)
        }
        unknown => {
            println!("Unrecognized command: '{unknown}'. Type '/help' for available commands.");
            Ok(())
        }
    }
}

/// Prompts for archive path and executes integrity test.
fn prompt_and_run_test(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
) -> Result<()> {
    print!("Enter archive path: ");
    let _ = stdout().flush();
    let mut line = String::new();
    stdin().read_line(&mut line)?;
    let archive_path = PathBuf::from(line.trim().trim_matches('"'));

    // Try testing without password first
    match app.test_archive(&archive_path, None) {
        Ok(res) => {
            formatter.print_test_result(&res);
            Ok(())
        }
        Err(e @ crate::error::UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            // Prompt for password interactively only if stdin is a real terminal
            use std::io::IsTerminal;
            if !std::io::stdin().is_terminal() {
                return Err(e);
            }
            let password =
                rpassword::prompt_password("Enter archive password: ").unwrap_or_default();
            let res = app.test_archive(&archive_path, Some(&password))?;
            formatter.print_test_result(&res);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Prompts for archive path, output directory, and executes extraction.
fn prompt_and_run_extract(
    app: &Application,
    formatter: &crate::cli::output::OutputFormatter,
) -> Result<()> {
    print!("Enter archive path: ");
    let _ = stdout().flush();
    let mut archive_line = String::new();
    stdin().read_line(&mut archive_line)?;
    let archive_path = PathBuf::from(archive_line.trim().trim_matches('"'));

    print!("Enter destination directory (leave empty for default): ");
    let _ = stdout().flush();
    let mut dest_line = String::new();
    stdin().read_line(&mut dest_line)?;
    let dest_trimmed = dest_line.trim().trim_matches('"');
    let dest_opt: Option<&Path> = if dest_trimmed.is_empty() {
        None
    } else {
        Some(Path::new(dest_trimmed))
    };

    // Try extracting without password first
    match app.extract_archive(&archive_path, dest_opt, None) {
        Ok(res) => {
            formatter.print_extract_result(&res);
            Ok(())
        }
        Err(e @ crate::error::UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            use std::io::IsTerminal;
            if !std::io::stdin().is_terminal() {
                return Err(e);
            }
            let password =
                rpassword::prompt_password("Enter archive password: ").unwrap_or_default();
            let res = app.extract_archive(&archive_path, dest_opt, Some(&password))?;
            formatter.print_extract_result(&res);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
