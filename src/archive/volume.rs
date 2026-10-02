//! Deterministic archive input and multipart volume resolution.
//!
//! Enforces strict volume sequencing, gap detection, and scope boundary checks.
//! Never scans directories recursively.

use super::format::ArchiveFormat;
use crate::error::ArchiveError;
use crate::security::SecurityPolicy;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Information about a resolved archive volume set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSet {
    /// The primary volume to open / extract from (always volume 1).
    pub primary_volume: PathBuf,

    /// All resolved volumes in chronological sequence order (part 1, part 2, ...).
    pub volumes: Vec<PathBuf>,

    /// Detected archive format.
    pub format: ArchiveFormat,

    /// Whether this is a multivolume set.
    pub is_multipart: bool,
}

/// Pattern category for volume naming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeNamingScheme {
    /// Modern RAR naming: `<Stem>.part<N>.rar`
    ModernPart {
        stem: String,
        volume_index: usize,
        digit_width: usize,
    },
    /// Legacy RAR naming: `<Stem>.rar` (vol 1), `<Stem>.r00` (vol 2), `<Stem>.r01` (vol 3), etc.
    LegacyPart { stem: String, volume_index: usize },
    /// Split 7-Zip naming: `<Stem>.7z.<NNN>`
    SplitSevenZip {
        stem: String,
        volume_index: usize,
        digit_width: usize,
    },
    /// Standalone single archive file.
    SingleFile,
}

/// Parses modern RAR multipart filenames: `<Stem>.part<Number>.rar` (case-insensitive).
#[must_use]
pub fn parse_modern_part_filename(filename: &str) -> Option<(String, usize, usize)> {
    let lower = filename.to_ascii_lowercase();
    if !lower.ends_with(".rar") {
        return None;
    }

    let without_ext = &filename[..filename.len() - 4];
    let lower_without_ext = &lower[..lower.len() - 4];

    // Find the last occurrence of ".part"
    let part_idx = lower_without_ext.rfind(".part")?;
    let stem = &without_ext[..part_idx];
    let after_part = &without_ext[part_idx + 5..];

    if after_part.is_empty() || !after_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let volume_index = after_part.parse::<usize>().ok()?;
    if volume_index == 0 {
        return None;
    }

    let digit_width = after_part.len();
    Some((stem.to_string(), volume_index, digit_width))
}

/// Parses legacy RAR multipart filenames: `<Stem>.rar` (vol 1), `<Stem>.r<digits>` (vol 2+), `<Stem>.s<digits>`
#[must_use]
pub fn parse_legacy_part_filename(filename: &str) -> Option<(String, usize)> {
    let lower = filename.to_ascii_lowercase();

    // Check .rXX extension (vol 2..101)
    if let Some(dot_idx) = lower.rfind('.') {
        let ext = &lower[dot_idx + 1..];
        let stem = &filename[..dot_idx];

        if ext == "rar" {
            // Could be legacy volume 1
            return Some((stem.to_string(), 1));
        }

        if ext.len() == 3 && ext.starts_with('r') {
            let digits = &ext[1..];
            if digits.chars().all(|c| c.is_ascii_digit()) {
                let num = digits.parse::<usize>().ok()?;
                return Some((stem.to_string(), num + 2));
            }
        }

        if ext.len() == 3 && ext.starts_with('s') {
            let digits = &ext[1..];
            if digits.chars().all(|c| c.is_ascii_digit()) {
                let num = digits.parse::<usize>().ok()?;
                return Some((stem.to_string(), num + 102));
            }
        }
    }

    None
}

/// Parses split 7z filenames: `<Stem>.7z.<digits>`
#[must_use]
pub fn parse_split_7z_filename(filename: &str) -> Option<(String, usize, usize)> {
    let lower = filename.to_ascii_lowercase();
    let dot_idx = lower.rfind('.')?;
    let digits = &lower[dot_idx + 1..];
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let remainder = &filename[..dot_idx];
    let lower_rem = &lower[..dot_idx];
    if !lower_rem.ends_with(".7z") {
        return None;
    }

    let stem = &remainder[..remainder.len() - 3];
    let index = digits.parse::<usize>().ok()?;
    if index == 0 {
        return None;
    }

    Some((stem.to_string(), index, digits.len()))
}

