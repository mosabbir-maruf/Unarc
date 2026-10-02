//! Command-line interface and presentation layer.

pub mod args;
pub mod interactive;
pub mod output;

pub use args::{Cli, Commands, ExtractArgs, TestArgs};
pub use interactive::run_interactive;
pub use output::OutputFormatter;

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

/// Standard terminal password prompter using masked terminal input.
#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalPasswordPrompter;

impl PasswordPrompter for TerminalPasswordPrompter {
    fn is_interactive(&self) -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal()
    }

    fn prompt_password(&self, prompt: &str) -> std::io::Result<String> {
        rpassword::prompt_password(prompt)
    }
}

/// Runs an archive integrity test with interactive password prompt fallback if encrypted.
pub fn run_test_with_prompt<P: PasswordPrompter>(
    app: &Application,
    archive: &Path,
    prompter: &P,
) -> Result<ArchiveTestResult> {
    match app.test_archive(archive, None) {
        Ok(r) => Ok(r),
        Err(e @ UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            if !prompter.is_interactive() {
                return Err(e);
            }
            let password = prompter
                .prompt_password("Enter archive password: ")
                .unwrap_or_default();
            app.test_archive(archive, Some(&password))
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
) -> Result<ArchiveExtractResult> {
    match app.extract_archive(archive, output, None) {
        Ok(r) => Ok(r),
        Err(e @ UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
            if !prompter.is_interactive() {
                return Err(e);
            }
            let password = prompter
                .prompt_password("Enter archive password: ")
                .unwrap_or_default();
            app.extract_archive(archive, output, Some(&password))
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
            let res = match run_test_with_prompt(&app, &args.archive, prompter) {
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
            let res = match run_extract_with_prompt(
                &app,
                &args.archive,
                args.output.as_deref(),
                prompter,
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
