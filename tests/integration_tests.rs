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
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "AKIAIOSFODNN7EXAMPLE");
    std::env::set_var("SSH_AUTH_SOCK", "/tmp/sensitive-ssh-agent.sock");
    std::env::set_var("GITHUB_TOKEN", "ghp_SECRET_TOKEN_1234567890");

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

    std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    std::env::remove_var("SSH_AUTH_SOCK");
    std::env::remove_var("GITHUB_TOKEN");
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