/// Detects the naming scheme of an archive path.
#[must_use]
pub fn detect_naming_scheme(path: &Path) -> VolumeNamingScheme {
    let filename = match path.file_name() {
        Some(name) => name.to_string_lossy(),
        None => return VolumeNamingScheme::SingleFile,
    };

    if let Some((stem, volume_index, digit_width)) = parse_modern_part_filename(&filename) {
        return VolumeNamingScheme::ModernPart {
            stem,
            volume_index,
            digit_width,
        };
    }

    if let Some((stem, volume_index, digit_width)) = parse_split_7z_filename(&filename) {
        return VolumeNamingScheme::SplitSevenZip {
            stem,
            volume_index,
            digit_width,
        };
    }

    if let Some((stem, volume_index)) = parse_legacy_part_filename(&filename) {
        // If it's .rar, it might be standalone unless proven otherwise
        return VolumeNamingScheme::LegacyPart { stem, volume_index };
    }

    VolumeNamingScheme::SingleFile
}

/// Header-level volume indicators parsed from RAR archive headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RarHeaderVolumeInfo {
    /// Whether the archive main header indicates it is part of a multivolume set.
    pub is_multivolume: bool,

    /// Whether this archive is flagged as the first volume in the set.
    pub is_first_volume: bool,

    /// Whether the archive indicates a subsequent volume follows.
    pub has_next_volume: bool,

    /// Volume number if present (1-based index).
    pub volume_number: Option<usize>,

    /// Detected format (RAR or RAR5).
    pub format: ArchiveFormat,
}

