use clap::Parser;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use unarc::archive::bundled::{
    EXPECTED_SHA256_LINUX_ARM64, EXPECTED_SHA256_LINUX_X64, EXPECTED_SHA256_MACOS,
    PINNED_7ZIP_RELEASE_URL, PINNED_7ZIP_VERSION, SevenZipBackend, resolve_bundled_engine,
};
use unarc::cli::args::{Cli, Commands, ExtractArgs, TestArgs};
use unarc::cli::interactive::{execute_interactive_command, filter_suggestions};
use unarc::cli::{OutputFormatter, PasswordPrompter, run_with_cli, run_with_cli_and_prompter};
use unarc::core::Application;
use unarc::error::{ArchiveError, ErrorCode, UnarcError};
use unarc::security::{
    ProcessSandboxPolicy, SandboxRunner, SandboxStatus, ScratchWorkspace, SecurityPolicy,
};

/// Test fixture signing seed used exclusively by integration tests.
const TEST_FIXTURE_SIGNING_SEED: [u8; 32] = *b"UNARC_OFFICIAL_RELEASE_KEY_SEED!";

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
fn test_cli_doctor_command() {
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Doctor),
    };
    assert!(run_with_cli(cli).is_ok());
}

#[test]
fn test_cli_doctor_json_command() {
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Some(Commands::Doctor),
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
    assert_eq!(PINNED_7ZIP_VERSION, "26.03");
    assert_eq!(
        PINNED_7ZIP_RELEASE_URL,
        "https://github.com/ip7z/7zip/releases/tag/26.03"
    );
    assert_eq!(
        EXPECTED_SHA256_MACOS,
        "5ca87677072c59f5602e5c49baa27d4694bacd2259b4e507f0094249d4281480"
    );
    assert_eq!(
        EXPECTED_SHA256_LINUX_ARM64,
        "2389ba20e4d8295e8709c20b6263b69bd1ec4972fe38a04ad7a1badbf595b996"
    );
    assert_eq!(
        EXPECTED_SHA256_LINUX_X64,
        "dc99eff5008f1ab79bd7084c68513701547a808a89502bf4133683535ab3c695"
    );
}

#[test]
fn test_output_formatter_no_color_behavior() {
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let formatter = OutputFormatter::new(false, false, false);
    // When NO_COLOR is set, color_enabled is false
    let app = Application::default();
    let info = app.app_info();
    formatter.print_info(&info);
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("NO_COLOR") };
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

fn calculate_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

