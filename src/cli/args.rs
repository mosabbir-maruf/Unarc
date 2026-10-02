//! Command-line interface argument definitions.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Production-grade, security-focused archive utility.
#[derive(Debug, Parser, Clone, PartialEq, Eq)]
#[command(
    name = "unarc",
    version,
    about = "Production-grade, security-focused archive utility",
    long_about = "Unarc is a security-focused CLI utility with zero-trust extraction defaults."
)]
pub struct Cli {
    /// Output in structured JSON format.
    #[arg(long, global = true)]
    pub json: bool,

    /// Increase logging/output verbosity.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress non-essential output.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available Unarc subcommands.
#[derive(Debug, Subcommand, Clone, PartialEq, Eq)]
pub enum Commands {
    /// Extract an archive securely with boundary enforcement.
    Extract(ExtractArgs),

    /// Test the integrity of an archive without writing to disk.
    Test(TestArgs),

    /// Display platform information, security policy status, and engine details.
    Info,

    /// Display version information.
    Version,
}

/// Arguments for the `extract` subcommand.
#[derive(Debug, Args, Clone, PartialEq, Eq)]
pub struct ExtractArgs {
    /// Path to the archive file to extract.
    #[arg(value_name = "ARCHIVE")]
    pub archive: PathBuf,

    /// Optional destination directory for extracted contents.
    #[arg(short, long, value_name = "OUTPUT")]
    pub output: Option<PathBuf>,
}

/// Arguments for the `test` subcommand.
#[derive(Debug, Args, Clone, PartialEq, Eq)]
pub struct TestArgs {
    /// Path to the archive file to test.
    #[arg(value_name = "ARCHIVE")]
    pub archive: PathBuf,
}