/// Inspects the initial header bytes of a RAR archive for multivolume flags without loading the file into memory.
pub fn inspect_rar_volume_flags(path: &Path) -> Result<RarHeaderVolumeInfo, ArchiveError> {
    let mut file = File::open(path).map_err(|_| ArchiveError::FileNotFound {
        path: path.to_string_lossy().to_string(),
    })?;

    let mut buf = [0u8; 512];
    let bytes_read = file
        .read(&mut buf)
        .map_err(|e| ArchiveError::CorruptArchive {
            message: format!("Failed to read archive header: {e}"),
        })?;

    if bytes_read < 7 {
        return Err(ArchiveError::CorruptArchive {
            message: "File too small to contain valid archive header".to_string(),
        });
    }

    // Check RAR5 magic
    if bytes_read >= 8 && buf.starts_with(b"Rar!\x1A\x07\x01\x00") {
        return parse_rar5_volume_flags(&buf[8..bytes_read], &mut file);
    }

    // Check RAR4 magic
    if buf.starts_with(b"Rar!\x1A\x07\x00") {
        return parse_rar4_volume_flags(&buf[7..bytes_read], &mut file);
    }

    Err(ArchiveError::UnsupportedFormat {
        format: path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

/// Parses RAR 4.x volume flags from the header buffer.
fn parse_rar4_volume_flags(
    data: &[u8],
    file: &mut File,
) -> Result<RarHeaderVolumeInfo, ArchiveError> {
    if data.len() < 7 {
        return Err(ArchiveError::CorruptArchive {
            message: "Truncated RAR4 main header".to_string(),
        });
    }

    // Offset 0..2: HEAD_CRC
    // Offset 2: HEAD_TYPE (0x73 = MAIN_HEAD)
    let head_type = data[2];
    if head_type != 0x73 {
        return Err(ArchiveError::CorruptArchive {
            message: format!("Expected RAR4 MAIN_HEAD (0x73), found 0x{head_type:02X}"),
        });
    }

    // Offset 3..5: FLAGS (u16 little-endian)
    let flags = u16::from_le_bytes([data[3], data[4]]);
    let is_multivolume = (flags & 0x0001) != 0;
    let is_first_volume = (flags & 0x0100) != 0 || !is_multivolume;

    // Check if subsequent volume exists by inspecting the end of archive header
    let has_next_volume = check_rar4_end_arc_next_volume(file).unwrap_or(false);

    Ok(RarHeaderVolumeInfo {
        is_multivolume,
        is_first_volume,
        has_next_volume,
        volume_number: None,
        format: ArchiveFormat::Rar,
    })
}

/// Checks the last block in a RAR4 file for `ENDARC_HEAD` with `EARC_NEXT_VOLUME`.
fn check_rar4_end_arc_next_volume(file: &mut File) -> Option<bool> {
    let file_len = file.metadata().ok()?.len();
    if file_len < 7 {
        return None;
    }

    // ENDARC_HEAD is typically 7 bytes at the very end of the archive
    let check_offset = file_len.saturating_sub(64);
    file.seek(SeekFrom::Start(check_offset)).ok()?;

    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;

    // Search for 0x7B (ENDARC_HEAD)
    for i in 0..tail.len().saturating_sub(4) {
        if tail[i] == 0x7B {
            let flags = u16::from_le_bytes([tail[i + 1], tail[i + 2]]);
            let next_vol = (flags & 0x0001) != 0;
            return Some(next_vol);
        }
    }

    None
}

/// Helper to decode a RAR5 variable-length integer (vint).
fn read_rar5_vint(slice: &[u8], offset: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0;
    while *offset < slice.len() {
        let b = slice[*offset];
        *offset += 1;
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Parses RAR 5.x volume flags.
fn parse_rar5_volume_flags(
    data: &[u8],
    _file: &mut File,
) -> Result<RarHeaderVolumeInfo, ArchiveError> {
    let mut offset = 4; // Skip 4-byte CRC32
    let _header_size =
        read_rar5_vint(data, &mut offset).ok_or_else(|| ArchiveError::CorruptArchive {
            message: "Truncated RAR5 header size".to_string(),
        })?;

    let header_type =
        read_rar5_vint(data, &mut offset).ok_or_else(|| ArchiveError::CorruptArchive {
            message: "Truncated RAR5 header type".to_string(),
        })?;

    if header_type != 1 {
        // Type 1 is Main archive header
        return Ok(RarHeaderVolumeInfo {
            is_multivolume: false,
            is_first_volume: true,
            has_next_volume: false,
            volume_number: None,
            format: ArchiveFormat::Rar5,
        });
    }

    let header_flags = read_rar5_vint(data, &mut offset).unwrap_or(0);
    if (header_flags & 0x01) != 0 {
        // Extra area present, skip extra area size
        let _extra_size = read_rar5_vint(data, &mut offset);
    }

    let archive_flags = read_rar5_vint(data, &mut offset).unwrap_or(0);
    let is_multivolume = (archive_flags & 0x0001) != 0;

    let volume_number = if (archive_flags & 0x0002) != 0 {
        read_rar5_vint(data, &mut offset).map(|v| (v as usize) + 1)
    } else {
        None
    };

    let is_first_volume = match volume_number {
        Some(1) => true,
        Some(_) => false,
        None => true,
    };

    Ok(RarHeaderVolumeInfo {
        is_multivolume,
        is_first_volume,
        has_next_volume: false,
        volume_number,
        format: ArchiveFormat::Rar5,
    })
}

/// Deterministic resolver for archive volume sets.
pub struct VolumeResolver;

impl VolumeResolver {
    /// Resolves the complete volume sequence for a selected archive path.
    ///
    /// Rules:
    /// - Starts strictly from the user-selected path.
    /// - Never scans directories recursively.
    /// - Restricts sibling searches strictly to the immediate parent directory.
    /// - Validates that every volume in sequence exists, is a regular file, and has valid headers.
    /// - Fails immediately with `MISSING_VOLUME` or `INVALID_VOLUME` on any gap, mismatch, or permission failure.
    pub fn resolve(
        selected_path: &Path,
        policy: &SecurityPolicy,
    ) -> Result<VolumeSet, ArchiveError> {
        // 1. Validate selected input path exists and inspect metadata in a single probe
        let metadata = match std::fs::symlink_metadata(selected_path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ArchiveError::FileNotFound {
                    path: selected_path.to_string_lossy().to_string(),
                });
            }
            Err(e) => {
                return Err(ArchiveError::InvalidVolume {
                    path: selected_path.display().to_string(),
                    reason: format!("Failed to read metadata: {e}"),
                });
            }
        };

        if metadata.file_type().is_symlink() && !policy.allow_symlinks {
            return Err(ArchiveError::InvalidVolume {
                path: selected_path.display().to_string(),
                reason: "Archive input cannot be a symlink under strict security policy"
                    .to_string(),
            });
        }

        if !metadata.file_type().is_file() {
            return Err(ArchiveError::InputNotFile {
                path: selected_path.display().to_string(),
                reason: "Archive path must be a regular file".to_string(),
            });
        }

        // 3. Determine naming scheme
        let scheme = detect_naming_scheme(selected_path);
        let parent_dir = selected_path.parent().unwrap_or_else(|| Path::new("."));

        match scheme {
            VolumeNamingScheme::ModernPart {
                stem,
                volume_index: _,
                digit_width,
            } => Self::resolve_modern_sequence(parent_dir, &stem, digit_width, policy),

            VolumeNamingScheme::SplitSevenZip {
                stem,
                volume_index: _,
                digit_width,
            } => Self::resolve_split_7z_sequence(parent_dir, &stem, digit_width, policy),

            VolumeNamingScheme::LegacyPart { stem, volume_index } => Self::resolve_legacy_sequence(
                parent_dir,
                &stem,
                volume_index,
                selected_path,
                policy,
            ),

            VolumeNamingScheme::SingleFile => {
                // Check if the format is RAR and whether the header indicates it's a volume
                if let Ok(flags) = inspect_rar_volume_flags(selected_path) {
                    if flags.is_multivolume {
                        // Header says it's multivolume! Look for legacy volume 2 (.r00)
                        let stem = selected_path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        return Self::resolve_legacy_sequence(
                            parent_dir,
                            &stem,
                            1,
                            selected_path,
                            policy,
                        );
                    }
                }

                let format = ArchiveFormat::from_path(selected_path)?;
                Ok(VolumeSet {
                    primary_volume: selected_path.to_path_buf(),
                    volumes: vec![selected_path.to_path_buf()],
                    format,
                    is_multipart: false,
                })
            }
        }
    }

    /// Resolves modern RAR sequence: `<stem>.part<N>.rar`
    fn resolve_modern_sequence(
        parent_dir: &Path,
        stem: &str,
        digit_width: usize,
        policy: &SecurityPolicy,
    ) -> Result<VolumeSet, ArchiveError> {
        // Collect matching siblings in the immediate parent directory only
        let siblings = Self::read_immediate_dir_entries(parent_dir)?;
        let mut parts_map = std::collections::BTreeMap::new();

        for entry_path in siblings {
            if let Some(file_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if let Some((part_stem, part_idx, _width)) = parse_modern_part_filename(file_name) {
                    if part_stem == stem {
                        // Found a matching sibling volume!
                        if parts_map.contains_key(&part_idx) {
                            return Err(ArchiveError::InvalidVolume {
                                path: entry_path.display().to_string(),
                                reason: format!(
                                    "Conflicting duplicate volume file for volume index {part_idx}"
                                ),
                            });
                        }
                        parts_map.insert(part_idx, entry_path);
                    }
                }
            }
        }

        if parts_map.is_empty() {
            let expected_first = format!("{stem}.part1.rar");
            return Err(ArchiveError::MissingVolume {
                expected: expected_first,
                details: "No volumes found for selected archive".to_string(),
            });
        }

        let max_idx = *parts_map.keys().next_back().unwrap();

        // Verify volume 1 exists
        if !parts_map.contains_key(&1) {
            let expected_first = Self::format_modern_part_name(stem, 1, digit_width);
            return Err(ArchiveError::MissingVolume {
                expected: expected_first,
                details: format!(
                    "Volume 1 is missing for multipart set with parts up to {max_idx}"
                ),
            });
        }

        // Verify contiguous sequence from 1 to max_idx
        let mut ordered_volumes = Vec::with_capacity(max_idx);
        for i in 1..=max_idx {
            match parts_map.get(&i) {
                Some(path) => {
                    Self::validate_volume_file(path, policy)?;
                    ordered_volumes.push(path.clone());
                }
                None => {
                    let expected_name = Self::format_modern_part_name(stem, i, digit_width);
                    return Err(ArchiveError::MissingVolume {
                        expected: expected_name,
                        details: format!("Missing volume {i} in contiguous sequence 1..{max_idx}"),
                    });
                }
            }
        }

        // Inspect final volume header to see if another volume is flagged as following
        if let Some(last_path) = ordered_volumes.last() {
            if let Ok(info) = inspect_rar_volume_flags(last_path) {
                if info.has_next_volume {
                    let next_idx = max_idx + 1;
                    let expected_next = Self::format_modern_part_name(stem, next_idx, digit_width);
                    return Err(ArchiveError::MissingVolume {
                        expected: expected_next,
                        details: format!(
                            "Header of volume {max_idx} indicates volume {next_idx} is required"
                        ),
                    });
                }
            }
        }

        let primary_volume = ordered_volumes[0].clone();
        let format = ArchiveFormat::from_path(&primary_volume).unwrap_or(ArchiveFormat::Rar);

        Ok(VolumeSet {
            primary_volume,
            volumes: ordered_volumes,
            format,
            is_multipart: true,
        })
    }

    /// Resolves legacy RAR sequence: `<stem>.rar`, `<stem>.r00`, `<stem>.r01`...
    fn resolve_legacy_sequence(
        parent_dir: &Path,
        stem: &str,
        _selected_idx: usize,
        selected_path: &Path,
        policy: &SecurityPolicy,
    ) -> Result<VolumeSet, ArchiveError> {
        let siblings = Self::read_immediate_dir_entries(parent_dir)?;
        let mut parts_map = std::collections::BTreeMap::new();

        for entry_path in siblings {
            if let Some(file_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if let Some((part_stem, part_idx)) = parse_legacy_part_filename(file_name) {
                    if part_stem == stem {
                        parts_map.insert(part_idx, entry_path);
                    }
                }
            }
        }

        // Check if primary volume `<stem>.rar` exists
        let primary_name = format!("{stem}.rar");
        let primary_path = parent_dir.join(&primary_name);

        if !primary_path.exists() {
            return Err(ArchiveError::MissingVolume {
                expected: primary_name,
                details: "Legacy multipart archive requires volume 1 (.rar)".to_string(),
            });
        }

        parts_map.insert(1, primary_path.clone());

        let max_idx = *parts_map.keys().next_back().unwrap();

        // If max_idx is 1, check if volume 1's header claims it is multivolume
        if max_idx == 1 {
            let flags = inspect_rar_volume_flags(&primary_path);
            if let Ok(info) = flags {
                if info.is_multivolume {
                    return Err(ArchiveError::MissingVolume {
                        expected: format!("{stem}.r00"),
                        details: "Archive header marks this as a multivolume set, but volume 2 (.r00) was not found".to_string(),
                    });
                }
            }

            // Standalone legacy RAR file
            Self::validate_volume_file(&primary_path, policy)?;
            let format = ArchiveFormat::from_path(selected_path).unwrap_or(ArchiveFormat::Rar);
            return Ok(VolumeSet {
                primary_volume: primary_path.clone(),
                volumes: vec![primary_path],
                format,
                is_multipart: false,
            });
        }

        // Contiguous verification for legacy sequence: 1..=max_idx
        let mut ordered = Vec::with_capacity(max_idx);
        for i in 1..=max_idx {
            match parts_map.get(&i) {
                Some(path) => {
                    Self::validate_volume_file(path, policy)?;
                    ordered.push(path.clone());
                }
                None => {
                    let expected_name = Self::format_legacy_part_name(stem, i);
                    return Err(ArchiveError::MissingVolume {
                        expected: expected_name,
                        details: format!(
                            "Missing volume {i} in contiguous legacy sequence 1..{max_idx}"
                        ),
                    });
                }
            }
        }

        let format = ArchiveFormat::from_path(&primary_path).unwrap_or(ArchiveFormat::Rar);

        Ok(VolumeSet {
            primary_volume: primary_path,
            volumes: ordered,
            format,
            is_multipart: true,
        })
    }

    /// Resolves split 7z sequence: `<stem>.7z.001`, `<stem>.7z.002`...
    fn resolve_split_7z_sequence(
        parent_dir: &Path,
        stem: &str,
        digit_width: usize,
        policy: &SecurityPolicy,
    ) -> Result<VolumeSet, ArchiveError> {
        let siblings = Self::read_immediate_dir_entries(parent_dir)?;
        let mut parts_map = std::collections::BTreeMap::new();

        for entry_path in siblings {
            if let Some(file_name) = entry_path.file_name().and_then(|n| n.to_str()) {
                if let Some((part_stem, part_idx, _width)) = parse_split_7z_filename(file_name) {
                    if part_stem == stem {
                        parts_map.insert(part_idx, entry_path);
                    }
                }
            }
        }

        let max_idx = parts_map.keys().next_back().copied().unwrap_or(1);

        if !parts_map.contains_key(&1) {
            let expected_first = format!("{stem}.7z.{:0width$}", 1, width = digit_width);
            return Err(ArchiveError::MissingVolume {
                expected: expected_first,
                details: format!("Split 7-Zip volume 1 is missing for set up to {max_idx}"),
            });
        }

        let mut ordered = Vec::with_capacity(max_idx);
        for i in 1..=max_idx {
            match parts_map.get(&i) {
                Some(p) => {
                    Self::validate_volume_file(p, policy)?;
                    ordered.push(p.clone());
                }
                None => {
                    let expected = format!("{stem}.7z.{:0width$}", i, width = digit_width);
                    return Err(ArchiveError::MissingVolume {
                        expected,
                        details: format!("Missing split 7z volume {i} in sequence 1..{max_idx}"),
                    });
                }
            }
        }

        let primary_volume = ordered[0].clone();

        Ok(VolumeSet {
            primary_volume,
            volumes: ordered,
            format: ArchiveFormat::SevenZip,
            is_multipart: true,
        })
    }

    /// Validates an individual volume file for existence, regular file status, and valid archive signature.
    fn validate_volume_file(path: &Path, policy: &SecurityPolicy) -> Result<(), ArchiveError> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| ArchiveError::InvalidVolume {
            path: path.display().to_string(),
            reason: format!("Cannot access volume metadata: {e}"),
        })?;

        if meta.file_type().is_symlink() && !policy.allow_symlinks {
            return Err(ArchiveError::InvalidVolume {
                path: path.display().to_string(),
                reason: "Volume is a symbolic link prohibited by active security policy"
                    .to_string(),
            });
        }

        if !meta.file_type().is_file() {
            return Err(ArchiveError::InvalidVolume {
                path: path.display().to_string(),
                reason: "Volume path must be a regular file".to_string(),
            });
        }

        // Verify valid magic header bytes (first 8 bytes)
        let mut f = File::open(path).map_err(|e| ArchiveError::InvalidVolume {
            path: path.display().to_string(),
            reason: format!("Failed to open volume: {e}"),
        })?;

        let mut header = [0u8; 8];
        let n = f
            .read(&mut header)
            .map_err(|e| ArchiveError::InvalidVolume {
                path: path.display().to_string(),
                reason: format!("Failed to read volume signature: {e}"),
            })?;

        if n < 4 {
            return Err(ArchiveError::InvalidVolume {
                path: path.display().to_string(),
                reason: "Volume file is too small to contain an archive header".to_string(),
            });
        }

        // Validate that magic bytes correspond to a known archive signature
        let is_valid_signature = header.starts_with(b"Rar!\x1A\x07")
            || header.starts_with(b"7z\xBC\xAF\x27\x1C")
            || header.starts_with(b"PK\x03\x04")
            || header.starts_with(b"PK\x05\x06")
            || header.starts_with(b"PK\x07\x08");

        if !is_valid_signature {
            return Err(ArchiveError::InvalidVolume {
                path: path.display().to_string(),
                reason: "Volume file does not have a valid archive signature".to_string(),
            });
        }

        Ok(())
    }

    /// Strictly non-recursive read of direct siblings in the parent directory.
    fn read_immediate_dir_entries(dir: &Path) -> Result<Vec<PathBuf>, ArchiveError> {
        let entries = std::fs::read_dir(dir).map_err(|e| ArchiveError::BackendFailure {
            backend: "filesystem".to_string(),
            message: format!("Failed to read directory {}: {e}", dir.display()),
        })?;

        let mut paths = Vec::new();
        for entry in entries.flatten() {
            paths.push(entry.path());
        }

        Ok(paths)
    }

    /// Formats modern part name with width padding.
    fn format_modern_part_name(stem: &str, index: usize, width: usize) -> String {
        if width > 1 {
            format!("{stem}.part{:0width$}.rar", index, width = width)
        } else {
            format!("{stem}.part{index}.rar")
        }
    }

    /// Formats legacy part name.
    fn format_legacy_part_name(stem: &str, index: usize) -> String {
        if index == 1 {
            format!("{stem}.rar")
        } else if index <= 101 {
            let offset = index - 2;
            format!("{stem}.r{:02}", offset)
        } else {
            let offset = index - 102;
            format!("{stem}.s{:02}", offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_modern_part_filename() {
        assert_eq!(
            parse_modern_part_filename("archive.part1.rar"),
            Some(("archive".to_string(), 1, 1))
        );
        assert_eq!(
            parse_modern_part_filename("My.Movie.part02.rar"),
            Some(("My.Movie".to_string(), 2, 2))
        );
        assert_eq!(
            parse_modern_part_filename("file.part003.RAR"),
            Some(("file".to_string(), 3, 3))
        );
        assert_eq!(parse_modern_part_filename("file.part0.rar"), None);
        assert_eq!(parse_modern_part_filename("file.partX.rar"), None);
        assert_eq!(parse_modern_part_filename("file.rar"), None);
    }

    #[test]
    fn test_parse_legacy_part_filename() {
        assert_eq!(
            parse_legacy_part_filename("movie.rar"),
            Some(("movie".to_string(), 1))
        );
        assert_eq!(
            parse_legacy_part_filename("movie.r00"),
            Some(("movie".to_string(), 2))
        );
        assert_eq!(
            parse_legacy_part_filename("movie.r01"),
            Some(("movie".to_string(), 3))
        );
        assert_eq!(
            parse_legacy_part_filename("movie.s00"),
            Some(("movie".to_string(), 102))
        );
        assert_eq!(parse_legacy_part_filename("movie.zip"), None);
    }

    #[test]
    fn test_parse_split_7z_filename() {
        assert_eq!(
            parse_split_7z_filename("backup.7z.001"),
            Some(("backup".to_string(), 1, 3))
        );
        assert_eq!(
            parse_split_7z_filename("data.7z.002"),
            Some(("data".to_string(), 2, 3))
        );
        assert_eq!(parse_split_7z_filename("plain.7z"), None);
    }

    #[test]
    fn test_detect_naming_scheme() {
        assert!(matches!(
            detect_naming_scheme(Path::new("video.part1.rar")),
            VolumeNamingScheme::ModernPart { .. }
        ));
        assert!(matches!(
            detect_naming_scheme(Path::new("video.r00")),
            VolumeNamingScheme::LegacyPart { .. }
        ));
        assert!(matches!(
            detect_naming_scheme(Path::new("video.7z.001")),
            VolumeNamingScheme::SplitSevenZip { .. }
        ));
        assert!(matches!(
            detect_naming_scheme(Path::new("standalone.zip")),
            VolumeNamingScheme::SingleFile
        ));
    }
}