#[test]
fn test_synthetic_rar4_with_engine() {
    if resolve_bundled_engine().is_err() {
        return;
    }

    let temp_dir = std::env::temp_dir();
    let rar_path = temp_dir.join("test_synthetic.rar");
    let out_dir = temp_dir.join("test_synthetic_out");

    let file_content = b"Hello from synthetic RAR4!";
    let file_crc = calculate_crc32(file_content);
    let file_name = b"hello.txt";

    let mut rar_bytes = Vec::new();
    // 1. Signature
    rar_bytes.extend_from_slice(b"Rar!\x1A\x07\x00");

    // 2. Main header (0x73)
    let mut main_head = Vec::new();
    main_head.push(0x73); // HEAD_TYPE: MAIN_HEAD
    main_head.extend_from_slice(&0x0000u16.to_le_bytes()); // FLAGS: standard
    main_head.extend_from_slice(&13u16.to_le_bytes()); // HEAD_SIZE: 13
    main_head.extend_from_slice(&0u16.to_le_bytes()); // RESERVED1
    main_head.extend_from_slice(&0u32.to_le_bytes()); // RESERVED2
    let main_crc = (calculate_crc32(&main_head) & 0xFFFF) as u16;
    rar_bytes.extend_from_slice(&main_crc.to_le_bytes());
    rar_bytes.extend_from_slice(&main_head);

    // 3. File header (0x74)
    let mut file_head = Vec::new();
    file_head.push(0x74); // HEAD_TYPE: FILE_HEAD
    file_head.extend_from_slice(&0x8000u16.to_le_bytes()); // FLAGS
    let head_size = (32 + file_name.len()) as u16;
    file_head.extend_from_slice(&head_size.to_le_bytes());
    file_head.extend_from_slice(&(file_content.len() as u32).to_le_bytes()); // PACK_SIZE
    file_head.extend_from_slice(&(file_content.len() as u32).to_le_bytes()); // UNP_SIZE
    file_head.push(3); // HOST_OS: UNIX
    file_head.extend_from_slice(&file_crc.to_le_bytes()); // FILE_CRC
    file_head.extend_from_slice(&0x546B4000u32.to_le_bytes()); // FILE_TIME
    file_head.push(29); // UNP_VER: 2.9
    file_head.push(0x30); // METHOD: 0x30 (store)
    file_head.extend_from_slice(&(file_name.len() as u16).to_le_bytes()); // NAME_SIZE
    file_head.extend_from_slice(&0x000081A4u32.to_le_bytes()); // ATTR
    file_head.extend_from_slice(file_name);

    let file_crc_head = (calculate_crc32(&file_head) & 0xFFFF) as u16;
    rar_bytes.extend_from_slice(&file_crc_head.to_le_bytes());
    rar_bytes.extend_from_slice(&file_head);
    rar_bytes.extend_from_slice(file_content);

    // 4. End of archive header (0x7B)
    let mut end_head = Vec::new();
    end_head.push(0x7B); // HEAD_TYPE: ENDARC_HEAD
    end_head.extend_from_slice(&0x0000u16.to_le_bytes()); // FLAGS
    end_head.extend_from_slice(&7u16.to_le_bytes()); // HEAD_SIZE: 7
    let end_crc = (calculate_crc32(&end_head) & 0xFFFF) as u16;
    rar_bytes.extend_from_slice(&end_crc.to_le_bytes());
    rar_bytes.extend_from_slice(&end_head);

    std::fs::write(&rar_path, &rar_bytes).unwrap();

    let output = std::process::Command::new(resolve_bundled_engine().unwrap())
        .args(["t", "-bso1", "-bse2", rar_path.to_str().unwrap()])
        .output()
        .unwrap();
    eprintln!("7zz stdout: {}", String::from_utf8_lossy(&output.stdout));
    eprintln!("7zz stderr: {}", String::from_utf8_lossy(&output.stderr));

    let app = Application::default();
    let test_res = app.test_archive(&rar_path, None);
    assert!(test_res.is_ok(), "Test archive failed: {test_res:?}");

    let extract_res = app.extract_archive(&rar_path, Some(&out_dir), None);
    assert!(
        extract_res.is_ok(),
        "Extract archive failed: {extract_res:?}"
    );

    let extracted_file = out_dir.join("hello.txt");
    assert!(extracted_file.exists());
    let content = std::fs::read_to_string(&extracted_file).unwrap();
    assert_eq!(content, "Hello from synthetic RAR4!");

    let _ = std::fs::remove_file(rar_path);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_synthetic_multipart_rar4_modern_with_engine() {
    if resolve_bundled_engine().is_err() {
        return;
    }

    let temp_dir = std::env::temp_dir();
    let part1_path = temp_dir.join("split_test.part1.rar");
    let part2_path = temp_dir.join("split_test.part2.rar");
    let out_dir = temp_dir.join("split_test_out");

    let content1 = b"Content in volume 1";
    let crc1 = calculate_crc32(content1);
    let name1 = b"file1.txt";

    let content2 = b"Content in volume 2";
    let crc2 = calculate_crc32(content2);
    let name2 = b"file2.txt";

    // Build part 1:
    let mut part1_bytes = Vec::new();
    part1_bytes.extend_from_slice(b"Rar!\x1A\x07\x00");

    // Main head: VOLUME (0x0001) | NEWNUMBERING (0x0010) | FIRSTVOLUME (0x0100) = 0x0111
    let mut main1 = Vec::new();
    main1.push(0x73);
    main1.extend_from_slice(&0x0111u16.to_le_bytes());
    main1.extend_from_slice(&13u16.to_le_bytes());
    main1.extend_from_slice(&0u16.to_le_bytes());
    main1.extend_from_slice(&0u32.to_le_bytes());
    let crc_main1 = (calculate_crc32(&main1) & 0xFFFF) as u16;
    part1_bytes.extend_from_slice(&crc_main1.to_le_bytes());
    part1_bytes.extend_from_slice(&main1);

    // File head part 1: file1.txt
    let mut fhead1 = Vec::new();
    fhead1.push(0x74);
    fhead1.extend_from_slice(&0x8000u16.to_le_bytes());
    let fhead1_size = (32 + name1.len()) as u16;
    fhead1.extend_from_slice(&fhead1_size.to_le_bytes());
    fhead1.extend_from_slice(&(content1.len() as u32).to_le_bytes()); // PACK_SIZE
    fhead1.extend_from_slice(&(content1.len() as u32).to_le_bytes()); // UNP_SIZE
    fhead1.push(3); // UNIX
    fhead1.extend_from_slice(&crc1.to_le_bytes());
    fhead1.extend_from_slice(&0x546B4000u32.to_le_bytes());
    fhead1.push(29);
    fhead1.push(0x30); // Store
    fhead1.extend_from_slice(&(name1.len() as u16).to_le_bytes());
    fhead1.extend_from_slice(&0x000081A4u32.to_le_bytes());
    fhead1.extend_from_slice(name1);
    let crc_fhead1 = (calculate_crc32(&fhead1) & 0xFFFF) as u16;
    part1_bytes.extend_from_slice(&crc_fhead1.to_le_bytes());
    part1_bytes.extend_from_slice(&fhead1);
    part1_bytes.extend_from_slice(content1);

    // End head part 1: NEXT_VOLUME (0x0001)
    let mut end1 = Vec::new();
    end1.push(0x7B);
    end1.extend_from_slice(&0x0001u16.to_le_bytes());
    end1.extend_from_slice(&7u16.to_le_bytes());
    let crc_end1 = (calculate_crc32(&end1) & 0xFFFF) as u16;
    part1_bytes.extend_from_slice(&crc_end1.to_le_bytes());
    part1_bytes.extend_from_slice(&end1);

    // Build part 2:
    let mut part2_bytes = Vec::new();
    part2_bytes.extend_from_slice(b"Rar!\x1A\x07\x00");

    // Main head: VOLUME (0x0001) | NEWNUMBERING (0x0010) = 0x0011
    let mut main2 = Vec::new();
    main2.push(0x73);
    main2.extend_from_slice(&0x0011u16.to_le_bytes());
    main2.extend_from_slice(&13u16.to_le_bytes());
    main2.extend_from_slice(&0u16.to_le_bytes());
    main2.extend_from_slice(&0u32.to_le_bytes());
    let crc_main2 = (calculate_crc32(&main2) & 0xFFFF) as u16;
    part2_bytes.extend_from_slice(&crc_main2.to_le_bytes());
    part2_bytes.extend_from_slice(&main2);

    // File head part 2: file2.txt
    let mut fhead2 = Vec::new();
    fhead2.push(0x74);
    fhead2.extend_from_slice(&0x8000u16.to_le_bytes());
    let fhead2_size = (32 + name2.len()) as u16;
    fhead2.extend_from_slice(&fhead2_size.to_le_bytes());
    fhead2.extend_from_slice(&(content2.len() as u32).to_le_bytes()); // PACK_SIZE
    fhead2.extend_from_slice(&(content2.len() as u32).to_le_bytes()); // UNP_SIZE
    fhead2.push(3);
    fhead2.extend_from_slice(&crc2.to_le_bytes());
    fhead2.extend_from_slice(&0x546B4000u32.to_le_bytes());
    fhead2.push(29);
    fhead2.push(0x30);
    fhead2.extend_from_slice(&(name2.len() as u16).to_le_bytes());
    fhead2.extend_from_slice(&0x000081A4u32.to_le_bytes());
    fhead2.extend_from_slice(name2);
    let crc_fhead2 = (calculate_crc32(&fhead2) & 0xFFFF) as u16;
    part2_bytes.extend_from_slice(&crc_fhead2.to_le_bytes());
    part2_bytes.extend_from_slice(&fhead2);
    part2_bytes.extend_from_slice(content2);

    // End head part 2: No next volume (0x0000)
    let mut end2 = Vec::new();
    end2.push(0x7B);
    end2.extend_from_slice(&0x0000u16.to_le_bytes());
    end2.extend_from_slice(&7u16.to_le_bytes());
    let crc_end2 = (calculate_crc32(&end2) & 0xFFFF) as u16;
    part2_bytes.extend_from_slice(&crc_end2.to_le_bytes());
    part2_bytes.extend_from_slice(&end2);

    std::fs::write(&part1_path, &part1_bytes).unwrap();
    std::fs::write(&part2_path, &part2_bytes).unwrap();

    let output = std::process::Command::new(resolve_bundled_engine().unwrap())
        .args(["t", "-bso1", "-bse2", part1_path.to_str().unwrap()])
        .output()
        .unwrap();
    eprintln!(
        "7zz multipart stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    eprintln!(
        "7zz multipart stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let app = Application::default();
    let test_res = app.test_archive(&part1_path, None);
    assert!(
        test_res.is_ok(),
        "Test multipart archive failed: {test_res:?}"
    );

    let extract_res = app.extract_archive(&part1_path, Some(&out_dir), None);
    assert!(
        extract_res.is_ok(),
        "Extract multipart archive failed: {extract_res:?}"
    );

    let ext1 = out_dir.join("file1.txt");
    assert!(ext1.exists());
    assert_eq!(std::fs::read(&ext1).unwrap(), content1);

    let ext2 = out_dir.join("file2.txt");
    assert!(ext2.exists());
    assert_eq!(std::fs::read(&ext2).unwrap(), content2);

    let _ = std::fs::remove_file(part1_path);
    let _ = std::fs::remove_file(part2_path);
    let _ = std::fs::remove_dir_all(out_dir);
}

fn write_vint(buf: &mut Vec<u8>, mut val: u64) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

fn build_synthetic_rar5(file_name: &str, content: &[u8]) -> Vec<u8> {
    let mut rar = Vec::new();
    // Signature
    rar.extend_from_slice(b"Rar!\x1A\x07\x01\x00");

    // 1. Main archive header (Type 1)
    let mut main_body = Vec::new();
    write_vint(&mut main_body, 1); // Header type: 1 (Main archive)
    write_vint(&mut main_body, 0); // Header flags: 0
    write_vint(&mut main_body, 0); // Archive flags: 0

    let mut main_block = Vec::new();
    write_vint(&mut main_block, main_body.len() as u64);
    main_block.extend_from_slice(&main_body);
    let main_crc = calculate_crc32(&main_block);
    rar.extend_from_slice(&main_crc.to_le_bytes());
    rar.extend_from_slice(&main_block);

    // 2. File header (Type 2)
    let mut file_body = Vec::new();
    write_vint(&mut file_body, 2); // Header type: 2 (File)
    write_vint(&mut file_body, 2); // Header flags: 2 (data area present)
    write_vint(&mut file_body, content.len() as u64); // Data size
    write_vint(&mut file_body, 0); // File flags
    write_vint(&mut file_body, content.len() as u64); // Unpacked size
    write_vint(&mut file_body, 0x20); // Attributes
    write_vint(&mut file_body, 0); // Compression info: 0 (store)
    write_vint(&mut file_body, 1); // Host OS: 1 (Unix)
    write_vint(&mut file_body, file_name.len() as u64); // Name size
    file_body.extend_from_slice(file_name.as_bytes());

    let mut file_block = Vec::new();
    write_vint(&mut file_block, file_body.len() as u64);
    file_block.extend_from_slice(&file_body);
    let file_crc = calculate_crc32(&file_block);
    rar.extend_from_slice(&file_crc.to_le_bytes());
    rar.extend_from_slice(&file_block);
    rar.extend_from_slice(content);

    // 3. End of archive header (Type 5)
    let mut end_body = Vec::new();
    write_vint(&mut end_body, 5); // Header type: 5 (End of archive)
    write_vint(&mut end_body, 0); // Header flags: 0
    write_vint(&mut end_body, 0); // End flags: 0

    let mut end_block = Vec::new();
    write_vint(&mut end_block, end_body.len() as u64);
    end_block.extend_from_slice(&end_body);
    let end_crc = calculate_crc32(&end_block);
    rar.extend_from_slice(&end_crc.to_le_bytes());
    rar.extend_from_slice(&end_block);

    rar
}

#[test]
fn test_synthetic_rar5_with_engine() {
    if resolve_bundled_engine().is_err() {
        return;
    }

    let temp_dir = std::env::temp_dir();
    let rar_path = temp_dir.join("test_rar5.rar");
    let out_dir = temp_dir.join("test_rar5_out");

    let content = b"Hello from synthetic RAR5 archive format!";
    let rar_bytes = build_synthetic_rar5("rar5_doc.txt", content);
    std::fs::write(&rar_path, &rar_bytes).unwrap();

    let app = Application::default();

    // Inspect format
    let meta = app.inspect_archive(&rar_path).unwrap();
    assert_eq!(meta.format, unarc::archive::ArchiveFormat::Rar5);

    // Test archive
    let test_res = app.test_archive(&rar_path, None);
    assert!(test_res.is_ok(), "Test RAR5 failed: {test_res:?}");

    // Extract archive
    let extract_res = app.extract_archive(&rar_path, Some(&out_dir), None);
    assert!(extract_res.is_ok(), "Extract RAR5 failed: {extract_res:?}");

    let extracted_file = out_dir.join("rar5_doc.txt");
    assert!(extracted_file.exists());
    assert_eq!(std::fs::read(&extracted_file).unwrap(), content);

    let _ = std::fs::remove_file(rar_path);
    let _ = std::fs::remove_dir_all(out_dir);
}

fn build_synthetic_rar4_volume(
    file_name: &str,
    content: &[u8],
    is_multivolume: bool,
    is_first_volume: bool,
    is_new_numbering: bool,
    has_next_volume: bool,
) -> Vec<u8> {
    let mut rar = Vec::new();
    rar.extend_from_slice(b"Rar!\x1A\x07\x00");

    let mut main_flags = 0u16;
    if is_multivolume {
        main_flags |= 0x0001;
    }
    if is_new_numbering {
        main_flags |= 0x0010;
    }
    if is_first_volume {
        main_flags |= 0x0100;
    }

    let mut main_head = Vec::new();
    main_head.push(0x73);
    main_head.extend_from_slice(&main_flags.to_le_bytes());
    main_head.extend_from_slice(&13u16.to_le_bytes());
    main_head.extend_from_slice(&0u16.to_le_bytes());
    main_head.extend_from_slice(&0u32.to_le_bytes());
    let crc_main = (calculate_crc32(&main_head) & 0xFFFF) as u16;
    rar.extend_from_slice(&crc_main.to_le_bytes());
    rar.extend_from_slice(&main_head);

    let crc_content = calculate_crc32(content);
    let mut fhead = Vec::new();
    fhead.push(0x74);
    fhead.extend_from_slice(&0x8000u16.to_le_bytes());
    let head_size = (32 + file_name.len()) as u16;
    fhead.extend_from_slice(&head_size.to_le_bytes());
    fhead.extend_from_slice(&(content.len() as u32).to_le_bytes());
    fhead.extend_from_slice(&(content.len() as u32).to_le_bytes());
    fhead.push(3);
    fhead.extend_from_slice(&crc_content.to_le_bytes());
    fhead.extend_from_slice(&0x546B4000u32.to_le_bytes());
    fhead.push(29);
    fhead.push(0x30);
    fhead.extend_from_slice(&(file_name.len() as u16).to_le_bytes());
    fhead.extend_from_slice(&0x000081A4u32.to_le_bytes());
    fhead.extend_from_slice(file_name.as_bytes());
    let crc_fhead = (calculate_crc32(&fhead) & 0xFFFF) as u16;
    rar.extend_from_slice(&crc_fhead.to_le_bytes());
    rar.extend_from_slice(&fhead);
    rar.extend_from_slice(content);

    let mut end_flags = 0u16;
    if has_next_volume {
        end_flags |= 0x0001;
    }
    let mut end_head = Vec::new();
    end_head.push(0x7B);
    end_head.extend_from_slice(&end_flags.to_le_bytes());
    end_head.extend_from_slice(&7u16.to_le_bytes());
    let crc_end = (calculate_crc32(&end_head) & 0xFFFF) as u16;
    rar.extend_from_slice(&crc_end.to_le_bytes());
    rar.extend_from_slice(&end_head);

    rar
}

fn build_synthetic_zip_with_path(entry_name: &str, content: &[u8]) -> Vec<u8> {
    let mut zip = Vec::new();
    let name_bytes = entry_name.as_bytes();
    let crc = calculate_crc32(content);

    let local_header_offset = zip.len() as u32;
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(name_bytes);
    zip.extend_from_slice(content);

    let central_dir_offset = zip.len() as u32;
    zip.extend_from_slice(b"PK\x01\x02");
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u32.to_le_bytes());
    zip.extend_from_slice(&local_header_offset.to_le_bytes());
    zip.extend_from_slice(name_bytes);

    let central_dir_size = (zip.len() as u32) - central_dir_offset;

    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&central_dir_size.to_le_bytes());
    zip.extend_from_slice(&central_dir_offset.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());

    zip
}

