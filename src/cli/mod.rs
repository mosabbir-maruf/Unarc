//! Command-line interface and presentation layer.

pub mod args;
pub mod interactive;
pub mod output;

pub use args::{Cli, Commands, ExtractArgs, TestArgs};
pub use interactive::run_interactive;
pub use output::OutputFormatter;

use crate::core::Application;
use crate::error::{ArchiveError, Result, UnarcError};
use clap::Parser;

/// Entrypoint for CLI execution, parses command line arguments and delegates to core application.
pub fn run() -> Result<()> {
    let _ = crate::platform::signals::install_signal_handlers();
    let cli = Cli::parse();
    run_with_cli(cli)
}

/// Runs CLI execution logic with pre-parsed arguments.
pub fn run_with_cli(cli: Cli) -> Result<()> {
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
            let res = match app.test_archive(&args.archive, None) {
                Ok(r) => r,
                Err(UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
                    let password = std::env::var("UNARC_PASSWORD").unwrap_or_else(|_| {
                        rpassword::prompt_password("Enter archive password: ").unwrap_or_default()
                    });
                    match app.test_archive(&args.archive, Some(&password)) {
                        Ok(r) => r,
                        Err(e) => {
                            formatter.print_error(&e);
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    formatter.print_error(&e);
                    return Err(e);
                }
            };
            formatter.print_test_result(&res);
            Ok(())
        }
        Some(Commands::Extract(args)) => {
            let res = match app.extract_archive(&args.archive, args.output.as_deref(), None) {
                Ok(r) => r,
                Err(UnarcError::Archive(ArchiveError::PasswordRequired { .. })) => {
                    let password = std::env::var("UNARC_PASSWORD").unwrap_or_else(|_| {
                        rpassword::prompt_password("Enter archive password: ").unwrap_or_default()
                    });
                    match app.extract_archive(
                        &args.archive,
                        args.output.as_deref(),
                        Some(&password),
                    ) {
                        Ok(r) => r,
                        Err(e) => {
                            formatter.print_error(&e);
                            return Err(e);
                        }
                    }
                }
                Err(e) => {
                    formatter.print_error(&e);
                    return Err(e);
                }
            };
            formatter.print_extract_result(&res);
            Ok(())
        }
    }
}
