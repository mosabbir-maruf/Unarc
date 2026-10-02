//! Archive format detection and classification.

use crate::error::ArchiveError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// Supported archive format categories in Unarc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveFormat {
    /// Standard ZIP archive (.zip)
    Zip,
    /// 7-Zip archive (.7z)
    SevenZip,
    /// Uncompressed Tape Archive (.tar)
    Tar,
    /// Gzip-compressed Tape Archive (.tar.gz, .tgz)
    TarGz,
    /// Bzip2-compressed Tape Archive (.tar.bz2, .tbz2)
    TarBz2,
    /// XZ-compressed Tape Archive (.tar.xz, .txz)
    TarXz,
    /// RAR archive (.rar, RAR 4.x and earlier)
    Rar,
    /// RAR5 archive (.rar, RAR 5.x)
    Rar5,
}

impl fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip => write!(f, "ZIP"),
            Self::SevenZip => write!(f, "7-Zip"),
            Self::Tar => write!(f, "TAR"),
            Self::TarGz => write!(f, "TAR.GZ"),
            Self::TarBz2 => write!(f, "TAR.BZ2"),
            Self::TarXz => write!(f, "TAR.XZ"),
            Self::Rar => write!(f, "RAR"),
            Self::Rar5 => write!(f, "RAR5"),
        }
    }
}

impl ArchiveFormat {
    /// Returns true if the format is RAR or RAR5.
    #[must_use]
    pub fn is_rar(&self) -> bool {
        matches!(self, Self::Rar | Self::Rar5)
    }

    /// Detects the archive format from file extension.
    #[must_use]
    pub fn from_extension(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();

        // Check compound extensions first
        if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            return Some(Self::TarGz);
        }
        if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
            return Some(Self::TarBz2);
        }
        if name.ends_with(".tar.xz") || name.ends_with(".txz") {
            return Some(Self::TarXz);
        }

        // Single extensions
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        match ext.as_str() {
            "zip" => Some(Self::Zip),
            "7z" => Some(Self::SevenZip),
            "tar" => Some(Self::Tar),
            "rar" => Some(Self::Rar),
            _ => {
                // Legacy multipart RAR volume extensions: .r00.. .r99, .s00.. .s99
                if (ext.starts_with('r') || ext.starts_with('s')) && ext.len() == 3 {
                    let digits = &ext[1..];
                    if digits.chars().all(|c| c.is_ascii_digit()) {
                        return Some(Self::Rar);
                    }
                }
                None
            }
        }
    }

    /// Detects format from magic bytes (file signature header).
    #[must_use]
    pub fn from_magic_bytes(header: &[u8]) -> Option<Self> {
        // Check 7-Zip magic: '7' 'z' 0xBC 0xAF 0x27 0x1C
        if header.len() >= 6 && header.starts_with(b"7z\xBC\xAF\x27\x1C") {
            return Some(Self::SevenZip);
        }

        // Check ZIP magic: PK\x03\x04 or PK\x05\x06 (empty) or PK\x07\x08 (spanned)
        if header.len() >= 4
            && (header.starts_with(b"PK\x03\x04") || header.starts_with(b"PK\x05\x06"))
        {
            return Some(Self::Zip);
        }

        // Check RAR5 magic: Rar!\x1A\x07\x01\x00 (v5)
        if header.len() >= 8 && header.starts_with(b"Rar!\x1A\x07\x01\x00") {
            return Some(Self::Rar5);
        }

        // Check RAR4 magic: Rar!\x1A\x07\x00 (v4)
        if header.len() >= 7 && header.starts_with(b"Rar!\x1A\x07\x00") {
            return Some(Self::Rar);
        }

        // Check general RAR magic prefix
        if header.len() >= 7 && header.starts_with(b"Rar!\x1A\x07") {
            return Some(Self::Rar);
        }

        // Check GZIP magic: 0x1F 0x8B
        if header.len() >= 2 && header.starts_with(b"\x1F\x8B") {
            return Some(Self::TarGz);
        }

        // Check BZIP2 magic: BZh
        if header.len() >= 3 && header.starts_with(b"BZh") {
            return Some(Self::TarBz2);
        }

        // Check XZ magic: 0xFD '7' 'z' 'X' 'Z' 0x00
        if header.len() >= 6 && header.starts_with(b"\xFD7zXZ\x00") {
            return Some(Self::TarXz);
        }

        // Check TAR ustar magic at offset 257
        if header.len() >= 262 && &header[257..262] == b"ustar" {
            return Some(Self::Tar);
        }

        None
    }

    /// Identifies the format from a path or returns an UnsupportedFormat error.
    pub fn from_path(path: &Path) -> Result<Self, ArchiveError> {
        Self::from_extension(path).ok_or_else(|| ArchiveError::UnsupportedFormat {
            format: path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_extension() {
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("archive.zip")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("backup.7z")),
            Some(ArchiveFormat::SevenZip)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("data.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("data.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("bundle.tar.bz2")),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("bundle.tbz2")),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("image.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("image.txz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("files.tar")),
            Some(ArchiveFormat::Tar)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("legacy.rar")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("legacy.r00")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("legacy.r01")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_extension(Path::new("movie.part1.rar")),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(ArchiveFormat::from_extension(Path::new("plain.txt")), None);
    }

    #[test]
    fn test_format_from_magic_bytes() {
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"PK\x03\x04payload"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"7z\xBC\xAF\x27\x1Cdata"),
            Some(ArchiveFormat::SevenZip)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"\x1F\x8B\x08compressed"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"BZh91AY&SY"),
            Some(ArchiveFormat::TarBz2)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"\xFD7zXZ\x00extra"),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"Rar!\x1A\x07\x00data"),
            Some(ArchiveFormat::Rar)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"Rar!\x1A\x07\x01\x00data"),
            Some(ArchiveFormat::Rar5)
        );
        assert_eq!(
            ArchiveFormat::from_magic_bytes(b"unknown bytes header"),
            None
        );
    }

    #[test]
    fn test_format_display() {
        assert_eq!(ArchiveFormat::Zip.to_string(), "ZIP");
        assert_eq!(ArchiveFormat::SevenZip.to_string(), "7-Zip");
        assert_eq!(ArchiveFormat::TarGz.to_string(), "TAR.GZ");
        assert_eq!(ArchiveFormat::Rar.to_string(), "RAR");
        assert_eq!(ArchiveFormat::Rar5.to_string(), "RAR5");
        assert!(ArchiveFormat::Rar.is_rar());
        assert!(ArchiveFormat::Rar5.is_rar());
        assert!(!ArchiveFormat::Zip.is_rar());
    }
}