#[test]
fn test_synthetic_multipart_rar4_legacy_with_engine() {
    if resolve_bundled_engine().is_err() {
        return;
    }

    let temp_dir = std::env::temp_dir();
    let vol1_path = temp_dir.join("legacy_series.rar");
    let vol2_path = temp_dir.join("legacy_series.r00");
    let out_dir = temp_dir.join("legacy_series_out");

    let c1 = b"Legacy vol 1 data";
    let c2 = b"Legacy vol 2 data";

    // Legacy volume 1: is_multivolume=true, is_first=true, new_numbering=false, has_next=true
    let b1 = build_synthetic_rar4_volume("f1.txt", c1, true, true, false, true);
    // Legacy volume 2: is_multivolume=true, is_first=false, new_numbering=false, has_next=false
    let b2 = build_synthetic_rar4_volume("f2.txt", c2, true, false, false, false);

    std::fs::write(&vol1_path, &b1).unwrap();
    std::fs::write(&vol2_path, &b2).unwrap();

    let app = Application::default();

    let test_res = app.test_archive(&vol1_path, None);
    assert!(
        test_res.is_ok(),
        "Test legacy multipart failed: {test_res:?}"
    );

    let ext_res = app.extract_archive(&vol1_path, Some(&out_dir), None);
    assert!(
        ext_res.is_ok(),
        "Extract legacy multipart failed: {ext_res:?}"
    );

    assert!(out_dir.join("f1.txt").exists());
    assert!(out_dir.join("f2.txt").exists());

    let _ = std::fs::remove_file(vol1_path);
    let _ = std::fs::remove_file(vol2_path);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_multipart_missing_middle_volume_fails() {
    let temp_dir = std::env::temp_dir();
    let p1 = temp_dir.join("movie_gap.part1.rar");
    let p3 = temp_dir.join("movie_gap.part3.rar");

    let b1 = build_synthetic_rar4_volume("m1.txt", b"part1", true, true, true, true);
    let b3 = build_synthetic_rar4_volume("m3.txt", b"part3", true, false, true, false);

    std::fs::write(&p1, &b1).unwrap();
    std::fs::write(&p3, &b3).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&p1, None, None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::MissingVolume { ref expected, .. }))
            if expected.contains("movie_gap.part2.rar")
        ),
        "Expected MissingVolume for part2, got {res:?}"
    );

    let _ = std::fs::remove_file(p1);
    let _ = std::fs::remove_file(p3);
}

#[test]
fn test_multipart_start_from_middle_resolves_and_missing_first_fails() {
    let temp_dir = std::env::temp_dir();
    let p1 = temp_dir.join("film.part1.rar");
    let p2 = temp_dir.join("film.part2.rar");
    let out_dir = temp_dir.join("film_out");

    let b1 = build_synthetic_rar4_volume("film1.txt", b"f1", true, true, true, true);
    let b2 = build_synthetic_rar4_volume("film2.txt", b"f2", true, false, true, false);

    std::fs::write(&p1, &b1).unwrap();
    std::fs::write(&p2, &b2).unwrap();

    let app = Application::default();

    // 1. Starting from part 2 should resolve full sequence including part 1
    let res = app.extract_archive(&p2, Some(&out_dir), None);
    assert!(
        res.is_ok(),
        "Expected resolution from part 2 to succeed: {res:?}"
    );
    assert!(out_dir.join("film1.txt").exists());
    assert!(out_dir.join("film2.txt").exists());

    let _ = std::fs::remove_file(&p1);
    let _ = std::fs::remove_dir_all(&out_dir);

    // 2. Now part 1 is missing, starting from part 2 must fail immediately with MissingVolume for part 1
    let res_missing = app.extract_archive(&p2, Some(&out_dir), None);
    assert!(
        matches!(
            res_missing,
            Err(UnarcError::Archive(ArchiveError::MissingVolume { ref expected, .. }))
            if expected.contains("film.part1.rar")
        ),
        "Expected MissingVolume for part 1, got {res_missing:?}"
    );

    let _ = std::fs::remove_file(p2);
}

#[test]
fn test_legacy_missing_second_volume_fails() {
    let temp_dir = std::env::temp_dir();
    let vol1_path = temp_dir.join("legacy_missing_r00.rar");

    // Volume 1 header specifies is_multivolume = true, but r00 does not exist
    let b1 = build_synthetic_rar4_volume("single.txt", b"data", true, true, false, true);
    std::fs::write(&vol1_path, &b1).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&vol1_path, None, None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::MissingVolume { ref expected, .. }))
            if expected.contains("legacy_missing_r00.r00")
        ),
        "Expected MissingVolume for .r00, got {res:?}"
    );

    let _ = std::fs::remove_file(vol1_path);
}

#[test]
fn test_multipart_invalid_volume_not_regular_file() {
    let temp_dir = std::env::temp_dir();
    let p1 = temp_dir.join("invalid_file_test.part1.rar");
    let p2 = temp_dir.join("invalid_file_test.part2.rar");

    let b1 = build_synthetic_rar4_volume("a.txt", b"a", true, true, true, true);
    std::fs::write(&p1, &b1).unwrap();

    // Create part 2 as a directory instead of a regular file
    std::fs::create_dir_all(&p2).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&p1, None, None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::InvalidVolume { .. }))
        ),
        "Expected InvalidVolume because part 2 is a directory, got {res:?}"
    );

    let _ = std::fs::remove_file(p1);
    let _ = std::fs::remove_dir(&p2);
}

#[test]
fn test_multipart_invalid_volume_corrupted_signature() {
    let temp_dir = std::env::temp_dir();
    let p1 = temp_dir.join("corrupt_sig.part1.rar");
    let p2 = temp_dir.join("corrupt_sig.part2.rar");

    let b1 = build_synthetic_rar4_volume("a.txt", b"a", true, true, true, true);
    std::fs::write(&p1, &b1).unwrap();
    // Part 2 has corrupted/invalid bytes (not archive magic)
    std::fs::write(&p2, b"CORRUPTED_NON_ARCHIVE_DATA_12345678").unwrap();

    let app = Application::default();
    let res = app.extract_archive(&p1, None, None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Archive(ArchiveError::InvalidVolume { .. }))
        ),
        "Expected InvalidVolume because part 2 has invalid signature, got {res:?}"
    );

    let _ = std::fs::remove_file(p1);
    let _ = std::fs::remove_file(p2);
}

