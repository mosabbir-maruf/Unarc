//! Command-line interface and presentation layer.

pub mod args;
pub mod interactive;
pub mod output;
pub mod password;
pub mod progress;

pub use args::{Cli, Commands, ExtractArgs, TestArgs};
pub use interactive::run_interactive;
pub use output::OutputFormatter;
pub use password::{
    EYE_TOGGLE_GLYPH, MASK_BULLET_GLYPH, PasswordPromptState, render_password_prompt,
};

use crate::archive::{ArchiveExtractResult, ArchiveTestResult};
use crate::core::Application;
use crate::error::{ArchiveError, Result, UnarcError};
use clap::Parser;
use std::path::Path;

/// Entrypoint for CLI execution, parses command line arguments and delegates to core application.
pub fn run() -> Result<()> {
    let _ = crate::platform::signals::install_signal_handlers();
    let cli = Cli::parse();
    run_with_cli(cli)
}

/// Trait abstraction for prompting passwords in interactive environments.
pub trait PasswordPrompter {
    /// Returns true if the standard input is attached to an interactive terminal.
    fn is_interactive(&self) -> bool;

    /// Prompts the user for password without echoing input.
    fn prompt_password(&self, prompt: &str) -> std::io::Result<String>;
}

/// Standard terminal password prompter using masked terminal input with eye toggle and character count.
#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalPasswordPrompter;

impl PasswordPrompter for TerminalPasswordPrompter {
    fn is_interactive(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    fn prompt_password(&self, prompt: &str) -> std::io::Result<String> {
        let left_margin = prompt.chars().take_while(|c| *c == ' ').count();
        let color_enabled = std::env::var_os("NO_COLOR").is_none();
        crate::cli::password::prompt_password_terminal(left_margin, color_enabled)
    }
}

#[must_use]
pub fn format_password_prompt(left_margin: usize) -> String {
    crate::cli::password::render_password_prompt(left_margin, "", false)
}

/// Runs an archive integrity test with interactive password prompt fallback if encrypted.
pub fn run_test_with_prompt<P: PasswordPrompter>(
    app: &Application,
    archive: &Path,
    prompter: &P,
    show_progress: bool,
    left_margin: usize,
) -> Result<ArchiveTestResult> {
    let archive_bytes = std::fs::metadata(archive)
        .ok()
        .map(|m| m.len())
        .unwrap_or(0);
    let run_with_bar = |pwd: Option<&str>| -> Result<ArchiveTestResult> {
        if show_progress {
            let mut bar = crate::cli::progress::ProgressBar::new("Testing", left_margin);
            if archive_bytes > 0 {
                bar.set_total_bytes(archive_bytes);
            }
            let res = app.test_archive_with_progress(archive, pwd, Some(&mut bar));
            bar.finish();
            res
        } else {
            app.test_archive(archive, pwd)
        }
    };

    match run_with_bar(None) {
        Ok(r) => Ok(r),
        Err(e @ UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            if !prompter.is_interactive() {
                return Err(e);
            }
            let prompt = format_password_prompt(left_margin);
            let password = match prompter.prompt_password(&prompt) {
                Ok(p) => p,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    return Err(UnarcError::Interrupted);
                }
                Err(_) => String::new(),
            };
            run_with_bar(Some(&password))
        }
        Err(e) => Err(e),
    }
}

