use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use unarc::archive::bundled::{
    resolve_bundled_engine, EXPECTED_SHA256_LINUX_ARM64, EXPECTED_SHA256_LINUX_X64,
    EXPECTED_SHA256_MACOS, PINNED_7ZIP_RELEASE_URL, PINNED_7ZIP_VERSION,
};
use unarc::cli::args::{Cli, Commands, ExtractArgs, TestArgs};
use unarc::cli::interactive::{execute_interactive_command, filter_suggestions};
use unarc::cli::output::OutputFormatter;
use unarc::cli::run_with_cli;
use unarc::core::Application;
use unarc::error::{ArchiveError, UnarcError};

#[test]
fn test_cli_version_command() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: false,
        command: Some(Commands::Version),
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_info_command() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Info),
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_info_json_command() {
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Some(Commands::Info),
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_test_nonexistent_file() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Test(TestArgs {
            archive: PathBuf::from("does_not_exist.7z"),
        })),
    };
    let res = run_with_cli(cli);
    assert!(matches!(
        res,
        Err(UnarcError::Archive(ArchiveError::FileNotFound { .. }))
    ));
}

#[test]
fn test_cli_extract_nonexistent_file() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: PathBuf::from("missing.zip"),
            output: Some(PathBuf::from("/tmp/out")),
        })),
    };
    let res = run_with_cli(cli);
    assert!(matches!(
        res,
        Err(UnarcError::Archive(ArchiveError::FileNotFound { .. }))
    ));
}

#[test]
fn test_interactive_suggestions_filtering() {
    let all = filter_suggestions("");
    assert_eq!(all.len(), 8);

    let ex = filter_suggestions("/ex");
    assert_eq!(ex.len(), 2);
    assert!(ex.iter().any(|s| s.command == "/extract"));
    assert!(ex.iter().any(|s| s.command == "/exit"));

    let doc = filter_suggestions("/doc");
    assert_eq!(doc.len(), 1);
    assert_eq!(doc[0].command, "/doctor");

    let t = filter_suggestions("/t");
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].command, "/test");
}

#[test]
fn test_interactive_commands_execution() {
    let app = Application::default();
    let formatter = OutputFormatter::new(false, true, false);

    assert!(execute_interactive_command(&app, &formatter, "/info").is_ok());
    assert!(execute_interactive_command(&app, &formatter, "/doctor").is_ok());
    assert!(execute_interactive_command(&app, &formatter, "/config").is_ok());
    assert!(execute_interactive_command(&app, &formatter, "/update").is_ok());
    assert!(execute_interactive_command(&app, &formatter, "/help").is_ok());
}

#[test]
fn test_pinned_engine_metadata() {
    assert_eq!(PINNED_7ZIP_VERSION, "24.09");
    assert_eq!(
        PINNED_7ZIP_RELEASE_URL,
        "https://github.com/ip7z/7zip/releases/tag/24.09"
    );
    assert_eq!(
        EXPECTED_SHA256_MACOS,
        "bd5765978a541323758d82ad1d30df76a2e3c86341f12d6b0524d837411e9b4a"
    );
    assert_eq!(
        EXPECTED_SHA256_LINUX_ARM64,
        "ea6a2595eba6441e1e60ddaa47d73d849e99ef2ba18d3f386557cdcb9dc9cebd"
    );
    assert_eq!(
        EXPECTED_SHA256_LINUX_X64,
        "9a556170350dafb60a97348b86a94b087d97fd36007760691576cac0d88b132b"
    );
}

#[test]
fn test_output_formatter_no_color_behavior() {
    std::env::set_var("NO_COLOR", "1");
    let formatter = OutputFormatter::new(false, false, false);
    // When NO_COLOR is set, color_enabled is false
    let app = Application::default();
    let info = app.app_info();
    formatter.print_info(&info);
    std::env::remove_var("NO_COLOR");
}

#[test]
fn test_real_archive_test_and_extract_if_engine_available() {
    if resolve_bundled_engine().is_err() {
        eprintln!("Skipping engine test: bundled 7zz not in environment");
        return;
    }

    let temp_dir = std::env::temp_dir();
    let test_zip = temp_dir.join("real_engine_test.zip");
    let extract_dir = temp_dir.join("real_engine_extracted");

    // Write minimal valid empty ZIP archive (EOCD record: 22 bytes)
    let mut file = File::create(&test_zip).unwrap();
    file.write_all(
        b"PK\x05\x06\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    )
    .unwrap();
    drop(file);

    let app = Application::default();

    // Direct CLI Test
    let test_res = app.test_archive(&test_zip, None);
    assert!(
        test_res.is_ok(),
        "Expected test_archive to succeed: {test_res:?}"
    );

    // Direct CLI Extract
    let extract_res = app.extract_archive(&test_zip, Some(&extract_dir), None);
    assert!(
        extract_res.is_ok(),
        "Expected extract_archive to succeed: {extract_res:?}"
    );
    assert!(extract_dir.exists());

    let _ = std::fs::remove_file(test_zip);
    let _ = std::fs::remove_dir_all(extract_dir);
}