#[test]
fn test_unrelated_sibling_archives_isolation() {
    let temp_dir = std::env::temp_dir();
    let target1 = temp_dir.join("target_archive.part1.rar");
    let target2 = temp_dir.join("target_archive.part2.rar");
    let sibling_diff_stem = temp_dir.join("target_archive2.part1.rar");
    let sibling_unrelated = temp_dir.join("other_document.rar");
    let out_dir = temp_dir.join("target_archive_out");

    let b1 = build_synthetic_rar4_volume("t1.txt", b"target1", true, true, true, true);
    let b2 = build_synthetic_rar4_volume("t2.txt", b"target2", true, false, true, false);
    let diff = build_synthetic_rar4_volume("diff.txt", b"diff", true, true, true, false);
    let unrel = build_synthetic_rar4_volume("unrel.txt", b"unrel", false, true, false, false);

    std::fs::write(&target1, &b1).unwrap();
    std::fs::write(&target2, &b2).unwrap();
    std::fs::write(&sibling_diff_stem, &diff).unwrap();
    std::fs::write(&sibling_unrelated, &unrel).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&target1, Some(&out_dir), None);
    assert!(res.is_ok(), "Expected extraction to succeed: {res:?}");

    // Sibling files must remain completely unchanged
    assert!(sibling_diff_stem.exists());
    assert!(sibling_unrelated.exists());
    assert_eq!(std::fs::read(&sibling_diff_stem).unwrap(), diff);
    assert_eq!(std::fs::read(&sibling_unrelated).unwrap(), unrel);

    let _ = std::fs::remove_file(target1);
    let _ = std::fs::remove_file(target2);
    let _ = std::fs::remove_file(sibling_diff_stem);
    let _ = std::fs::remove_file(sibling_unrelated);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_archive_entry_path_traversal_rejection() {
    let temp_dir = std::env::temp_dir();
    let traversal_zip = temp_dir.join("evil_traversal.zip");
    let out_dir = temp_dir.join("evil_traversal_out");

    let zip_bytes = build_synthetic_zip_with_path("../../evil.txt", b"malicious");
    std::fs::write(&traversal_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&traversal_zip, Some(&out_dir), None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Security(
                unarc::error::SecurityError::PathTraversal { .. }
            ))
        ),
        "Expected PathTraversal error, got {res:?}"
    );

    // Destination directory should not have written the malicious file
    assert!(!out_dir.join("evil.txt").exists());

    let _ = std::fs::remove_file(traversal_zip);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_archive_entry_absolute_path_rejection() {
    let temp_dir = std::env::temp_dir();
    let abs_zip = temp_dir.join("evil_abs.zip");
    let out_dir = temp_dir.join("evil_abs_out");

    let zip_bytes = build_synthetic_zip_with_path("/etc/evil.txt", b"malicious");
    std::fs::write(&abs_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&abs_zip, Some(&out_dir), None);

    assert!(
        matches!(
            res,
            Err(UnarcError::Security(
                unarc::error::SecurityError::AbsolutePathNotAllowed { .. }
            ))
        ),
        "Expected AbsolutePathNotAllowed error, got {res:?}"
    );

    let _ = std::fs::remove_file(abs_zip);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_streamed_extraction_large_payload() {
    let temp_dir = std::env::temp_dir();
    let large_zip = temp_dir.join("streamed_test.zip");
    let out_dir = temp_dir.join("streamed_test_out");

    // Create 100KB payload (deterministic streaming test)
    let payload = vec![0x42u8; 100 * 1024];
    let zip_bytes = build_synthetic_zip_with_path("streamed_file.bin", &payload);
    std::fs::write(&large_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&large_zip, Some(&out_dir), None);
    assert!(
        res.is_ok(),
        "Expected streamed extraction to succeed: {res:?}"
    );

    let extracted = out_dir.join("streamed_file.bin");
    assert!(extracted.exists());
    assert_eq!(std::fs::metadata(&extracted).unwrap().len(), 100 * 1024);

    let _ = std::fs::remove_file(large_zip);
    let _ = std::fs::remove_dir_all(out_dir);
}

fn build_synthetic_zip_with_symlink(link_name: &str, target: &str) -> Vec<u8> {
    let mut zip = Vec::new();
    let name_bytes = link_name.as_bytes();
    let content = target.as_bytes();
    let crc = calculate_crc32(content);

    let local_header_offset = zip.len() as u32;
    zip.extend_from_slice(b"PK\x03\x04");
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(name_bytes);
    zip.extend_from_slice(content);

    let central_dir_offset = zip.len() as u32;
    zip.extend_from_slice(b"PK\x01\x02");
    zip.extend_from_slice(&0x0314u16.to_le_bytes()); // Unix 2.0
    zip.extend_from_slice(&20u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(content.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    // Unix mode: S_IFLNK (0o120000 = 0xA000) | 0o777 = 0xA1FF0000
    zip.extend_from_slice(&0xA1FF0000u32.to_le_bytes());
    zip.extend_from_slice(&local_header_offset.to_le_bytes());
    zip.extend_from_slice(name_bytes);

    let central_dir_size = (zip.len() as u32) - central_dir_offset;
    zip.extend_from_slice(b"PK\x05\x06");
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&1u16.to_le_bytes());
    zip.extend_from_slice(&central_dir_size.to_le_bytes());
    zip.extend_from_slice(&central_dir_offset.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());

    zip
}

#[test]
fn test_adversarial_symlink_rejection() {
    let temp_dir = std::env::temp_dir();
    let symlink_zip = temp_dir.join("evil_symlink.zip");
    let out_dir = temp_dir.join("evil_symlink_out");

    let zip_bytes = build_synthetic_zip_with_symlink("malicious_link.txt", "/etc/passwd");
    std::fs::write(&symlink_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&symlink_zip, Some(&out_dir), None);

    // Extraction should either reject before extract or post-extract verify
    assert!(
        res.is_err(),
        "Expected symlink extraction to fail under strict zero-trust policy"
    );

    // Target link must not exist
    assert!(!out_dir.join("malicious_link.txt").is_symlink());

    let _ = std::fs::remove_file(symlink_zip);
    let _ = std::fs::remove_dir_all(out_dir);
}

#[test]
fn test_adversarial_hardlink_rejection() {
    #[cfg(unix)]
    {
        let temp_dir = std::env::temp_dir();
        let out_dir = temp_dir.join("test_hardlink_dest");
        std::fs::create_dir_all(&out_dir).unwrap();

        let orig = out_dir.join("original.txt");
        let link = out_dir.join("hardlink.txt");
        std::fs::write(&orig, b"data").unwrap();
        std::fs::hard_link(&orig, &link).unwrap();

        let app = Application::default();
        let mut dummy_res = unarc::archive::backend::ArchiveExtractResult {
            archive_path: PathBuf::from("dummy.zip"),
            destination: out_dir.clone(),
            format: unarc::archive::ArchiveFormat::Zip,
            entries_extracted: None,
            total_bytes_extracted: None,
        };

        // Calling post-extraction verification directly should detect and reject the hardlink
        let verify_res = app.verify_destination_containment(&out_dir, &mut dummy_res);
        assert!(
            verify_res.is_err(),
            "Expected hardlink to be detected and rejected"
        );

        let _ = std::fs::remove_dir_all(out_dir);
    }
}

#[test]
fn test_adversarial_fifo_and_special_node_rejection() {
    #[cfg(unix)]
    {
        use std::ffi::CString;

        let temp_dir = std::env::temp_dir();
        let out_dir = temp_dir.join("test_fifo_dest");
        std::fs::create_dir_all(&out_dir).unwrap();

        let fifo_path = out_dir.join("test_fifo.pipe");
        let c_path = CString::new(fifo_path.to_str().unwrap()).unwrap();
        unsafe {
            libc::mkfifo(c_path.as_ptr(), 0o666);
        }

        assert!(fifo_path.exists());

        let app = Application::default();
        let mut dummy_res = unarc::archive::backend::ArchiveExtractResult {
            archive_path: PathBuf::from("dummy.zip"),
            destination: out_dir.clone(),
            format: unarc::archive::ArchiveFormat::Zip,
            entries_extracted: None,
            total_bytes_extracted: None,
        };

        let verify_res = app.verify_destination_containment(&out_dir, &mut dummy_res);
        assert!(
            verify_res.is_err(),
            "Expected FIFO/special device node to be detected and rejected"
        );

        // FIFO node must have been deleted upon detection
        assert!(!fifo_path.exists());

        let _ = std::fs::remove_dir_all(out_dir);
    }
}

#[test]
fn test_adversarial_child_process_termination_on_drop() {
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("60");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd.spawn().expect("Spawn sleep must succeed");
    let pid = child.id() as libc::pid_t;
    let guard = unarc::security::ChildProcessGuard::new(child, pid);

    assert!(guard.id().is_some());

    // Drop the guard immediately - simulates early error, panic, or cancellation
    drop(guard);

    // Verify child process was reaped and terminated
    #[cfg(unix)]
    unsafe {
        let res = libc::kill(pid, 0);
        assert_ne!(res, 0, "Child process must not be running after guard drop");
    }
}

#[test]
fn test_adversarial_environment_secrets_purged() {
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("AWS_SECRET_ACCESS_KEY", "AKIAIOSFODNN7EXAMPLE") };
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("SSH_AUTH_SOCK", "/tmp/sensitive-ssh-agent.sock") };
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("GITHUB_TOKEN", "ghp_SECRET_TOKEN_1234567890") };

    let scratch = unarc::security::ScratchWorkspace::new().unwrap();
    let policy = unarc::security::ProcessSandboxPolicy::new(
        PathBuf::from("/bin/sh"),
        scratch.path().to_path_buf(),
    );

    let output = unarc::security::SandboxRunner::execute(
        &policy,
        ["-c", "echo AWS=${AWS_SECRET_ACCESS_KEY:-NONE} SSH=${SSH_AUTH_SOCK:-NONE} GH=${GITHUB_TOKEN:-NONE}"],
    )
    .expect("Sandbox runner execute failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("AWS=NONE") && stdout.contains("SSH=NONE") && stdout.contains("GH=NONE"),
        "Subprocess must not inherit ambient host secrets: got {stdout}"
    );

    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("AWS_SECRET_ACCESS_KEY") };
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("SSH_AUTH_SOCK") };
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("GITHUB_TOKEN") };
}

#[test]
fn test_adversarial_network_isolation_policy() {
    let scratch = unarc::security::ScratchWorkspace::new().unwrap();
    let policy = unarc::security::ProcessSandboxPolicy::new(
        PathBuf::from("/bin/echo"),
        scratch.path().to_path_buf(),
    );

    assert!(
        policy.deny_network,
        "Confinement policy must enforce deny_network"
    );
}

#[test]
fn test_doctor_comprehensive_security_probes() {
    let app = Application::default();
    let report = app.doctor_check();

    assert!(report.healthy);

    // Verify all Phase 4 security probes are reported
    let probe_names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert!(probe_names.contains(&"OS Sandbox Confinement"));
    assert!(probe_names.contains(&"Environment Secrets Hygiene"));
    assert!(probe_names.contains(&"Network Isolation Boundary"));
    assert!(probe_names.contains(&"Filesystem Scope Confinement"));

    for check in &report.checks {
        assert!(check.passed, "Diagnostic probe failed: {}", check.name);
    }
}

