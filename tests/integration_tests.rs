use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use unarc::cli::args::{Cli, Commands, InspectArgs, ValidateArgs};
use unarc::cli::run_with_cli;
use unarc::error::{ArchiveError, SecurityError, UnarcError};

#[test]
fn test_cli_info_command() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Commands::Info,
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_info_json_command() {
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Commands::Info,
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_validate_safe_path() {
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Commands::Validate(ValidateArgs {
            path: PathBuf::from("extracted/safe_file.txt"),
            base_dir: Some(PathBuf::from("/tmp/destination")),
        }),
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_validate_traversal_path() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Commands::Validate(ValidateArgs {
            path: PathBuf::from("../evil.txt"),
            base_dir: None,
        }),
    };
    let res = run_with_cli(cli);
    assert!(matches!(
        res,
        Err(UnarcError::Security(SecurityError::PathTraversal { .. }))
    ));
}

#[test]
fn test_cli_inspect_nonexistent() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Commands::Inspect(InspectArgs {
            archive: PathBuf::from("non_existent_archive_file.zip"),
        }),
    };
    let res = run_with_cli(cli);
    assert!(matches!(
        res,
        Err(UnarcError::Archive(ArchiveError::FileNotFound { .. }))
    ));
}

#[test]
fn test_cli_inspect_valid_zip() {
    let temp_dir = std::env::temp_dir();
    let temp_zip = temp_dir.join("cli_test_archive.zip");

    let mut file = File::create(&temp_zip).unwrap();
    file.write_all(
        b"PK\x05\x06\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    )
    .unwrap();
    drop(file);

    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Commands::Inspect(InspectArgs {
            archive: temp_zip.clone(),
        }),
    };
    assert!(run_with_cli(cli).is_ok());

    let _ = std::fs::remove_file(temp_zip);
}