/// Runs an archive extraction with interactive password prompt fallback if encrypted.
pub fn run_extract_with_prompt<P: PasswordPrompter>(
    app: &Application,
    archive: &Path,
    output: Option<&Path>,
    prompter: &P,
    show_progress: bool,
    left_margin: usize,
) -> Result<ArchiveExtractResult> {
    // Centralized preflight validation executes upfront before creating progress UI or spawning engine
    let preflight = app.preflight_extract(archive, output)?;
    let archive_bytes: u64 = preflight
        .volume_set
        .volumes
        .iter()
        .filter_map(|v| std::fs::metadata(v).ok().map(|m| m.len()))
        .sum();

    let run_with_bar = |pwd: Option<&str>| -> Result<ArchiveExtractResult> {
        if show_progress {
            let mut bar = crate::cli::progress::ProgressBar::new("Extracting", left_margin);
            if archive_bytes > 0 {
                bar.set_total_bytes(archive_bytes);
            }
            let res = app.extract_with_preflight(&preflight, pwd, Some(&mut bar));
            bar.finish();
            res
        } else {
            app.extract_with_preflight(&preflight, pwd, None)
        }
    };

    match run_with_bar(None) {
        Ok(r) => Ok(r),
        Err(e @ UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            if !prompter.is_interactive() {
                return Err(e);
            }
            let prompt = format_password_prompt(left_margin);
            let password = match prompter.prompt_password(&prompt) {
                Ok(p) => p,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    return Err(UnarcError::Interrupted);
                }
                Err(_) => String::new(),
            };
            run_with_bar(Some(&password))
        }
        Err(e) => Err(e),
    }
}

/// Runs CLI execution logic with pre-parsed arguments using the standard terminal prompter.
pub fn run_with_cli(cli: Cli) -> Result<()> {
    run_with_cli_and_prompter(cli, &TerminalPasswordPrompter)
}

/// Runs CLI execution logic with pre-parsed arguments and a specified password prompter.
pub fn run_with_cli_and_prompter<P: PasswordPrompter>(cli: Cli, prompter: &P) -> Result<()> {
    let formatter = OutputFormatter::new(cli.json, cli.quiet, cli.verbose);
    let app = Application::default();

    match cli.command {
        None => {
            // Interactive mode when launched without a subcommand
            if let Err(e) = run_interactive(&app, cli.quiet, cli.verbose, cli.json) {
                formatter.print_error(&e);
                return Err(e);
            }
            Ok(())
        }
        Some(Commands::Version) => {
            formatter.print_version();
            Ok(())
        }
        Some(Commands::Info) => {
            let info = app.app_info();
            formatter.print_info(&info);
            Ok(())
        }
        Some(Commands::Doctor) => {
            let report = app.doctor_check();
            formatter.print_doctor(&report);
            Ok(())
        }
        Some(Commands::Test(args)) => {
            let show_progress = formatter.should_show_progress();
            let res = match run_test_with_prompt(&app, &args.archive, prompter, show_progress, 0) {
                Ok(r) => r,
                Err(e) => {
                    formatter.print_error(&e);
                    return Err(e);
                }
            };
            formatter.print_test_result(&res);
            Ok(())
        }
        Some(Commands::Extract(args)) => {
            let show_progress = formatter.should_show_progress();
            let res = match run_extract_with_prompt(
                &app,
                &args.archive,
                args.output.as_deref(),
                prompter,
                show_progress,
                0,
            ) {
                Ok(r) => r,
                Err(e) => {
                    formatter.print_error(&e);
                    return Err(e);
                }
            };
            formatter.print_extract_result(&res);
            Ok(())
        }
        Some(Commands::Update(args)) => {
            let manager = crate::core::update::UpdateManager::default();
            if args.check {
                match manager.check_for_update(args.source.as_deref()) {
                    Ok(check_res) => {
                        formatter.print_update_check(&check_res);
                        Ok(())
                    }
                    Err(e) => {
                        formatter.print_error(&e);
                        Err(e)
                    }
                }
            } else {
                match manager.apply_update(args.source.as_deref(), None) {
                    Ok(apply_res) => {
                        formatter.print_update_apply(&apply_res);
                        Ok(())
                    }
                    Err(e) => {
                        formatter.print_error(&e);
                        Err(e)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_password_prompt_alignment() {
        assert_eq!(format_password_prompt(0), "Password    (0)  ◉");
        assert_eq!(format_password_prompt(4), "    Password    (0)  ◉");
        assert_eq!(
            format_password_prompt(20),
            "                    Password    (0)  ◉"
        );
    }
}