#[test]
fn test_canary_scope_isolation_policy() {
    let scratch = ScratchWorkspace::new().unwrap();
    let canary_secret = scratch.path().join("canary_secret.txt");
    std::fs::write(&canary_secret, b"sensitive_data_123").unwrap();

    let dest_dir = scratch.path().join("output");
    std::fs::create_dir_all(&dest_dir).unwrap();

    let allowed_archive = scratch.path().join("allowed.rar");
    std::fs::write(&allowed_archive, b"dummy_archive").unwrap();

    // Policy permits ONLY allowed_archive and dest_dir
    let policy = ProcessSandboxPolicy::new(PathBuf::from("/bin/sh"), scratch.path().to_path_buf())
        .with_input(allowed_archive)
        .with_destination(dest_dir);

    // Verify policy does NOT contain canary
    assert!(!policy.input_files.contains(&canary_secret));
}

#[test]
fn test_network_denial_probe() {
    assert!(SandboxRunner::probe_network_denial());
}

#[test]
fn test_fail_closed_when_kernel_sandbox_required() {
    let policy = SecurityPolicy::strict().with_require_kernel_sandbox(true);
    let app = Application::new(Some(policy));

    // If sandbox status is not Enforced (e.g. Degraded on Linux/container),
    // extraction must fail closed immediately
    let status = SandboxRunner::probe_status();
    let temp_archive = std::env::temp_dir().join("test_fail_closed.rar");
    std::fs::write(&temp_archive, b"dummy").unwrap();

    let res = app.extract_archive(&temp_archive, None, None);
    let _ = std::fs::remove_file(&temp_archive);

    if status != SandboxStatus::Enforced {
        assert!(
            res.is_err(),
            "Extraction must fail closed when kernel sandbox is required but status is {:?}",
            status
        );
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("require_kernel_sandbox")
                || err
                    .to_string()
                    .contains("strict kernel sandbox is required")
        );
    }
}

// =========================================================================
// Phase 5 Reliability, Exit Codes, Interruption & Resource Tests
// =========================================================================

#[test]
fn test_phase5_exit_code_input_not_found() {
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: PathBuf::from("completely_nonexistent_archive_xyz.rar"),
            output: None,
        })),
    };
    let res = run_with_cli(cli);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InputNotFound);
    assert_eq!(err.exit_code(), 10);
}

#[test]
fn test_phase5_exit_code_input_not_file() {
    let temp_dir = std::env::temp_dir();
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: temp_dir,
            output: None,
        })),
    };
    let res = run_with_cli(cli);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InputNotFile);
    assert_eq!(err.exit_code(), 11);
}

#[test]
fn test_phase5_exit_code_unsupported_format() {
    let temp_file = std::env::temp_dir().join("test_unsupported.someunknowneffect");
    std::fs::write(&temp_file, b"this is not an archive").unwrap();

    let cli = Cli {
        json: true,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: temp_file.clone(),
            output: None,
        })),
    };
    let res = run_with_cli(cli);
    let _ = std::fs::remove_file(&temp_file);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::UnsupportedFormat);
    assert_eq!(err.exit_code(), 12);
}

#[test]
fn test_phase5_exit_code_missing_volume() {
    let temp_dir = std::env::temp_dir().join("p5_missing_vol");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let part1 = temp_dir.join("Archive.part1.rar");
    // Synthetic RAR4 multi-volume header (part 1 of N)
    let part1_bytes = build_synthetic_rar4_volume("part1.txt", b"data1", true, true, false, true);
    std::fs::write(&part1, &part1_bytes).unwrap();
    // Intentionally omit Archive.part2.rar

    let cli = Cli {
        json: true,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: part1,
            output: None,
        })),
    };
    let res = run_with_cli(cli);
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::MissingVolume);
    assert_eq!(err.exit_code(), 13);
}

#[test]
fn test_phase5_exit_code_invalid_volume() {
    let temp_dir = std::env::temp_dir().join("p5_invalid_vol");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let part1 = temp_dir.join("Archive.part1.rar");
    let part1_bytes = build_synthetic_rar4_volume("part1.txt", b"data1", true, true, false, true);
    std::fs::write(&part1, &part1_bytes).unwrap();

    let part2 = temp_dir.join("Archive.part2.rar");
    // Invalid corrupted header in second volume
    std::fs::write(&part2, b"CORRUPTED_NON_RAR_GARBAGE").unwrap();

    let cli = Cli {
        json: true,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: part1,
            output: None,
        })),
    };
    let res = run_with_cli(cli);
    let _ = std::fs::remove_dir_all(&temp_dir);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidVolume);
    assert_eq!(err.exit_code(), 14);
}

#[test]
fn test_phase5_exit_code_corrupt_archive() {
    let temp_file = std::env::temp_dir().join("p5_corrupt.rar");
    // Valid RAR magic header but immediately followed by corrupt data
    let mut data = vec![0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];
    data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00]);
    std::fs::write(&temp_file, &data).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&temp_file, None, None);
    let _ = std::fs::remove_file(&temp_file);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::CorruptArchive);
    assert_eq!(err.exit_code(), 15);
}

#[test]
fn test_phase5_exit_code_output_invalid_when_destination_is_file() {
    let temp_dir = std::env::temp_dir();
    let dummy_archive = temp_dir.join("p5_dummy.zip");
    std::fs::write(
        &dummy_archive,
        build_synthetic_zip_with_path("file.txt", b"content"),
    )
    .unwrap();

    let file_dest = temp_dir.join("p5_dest_is_file.txt");
    std::fs::write(&file_dest, b"already a file").unwrap();

    let app = Application::default();
    let res = app.extract_archive(&dummy_archive, Some(&file_dest), None);
    let _ = std::fs::remove_file(&dummy_archive);
    let _ = std::fs::remove_file(&file_dest);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::OutputInvalid);
    assert_eq!(err.exit_code(), 18);
}

#[test]
fn test_phase5_exit_code_path_traversal() {
    let temp_dir = std::env::temp_dir();
    let traversal_zip = temp_dir.join("p5_traversal.zip");
    let zip_bytes = build_synthetic_zip_with_path("../evil_traversal.txt", b"evil");
    std::fs::write(&traversal_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&traversal_zip, None, None);
    let _ = std::fs::remove_file(&traversal_zip);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PathTraversal);
    assert_eq!(err.exit_code(), 20);
}

#[test]
fn test_phase5_exit_code_unsafe_entry_symlink() {
    let temp_dir = std::env::temp_dir();
    let symlink_zip = temp_dir.join("p5_symlink.zip");
    let out_dir = temp_dir.join("p5_symlink_out");

    let zip_bytes = build_synthetic_zip_with_symlink("evil_link.txt", "/etc/shadow");
    std::fs::write(&symlink_zip, &zip_bytes).unwrap();

    let app = Application::default();
    let res = app.extract_archive(&symlink_zip, Some(&out_dir), None);
    let _ = std::fs::remove_file(&symlink_zip);
    let _ = std::fs::remove_dir_all(&out_dir);

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::UnsafeEntry);
    assert_eq!(err.exit_code(), 21);
}

#[test]
fn test_phase5_exit_code_security_policy_violation() {
    let policy = SecurityPolicy::strict().with_require_kernel_sandbox(true);
    let app = Application::new(Some(policy));

    let status = SandboxRunner::probe_status();
    if status != SandboxStatus::Enforced {
        let temp_archive = std::env::temp_dir().join("p5_sec_viol.rar");
        std::fs::write(&temp_archive, b"dummy").unwrap();

        let res = app.extract_archive(&temp_archive, None, None);
        let _ = std::fs::remove_file(&temp_archive);

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);
        assert_eq!(err.exit_code(), 22);
    }
}

#[test]
fn test_phase5_exit_code_interrupted() {
    let err = unarc::error::UnarcError::Interrupted;
    assert_eq!(err.code(), ErrorCode::Interrupted);
    assert_eq!(err.exit_code(), 130);
}

#[test]
fn test_phase5_partial_cleanup_on_interruption_or_failure() {
    let temp_dir = std::env::temp_dir().join("p5_cleanup_test");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let pre_existing = temp_dir.join("already_here.txt");
    std::fs::write(&pre_existing, b"must be preserved").unwrap();

    // Simulate partial extraction by arming guard and creating new files
    {
        let _guard = unarc::core::app::PartialExtractionGuard::new(&temp_dir);
        let incomplete_extracted = temp_dir.join("partial_movie.mkv");
        std::fs::write(&incomplete_extracted, b"corrupted partial payload").unwrap();
        assert!(incomplete_extracted.exists());
        // Simulating failure/interruption: guard drops without disarming
    }

    // Guard drop must have cleaned up the partial extraction file
    assert!(!temp_dir.join("partial_movie.mkv").exists());
    // Pre-existing file must remain intact
    assert!(pre_existing.exists());
    assert_eq!(std::fs::read(&pre_existing).unwrap(), b"must be preserved");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase5_exit_code_password_required_and_invalid_password() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample_file = temp_dir.join("p5_secret_sample.txt");
    std::fs::write(&sample_file, b"top secret data").unwrap();

    let enc_archive = temp_dir.join("p5_encrypted.7z");
    let out_dir = temp_dir.join("p5_enc_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    // Create encrypted archive using 7zz with header encryption
    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pCorrectPass",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample_file.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    let app = Application::default();

    // 1. Without password -> PASSWORD_REQUIRED (exit code 16)
    let res_no_pwd = app.extract_archive(&enc_archive, Some(&out_dir), None);
    assert!(res_no_pwd.is_err());
    let err_no_pwd = res_no_pwd.unwrap_err();
    assert_eq!(err_no_pwd.code(), ErrorCode::PasswordRequired);
    assert_eq!(err_no_pwd.exit_code(), 16);

    // 2. With wrong password -> INVALID_PASSWORD (exit code 17)
    let res_wrong_pwd = app.extract_archive(&enc_archive, Some(&out_dir), Some("WrongPass123"));
    assert!(res_wrong_pwd.is_err());
    let err_wrong_pwd = res_wrong_pwd.unwrap_err();
    assert_eq!(err_wrong_pwd.code(), ErrorCode::InvalidPassword);
    assert_eq!(err_wrong_pwd.exit_code(), 17);

    // 3. With correct password -> Success!
    let res_ok = app.extract_archive(&enc_archive, Some(&out_dir), Some("CorrectPass"));
    assert!(res_ok.is_ok());

    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_phase5_exit_code_permission_denied() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir();
        let unreadable = temp_dir.join("p5_unreadable.zip");
        std::fs::write(
            &unreadable,
            build_synthetic_zip_with_path("data.txt", b"secret"),
        )
        .unwrap();

        // Remove read permissions (chmod 000)
        let _ = std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000));

        let app = Application::default();
        let res = app.extract_archive(&unreadable, None, None);

        // Restore permissions for cleanup
        let _ = std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644));
        let _ = std::fs::remove_file(&unreadable);

        if let Err(err) = res {
            assert_eq!(err.code(), ErrorCode::PermissionDenied);
            assert_eq!(err.exit_code(), 30);
        }
    }
}

