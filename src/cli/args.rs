//! Command-line interface argument definitions.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Production-grade, security-focused archive utility.
#[derive(Debug, Parser)]
#[command(
    name = "unarc",
    version,
    about = "Production-grade, security-focused archive utility",
    long_about = "Unarc is a security-focused CLI foundation designed for macOS Apple Silicon and Linux, enforcing zero-trust extraction policies."
)]
pub struct Cli {
    /// Global output in structured JSON format.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase logging/output verbosity.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress non-essential output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available Unarc subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Inspect archive format and metadata.
    Inspect(InspectArgs),

    /// Validate a path or candidate entry against strict security policies.
    Validate(ValidateArgs),

    /// Display platform information, security policy status, and engine capabilities.
    Info,
}

/// Arguments for the `inspect` subcommand.
#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Path to the archive file to inspect.
    #[arg(value_name = "ARCHIVE")]
    pub archive: PathBuf,
}

/// Arguments for the `validate` subcommand.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Candidate path to validate for traversal or security risks.
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Optional destination boundary directory to test containment against.
    #[arg(long, value_name = "BASE_DIR")]
    pub base_dir: Option<PathBuf>,
}
