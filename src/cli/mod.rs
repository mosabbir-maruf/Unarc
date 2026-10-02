//! Command-line interface and presentation layer.

pub mod args;
pub mod output;

pub use args::{Cli, Commands};
pub use output::OutputFormatter;

use crate::core::Application;
use crate::error::Result;
use clap::Parser;

/// Entrypoint for CLI execution, parses command line arguments and delegates to core application.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    run_with_cli(cli)
}

/// Runs CLI execution logic with pre-parsed arguments (useful for integration testing).
pub fn run_with_cli(cli: Cli) -> Result<()> {
    let formatter = OutputFormatter::new(cli.json, cli.quiet);
    let app = Application::default();

    match cli.command {
        Commands::Inspect(args) => match app.inspect_archive(&args.archive) {
            Ok(metadata) => {
                formatter.print_inspection(&args.archive, &metadata);
                Ok(())
            }
            Err(e) => {
                formatter.print_error(&e);
                Err(e)
            }
        },
        Commands::Validate(args) => {
            let sanitized = match app.validate_path(&args.path) {
                Ok(p) => p,
                Err(e) => {
                    formatter.print_error(&e);
                    return Err(e);
                }
            };

            let destination = if let Some(ref base) = args.base_dir {
                match app.validate_destination(base, &sanitized) {
                    Ok(dest) => Some(dest),
                    Err(e) => {
                        formatter.print_error(&e);
                        return Err(e);
                    }
                }
            } else {
                None
            };

            formatter.print_validation(&args.path, &sanitized, destination.as_deref());
            Ok(())
        }
        Commands::Info => {
            let info = app.app_info();
            formatter.print_info(&info);
            Ok(())
        }
    }
}