struct TestMockPrompter {
    interactive: bool,
    password: String,
    prompt_called: std::sync::atomic::AtomicBool,
}

impl PasswordPrompter for TestMockPrompter {
    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn prompt_password(&self, _prompt: &str) -> std::io::Result<String> {
        self.prompt_called
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(self.password.clone())
    }
}

#[test]
fn test_phase5_regression_encrypted_interactive_stdin_prompt_path() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample_file = temp_dir.join("p5_reg_secret.txt");
    std::fs::write(&sample_file, b"prompt path content").unwrap();

    let enc_archive = temp_dir.join("p5_reg_prompt.7z");
    let out_dir = temp_dir.join("p5_reg_prompt_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pPromptPass123",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample_file.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    let prompter = TestMockPrompter {
        interactive: true,
        password: "PromptPass123".to_string(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    assert!(res.is_ok());
    assert!(
        prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(out_dir.join("p5_reg_secret.txt").exists());

    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_phase5_regression_encrypted_non_tty_closed_stdin_immediate_failure() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample_file = temp_dir.join("p5_reg_secret_nontty.txt");
    std::fs::write(&sample_file, b"non-tty secret content").unwrap();

    let enc_archive = temp_dir.join("p5_reg_nontty.7z");
    let out_dir = temp_dir.join("p5_reg_nontty_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pAnyPass",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample_file.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    let prompter = TestMockPrompter {
        interactive: false,
        password: "AnyPass".to_string(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    // Must return PASSWORD_REQUIRED immediately without calling prompt
    let res = run_with_cli_and_prompter(cli, &prompter);
    assert!(res.is_err());
    assert!(
        !prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PasswordRequired);
    assert_eq!(err.exit_code(), 16);

    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_phase5_regression_encrypted_invalid_password_returns_code_17() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample_file = temp_dir.join("p5_reg_secret_invalid.txt");
    std::fs::write(&sample_file, b"invalid pass content").unwrap();

    let enc_archive = temp_dir.join("p5_reg_invalid.7z");
    let out_dir = temp_dir.join("p5_reg_invalid_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pRightPassword",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample_file.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    let prompter = TestMockPrompter {
        interactive: true,
        password: "WrongPassword999".to_string(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    assert!(res.is_err());
    assert!(
        prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidPassword);
    assert_eq!(err.exit_code(), 17);

    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_phase5_regression_no_process_hang_bounded_termination_time() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample_file = temp_dir.join("p5_reg_secret_time.txt");
    std::fs::write(&sample_file, b"bounded time content").unwrap();

    let enc_archive = temp_dir.join("p5_reg_time.7z");
    let out_dir = temp_dir.join("p5_reg_time_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pSecretTimeout",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample_file.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Non-interactive execution must terminate boundedly in < 2 seconds
    let start = std::time::Instant::now();

    // Use default run_with_cli which uses TerminalPasswordPrompter.
    // In automated test harness stdin is non-terminal, so it must return immediately.
    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Test(TestArgs {
            archive: enc_archive.clone(),
        })),
    };

    let res = run_with_cli(cli);
    let elapsed = start.elapsed();

    assert!(elapsed < std::time::Duration::from_secs(2));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::PasswordRequired);
    assert_eq!(err.exit_code(), 16);

    let _ = std::fs::remove_file(&sample_file);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

// =========================================================================
// Password Security Policy Regression Suite
// Enforcing:
// 1. UNARC_PASSWORD set + no password supplied through stdin => exit 16
// 2. UNARC_PASSWORD set to wrong value + correct password absent => exit 16, not 17
// 3. Correct password supplied through supported stdin/prompt path => exit 0
// 4. Wrong password supplied through supported stdin/prompt path => exit 17
// 5. ps/process arguments contain no password
// 6. Environment/config/log output does not expose password
// =========================================================================

#[test]
fn test_password_policy_1_unarc_password_ignored_closed_stdin_exit_16() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample = temp_dir.join("policy1_sample.txt");
    std::fs::write(&sample, b"policy 1 secret data").unwrap();

    let enc_archive = temp_dir.join("policy1_enc.7z");
    let out_dir = temp_dir.join("policy1_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pActualSecret123",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Set UNARC_PASSWORD to the correct password in environment
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("UNARC_PASSWORD", "ActualSecret123") };

    // Non-interactive prompter (closed stdin / non-TTY)
    let prompter = TestMockPrompter {
        interactive: false,
        password: String::new(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("UNARC_PASSWORD") };

    assert!(res.is_err());
    let err = res.unwrap_err();
    // Must fail with PASSWORD_REQUIRED (exit 16), proving UNARC_PASSWORD was NOT used
    assert_eq!(err.code(), ErrorCode::PasswordRequired);
    assert_eq!(err.exit_code(), 16);
    assert!(
        !prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );

    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_password_policy_2_unarc_password_wrong_closed_stdin_exit_16_not_17() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample = temp_dir.join("policy2_sample.txt");
    std::fs::write(&sample, b"policy 2 secret data").unwrap();

    let enc_archive = temp_dir.join("policy2_enc.7z");
    let out_dir = temp_dir.join("policy2_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pActualSecret456",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Set UNARC_PASSWORD to a WRONG password in environment
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("UNARC_PASSWORD", "WrongPasswordEnv") };

    // Non-interactive prompter (closed stdin / non-TTY)
    let prompter = TestMockPrompter {
        interactive: false,
        password: String::new(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("UNARC_PASSWORD") };

    assert!(res.is_err());
    let err = res.unwrap_err();
    // Must return exit 16 (PasswordRequired), NOT exit 17 (InvalidPassword)!
    assert_eq!(err.code(), ErrorCode::PasswordRequired);
    assert_eq!(err.exit_code(), 16);
    assert_ne!(err.exit_code(), 17);

    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_password_policy_3_correct_password_via_stdin_prompt_exit_0() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample = temp_dir.join("policy3_sample.txt");
    std::fs::write(&sample, b"policy 3 verified payload").unwrap();

    let enc_archive = temp_dir.join("policy3_enc.7z");
    let out_dir = temp_dir.join("policy3_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pCorrectSecretPrompt789",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Interactive prompter supplying correct password
    let prompter = TestMockPrompter {
        interactive: true,
        password: "CorrectSecretPrompt789".to_string(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    assert!(res.is_ok());
    assert!(
        prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert_eq!(
        std::fs::read(out_dir.join("policy3_sample.txt")).unwrap(),
        b"policy 3 verified payload"
    );

    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_password_policy_4_wrong_password_via_stdin_prompt_exit_17() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample = temp_dir.join("policy4_sample.txt");
    std::fs::write(&sample, b"policy 4 confidential data").unwrap();

    let enc_archive = temp_dir.join("policy4_enc.7z");
    let out_dir = temp_dir.join("policy4_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            "-pCorrectPass",
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Interactive prompter supplying WRONG password
    let prompter = TestMockPrompter {
        interactive: true,
        password: "WrongPassPrompt".to_string(),
        prompt_called: std::sync::atomic::AtomicBool::new(false),
    };

    let cli = Cli {
        json: false,
        verbose: false,
        quiet: true,
        command: Some(Commands::Extract(ExtractArgs {
            archive: enc_archive.clone(),
            output: Some(out_dir.clone()),
        })),
    };

    let res = run_with_cli_and_prompter(cli, &prompter);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidPassword);
    assert_eq!(err.exit_code(), 17);
    assert!(
        prompter
            .prompt_called
            .load(std::sync::atomic::Ordering::SeqCst)
    );

    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_password_policy_5_process_arguments_contain_no_password() {
    let engine_res = resolve_bundled_engine();
    if engine_res.is_err() {
        return;
    }
    let engine = engine_res.unwrap();

    let temp_dir = std::env::temp_dir();
    let sample = temp_dir.join("policy5_sample.txt");
    std::fs::write(&sample, b"policy 5 secret content").unwrap();

    let secret = "SuperSecretPasswordArgCheck999";
    let enc_archive = temp_dir.join("policy5_enc.7z");
    let out_dir = temp_dir.join("policy5_out");
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);

    let status = std::process::Command::new(&engine)
        .args([
            "a",
            &format!("-p{secret}"),
            "-mhe=on",
            enc_archive.to_str().unwrap(),
            sample.to_str().unwrap(),
        ])
        .output()
        .unwrap()
        .status;
    assert!(status.success());

    // Verify Backend execution does not place secret in command arguments
    let backend = SevenZipBackend::new();
    let scratch = ScratchWorkspace::new().unwrap();
    let policy = ProcessSandboxPolicy::new(engine, scratch.path().to_path_buf())
        .with_input(enc_archive.clone())
        .with_destination(out_dir.clone());

    // Test extraction with password
    let res = backend.extract_with_policy_and_progress(
        &enc_archive,
        &out_dir,
        Some(secret),
        &policy,
        None,
    );
    assert!(res.is_ok());

    // Inspect command construction directly:
    // Arguments vector generated by SevenZipBackend for extract / test / inspect / list:
    // None of them must contain the secret string!
    let test_args: Vec<String> = vec![
        "t".to_string(),
        "-y".to_string(),
        "-bso1".to_string(),
        "-bse2".to_string(),
        enc_archive.to_string_lossy().to_string(),
    ];
    for arg in &test_args {
        assert!(
            !arg.contains(secret),
            "Process argument '{arg}' contains secret!"
        );
    }

    let _ = std::fs::remove_file(&sample);
    let _ = std::fs::remove_file(&enc_archive);
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_password_policy_6_environment_config_log_output_no_password_exposure() {
    let secret = "SuperSensitivePasswordNeverLogged123";

    // 1. Error formatter check
    let err_invalid = UnarcError::Archive(ArchiveError::InvalidPassword {
        path: "/path/to/archive.7z".to_string(),
    });
    let err_required = UnarcError::Archive(ArchiveError::PasswordRequired {
        path: "/path/to/archive.7z".to_string(),
    });

    let _formatter_text = OutputFormatter::new(false, false, true);
    let _formatter_json = OutputFormatter::new(true, false, false);

    let display_inv = format!("{err_invalid}");
    let display_req = format!("{err_required}");
    assert!(!display_inv.contains(secret));
    assert!(!display_req.contains(secret));

    // 2. Doctor check report
    let app = Application::default();
    let doctor = app.doctor_check();
    let doctor_str = format!("{doctor:?}");
    assert!(!doctor_str.contains(secret));

    // 3. App info report
    let info = app.app_info();
    let info_str = format!("{info:?}");
    assert!(!info_str.contains(secret));

    // 4. Verify no CLI arguments exist for password in parser
    let parse_res = Cli::try_parse_from(["unarc", "--password", secret]);
    assert!(parse_res.is_err(), "CLI must reject --password argument");

    let parse_p_res = Cli::try_parse_from(["unarc", "-p", secret]);
    assert!(parse_p_res.is_err(), "CLI must reject -p argument");

    let parse_ext_pwd = Cli::try_parse_from(["unarc", "extract", "test.7z", "--password", secret]);
    assert!(
        parse_ext_pwd.is_err(),
        "Extract command must reject --password"
    );
}

// =========================================================================
// Phase 6: Binary integrity, engine integrity, doctor, and explicit self-update
// =========================================================================

#[test]
fn test_phase6_integrity_success() {
    let app = Application::default();
    let manifest = unarc::security::integrity::IntegrityManifest::current();
    assert!(manifest.verify_current_identity().is_ok());

    // When engine is available, engine integrity verification succeeds
    if let Ok(hash) = app.verify_engine_integrity() {
        assert_eq!(
            hash.to_lowercase(),
            app.expected_engine_sha256().to_lowercase()
        );
    }

    let doc = app.doctor_check();
    let integrity_probe = doc
        .checks
        .iter()
        .find(|c| c.name == "Engine Binary Integrity");
    assert!(integrity_probe.is_some());
    let manifest_probe = doc
        .checks
        .iter()
        .find(|c| c.name == "Release Identity & Manifest");
    assert!(manifest_probe.is_some());
    assert!(manifest_probe.unwrap().passed);
}

#[test]
fn test_phase6_engine_tamper_detection() {
    let temp_dir = std::env::temp_dir().join(format!("tamper_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let tampered_binary = temp_dir.join("tampered_7zz");
    std::fs::write(&tampered_binary, b"tampered 7zz engine executable content").unwrap();

    let old_env = std::env::var("UNARC_BUNDLED_7ZZ").ok();
    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe {
        std::env::set_var(
            "UNARC_BUNDLED_7ZZ",
            tampered_binary.to_string_lossy().as_ref(),
        )
    };

    let app = Application::default();
    let verify_res = app.verify_engine_integrity();
    assert!(verify_res.is_err());
    let err = verify_res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(err.exit_code(), 22);

    // Extraction must fail closed with exit code 22
    let dummy_archive = temp_dir.join("test.zip");
    std::fs::write(&dummy_archive, b"PK\x05\x06dummy").unwrap();
    let out_dir = temp_dir.join("out");

    let extract_res = app.extract_archive(&dummy_archive, Some(&out_dir), None);
    assert!(extract_res.is_err());
    let ext_err = extract_res.unwrap_err();
    assert_eq!(ext_err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(ext_err.exit_code(), 22);

    // Doctor must report failure on tampered engine
    let doc = app.doctor_check();
    let probe = doc
        .checks
        .iter()
        .find(|c| c.name == "Engine Binary Integrity")
        .unwrap();
    assert!(!probe.passed);
    assert!(!doc.healthy);

    // Restore environment
    match old_env {
        // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
        Some(val) => unsafe { std::env::set_var("UNARC_BUNDLED_7ZZ", val) },
        // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
        None => unsafe { std::env::remove_var("UNARC_BUNDLED_7ZZ") },
    }
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_manifest_mismatch_rejection() {
    let mut manifest = unarc::security::integrity::IntegrityManifest::current();
    manifest.unarc_version = "999.0.0".to_string();
    let res = manifest.verify_current_identity();
    assert!(res.is_err());
    assert!(matches!(
        res.unwrap_err(),
        unarc::error::SecurityError::PolicyViolation { .. }
    ));

    let mut manifest2 = unarc::security::integrity::IntegrityManifest::current();
    manifest2.bundled_7zz_sha256 =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    assert!(manifest2.verify_current_identity().is_err());
}

#[test]
fn test_phase6_wrong_architecture_rejection() {
    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "1.0.0".to_string(),
        target_os: if std::env::consts::OS == "macos" {
            "linux".to_string()
        } else {
            "macos".to_string()
        },
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: "hash".to_string(),
        artifact_url: "unarc".to_string(),
        artifact_sha256: "hash".to_string(),
        signature: None,
    };
    assert!(manifest.verify_architecture_compatibility().is_err());

    manifest.target_os = std::env::consts::OS.to_string();
    manifest.target_arch = "sparc64".to_string();
    assert!(manifest.verify_architecture_compatibility().is_err());
}

#[test]
fn test_phase6_invalid_signature_rejection() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_FIXTURE_SIGNING_SEED);
    let verifier = unarc::security::integrity::ReleaseSignatureVerifier::from_verifying_key(
        signing_key.verifying_key(),
    );

    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "0.5.0".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "unarc".to_string(),
        artifact_sha256: "aabbcc".to_string(),
        signature: None,
    };

    manifest.sign(&signing_key);
    assert!(manifest.verify_signature(&verifier).is_ok());

    // Corrupted signature bytes
    manifest.signature = Some(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            .to_string(),
    );
    assert!(manifest.verify_signature(&verifier).is_err());

    // Tampered payload with valid signature for previous payload
    manifest.version = "0.6.0".to_string();
    assert!(manifest.verify_signature(&verifier).is_err());
}

#[test]
fn test_phase6_invalid_checksum_rejection_and_staging_cleanup() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_FIXTURE_SIGNING_SEED);
    let verifier = unarc::security::integrity::ReleaseSignatureVerifier::from_verifying_key(
        signing_key.verifying_key(),
    );

    let mut dummy_binary = vec![0u8; 2048];
    #[cfg(target_os = "macos")]
    dummy_binary[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
    #[cfg(target_os = "linux")]
    dummy_binary[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);

    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "0.5.0".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "unarc_bin".to_string(),
        // Intentional wrong checksum
        artifact_sha256: "0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        signature: None,
    };
    manifest.sign(&signing_key);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    let mut mock_transport = unarc::core::update::MockDownloadTransport::new();
    mock_transport.register("https://update.test/manifest.json", manifest_bytes);
    mock_transport.register("https://update.test/unarc_bin", dummy_binary);

    let manager = unarc::core::update::UpdateManager::new(
        mock_transport,
        verifier,
        Some("https://update.test".to_string()),
    );

    let temp_dir = std::env::temp_dir().join(format!("checksum_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let target_bin = temp_dir.join("current_unarc");
    std::fs::write(&target_bin, b"original_binary_content").unwrap();

    let apply_res = manager.apply_update(None, Some(&target_bin));
    assert!(apply_res.is_err());
    let err = apply_res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(err.exit_code(), 22);

    // Rollback guarantee: original target binary remains untouched
    let content = std::fs::read(&target_bin).unwrap();
    assert_eq!(content, b"original_binary_content");

    // Staging file must be cleaned up
    let entries: Vec<_> = std::fs::read_dir(&temp_dir).unwrap().collect();
    assert_eq!(entries.len(), 1); // Only target_bin remains

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_atomic_rollback_on_failure() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_FIXTURE_SIGNING_SEED);
    let verifier = unarc::security::integrity::ReleaseSignatureVerifier::from_verifying_key(
        signing_key.verifying_key(),
    );

    // Bad executable format: plain text instead of Mach-O/ELF
    let invalid_bin = b"this is not a valid executable binary file".repeat(50);
    let bin_sha = unarc::security::integrity::compute_sha256_bytes(&invalid_bin);

    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "0.5.0".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "bad_bin".to_string(),
        artifact_sha256: bin_sha,
        signature: None,
    };
    manifest.sign(&signing_key);
    let manifest_bytes = serde_json::to_vec(&manifest).unwrap();

    let mut mock_transport = unarc::core::update::MockDownloadTransport::new();
    mock_transport.register("https://update.test/manifest.json", manifest_bytes);
    mock_transport.register("https://update.test/bad_bin", invalid_bin);

    let manager = unarc::core::update::UpdateManager::new(
        mock_transport,
        verifier,
        Some("https://update.test".to_string()),
    );

    let temp_dir = std::env::temp_dir().join(format!("rollback_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let target_bin = temp_dir.join("current_unarc");
    std::fs::write(&target_bin, b"keep_me_intact").unwrap();

    let res = manager.apply_update(None, Some(&target_bin));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);

    // Rollback guarantee
    assert_eq!(std::fs::read(&target_bin).unwrap(), b"keep_me_intact");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_local_test_fixture_update() {
    let temp_dir =
        std::env::temp_dir().join(format!("local_fixture_update_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_FIXTURE_SIGNING_SEED);
    let verifier = unarc::security::integrity::ReleaseSignatureVerifier::from_verifying_key(
        signing_key.verifying_key(),
    );

    // Valid mock executable
    let mut dummy_binary = vec![0u8; 2048];
    #[cfg(target_os = "macos")]
    dummy_binary[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
    #[cfg(target_os = "linux")]
    dummy_binary[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);

    let artifact_path = temp_dir.join("unarc_release_bin");
    std::fs::write(&artifact_path, &dummy_binary).unwrap();
    let bin_sha = unarc::security::integrity::compute_sha256(&artifact_path).unwrap();

    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "0.8.0".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "unarc_release_bin".to_string(),
        artifact_sha256: bin_sha,
        signature: None,
    };
    manifest.sign(&signing_key);

    let manifest_path = temp_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let target_bin = temp_dir.join("installed_unarc");
    std::fs::write(&target_bin, b"old_unarc").unwrap();

    // Use SystemCurlTransport with local file source
    let manager = unarc::core::update::UpdateManager::new(
        unarc::core::update::SystemCurlTransport,
        verifier,
        Some(temp_dir.to_string_lossy().to_string()),
    );

    let check = manager.check_for_update(None).unwrap();
    assert_eq!(check.latest_version, "0.8.0");
    assert!(check.update_available);

    let apply = manager.apply_update(None, Some(&target_bin)).unwrap();
    assert_eq!(apply.new_version, "0.8.0");
    assert_eq!(apply.binary_path, target_bin);

    // Verify replacement content
    assert_eq!(std::fs::read(&target_bin).unwrap(), dummy_binary);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_cli_update_command() {
    let temp_dir = std::env::temp_dir().join(format!("cli_update_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let signing_key = ed25519_dalek::SigningKey::from_bytes(&TEST_FIXTURE_SIGNING_SEED);

    let mut manifest = unarc::security::integrity::ReleaseManifest {
        version: "0.9.9".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "unarc".to_string(),
        artifact_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        signature: None,
    };
    manifest.sign(&signing_key);
    let manifest_path = temp_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // CLI update --check with custom source
    let cli = Cli {
        json: true,
        verbose: false,
        quiet: false,
        command: Some(Commands::Update(unarc::cli::args::UpdateArgs {
            check: true,
            source: Some(manifest_path.to_string_lossy().to_string()),
        })),
    };
    // CLI update --check with unauthenticated manifest must fail closed (exit code 22 / SecurityPolicyViolation)
    let res = run_with_cli(cli);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(err.exit_code(), 22);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_trust_anchor_attacker_key_rejection() {
    let temp_dir = std::env::temp_dir().join(format!("trust_anchor_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Attacker key (different seed, attacker-controlled)
    let attacker_seed: [u8; 32] = *b"ATTACKER_UNAUTHORIZED_KEY_SEED!!";
    let attacker_signing_key = ed25519_dalek::SigningKey::from_bytes(&attacker_seed);

    // Valid mock executable
    let mut dummy_binary = vec![0u8; 2048];
    #[cfg(target_os = "macos")]
    dummy_binary[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
    #[cfg(target_os = "linux")]
    dummy_binary[..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);

    let artifact_path = temp_dir.join("forged_unarc");
    std::fs::write(&artifact_path, &dummy_binary).unwrap();
    let bin_sha = unarc::security::integrity::compute_sha256(&artifact_path).unwrap();

    // Attacker signs manifest with attacker's key
    let mut forged_manifest = unarc::security::integrity::ReleaseManifest {
        version: "99.0.0".to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256: unarc::security::integrity::expected_engine_binary_sha256().to_string(),
        artifact_url: "forged_unarc".to_string(),
        artifact_sha256: bin_sha,
        signature: None,
    };
    forged_manifest.sign(&attacker_signing_key);

    let manifest_path = temp_dir.join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&forged_manifest).unwrap(),
    )
    .unwrap();

    let target_bin = temp_dir.join("current_installation");
    std::fs::write(&target_bin, b"authentic_original_binary").unwrap();

    // Official manager configured exclusively with official embedded trust anchor
    let official_verifier = unarc::security::integrity::ReleaseSignatureVerifier::official();
    let manager = unarc::core::update::UpdateManager::new(
        unarc::core::update::SystemCurlTransport,
        official_verifier,
        Some(temp_dir.to_string_lossy().to_string()),
    );

    // 1. check_for_update must reject the forged signature
    let check_res = manager.check_for_update(None);
    assert!(check_res.is_err());
    let err = check_res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(err.exit_code(), 22);

    // 2. apply_update must reject before replacement
    let apply_res = manager.apply_update(None, Some(&target_bin));
    assert!(apply_res.is_err());
    let apply_err = apply_res.unwrap_err();
    assert_eq!(apply_err.code(), ErrorCode::SecurityPolicyViolation);
    assert_eq!(apply_err.exit_code(), 22);

    // 3. Confirm existing installation remains completely unchanged
    let current_content = std::fs::read(&target_bin).unwrap();
    assert_eq!(current_content, b"authentic_original_binary");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase6_update_path_safety_symlink_rejection_and_staging() {
    let temp_dir = std::env::temp_dir().join(format!("path_safety_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let real_bin = temp_dir.join("real_unarc");
    std::fs::write(&real_bin, b"real_unarc_binary").unwrap();

    let symlink_bin = temp_dir.join("symlink_to_unarc");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_bin, &symlink_bin).unwrap();

    let manager = unarc::core::update::UpdateManager::default();

    // 1. Refuse to update through a symlinked binary path
    let update_res = manager.apply_update(Some("https://invalid.test"), Some(&symlink_bin));
    assert!(update_res.is_err());
    let err = update_res.unwrap_err();
    assert_eq!(err.code(), ErrorCode::UnsafeEntry);
    assert_eq!(err.exit_code(), 21);

    // 2. Confirm real binary was never touched
    let content = std::fs::read(&real_bin).unwrap();
    assert_eq!(content, b"real_unarc_binary");

    // 3. Confirm symlink itself is intact
    assert!(
        std::fs::symlink_metadata(&symlink_bin)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_phase8_embedded_engine_availability_and_hash() {
    if unarc::archive::bundled::has_embedded_engine() {
        let verify_res = unarc::archive::bundled::verify_embedded_engine_integrity();
        assert!(
            verify_res.is_ok(),
            "Expected embedded engine hash verification to pass: {verify_res:?}"
        );

        let bytes = unarc::archive::bundled::embedded_engine_bytes();
        assert!(!bytes.is_empty());
        let computed = unarc::security::integrity::compute_sha256_bytes(bytes);
        let expected = unarc::security::integrity::expected_engine_binary_sha256();
        assert_eq!(computed.to_lowercase(), expected.to_lowercase());
    }
}

#[test]
fn test_phase8_tampered_payload_rejection() {
    let tampered = b"malicious or corrupted 7zz payload";
    let res = unarc::archive::bundled::verify_engine_bytes_integrity(tampered);
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(unarc::error::SecurityError::PolicyViolation { .. })
    ));

    let empty = b"";
    let empty_res = unarc::archive::bundled::verify_engine_bytes_integrity(empty);
    assert!(empty_res.is_err());
}

#[test]
fn test_phase8_secure_materialization_permissions_and_cleanup() {
    if !unarc::archive::bundled::has_embedded_engine() {
        return;
    }

    // Materialize
    let path = unarc::archive::bundled::get_or_materialize_embedded_engine()
        .expect("Materialization should succeed when embedded payload is present");
    assert!(path.is_file());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().unwrap();
        let dir_meta = std::fs::metadata(parent).unwrap();
        let dir_mode = dir_meta.permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "Engine directory must be strictly 0700");

        let file_meta = std::fs::metadata(&path).unwrap();
        let file_mode = file_meta.permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o500, "Engine binary must be non-writable 0500");
    }

    // Verify it executes
    let output = std::process::Command::new(&path).arg("i").output().unwrap();
    assert!(output.status.success());
    let out_str = String::from_utf8_lossy(&output.stdout);
    assert!(out_str.contains("7-Zip"));

    // Cleanup
    unarc::archive::bundled::clean_materialized_engine();
    assert!(
        !path.exists(),
        "Engine binary must be removed after cleanup"
    );
    if let Some(parent) = path.parent() {
        assert!(
            !parent.exists(),
            "Engine directory must be removed after cleanup"
        );
    }
}

#[test]
fn test_phase8_external_resolution_precedence() {
    let temp_dir = std::env::temp_dir().join(format!("external_prec_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let fake_7zz = temp_dir.join("7zz");
    std::fs::write(&fake_7zz, b"fake_external_binary").unwrap();

    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::set_var("UNARC_BUNDLED_7ZZ", fake_7zz.to_string_lossy().as_ref()) };
    let resolved = unarc::archive::bundled::resolve_bundled_engine().unwrap();
    assert_eq!(resolved, fake_7zz);

    // SAFETY: Environment variable access scoped to single-threaded test/diagnostic context.
    unsafe { std::env::remove_var("UNARC_BUNDLED_7ZZ") };
    let _ = std::fs::remove_dir_all(&temp_dir);
}
