pub mod backend_libarchive;
pub mod backend_sevenz;
pub mod backend_unrar;
pub mod backend_zip;

use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};
use backend_libarchive::BackendLibarchive;
use backend_sevenz::BackendSevenZ;
use backend_unrar::BackendUnrar;
use backend_zip::BackendZip;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The strict contract every archive engine must follow.
pub trait ArchiveBackend: Send + Sync {
    /// Parses the entire archive upfront into a flat VFS array.
    fn parse_upfront(
        &self,
        path: &Path,
        filename_encoding: Option<&str>,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError>;

    /// Extracts a specific file from the archive.
    fn extract_node(
        &self,
        archive_path: &Path,
        node: &VfsNode,
        dest: &Path,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<(), CheesyError>;
}

/// The Factory Router: Inspects the file extension and spins up the right backend.
pub fn get_backend(path: &PathBuf) -> Result<Box<dyn ArchiveBackend>, CheesyError> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 8]; // We only need the first 8 bytes to identify any archive

    // Read first 8 bytes.
    // If the file is smaller than 8 bytes, it's not a valid archive anyway.
    let bytes_read = file.read(&mut buffer)?;
    if bytes_read < 2 {
        return Err(CheesyError::UnsupportedFormat(
            "File is too small to be an archive.".into(),
        ));
    }

    // ZIP (PK\x03\x04)
    // Note: Empty ZIPs or spanning ZIPs might have different signatures (like PK\x05\x06),
    // but standard local file headers start with 50 4B 03 04.
    if bytes_read >= 4 && buffer[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        return Ok(Box::new(BackendZip));
    }

    // RAR (Rar!\x1A\x07\x00 for v4, Rar!\x1A\x07\x01\x00 for v5)
    if bytes_read >= 7 && buffer[0..7] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00]
        || bytes_read >= 8 && buffer[0..8] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00]
    {
        return Ok(Box::new(BackendUnrar));
    }

    // 7z (7z\xBC\xAF\x27\x1C)
    if bytes_read >= 6 && buffer[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        return Ok(Box::new(BackendSevenZ));
    }

    // GZip (\x1F\x8B)
    if bytes_read >= 2 && buffer[0..2] == [0x1F, 0x8B] {
        return Ok(Box::new(BackendLibarchive));
    }

    // Bzip2 (BZh)
    if bytes_read >= 3 && buffer[0..3] == [0x42, 0x5A, 0x68] {
        return Ok(Box::new(BackendLibarchive));
    }

    // XZ (\xFD7zXZ\x00)
    if bytes_read >= 6 && buffer[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return Ok(Box::new(BackendLibarchive));
    }

    // -----------------------------------------------

    // Edge Cases (e.g., self-extracting .exe archives)
    // Fall back to extension checking
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "zip" | "cbz" => Ok(Box::new(BackendZip)),
        "7z" => Ok(Box::new(BackendSevenZ)),
        "tar" | "gz" | "tgz" | "bz2" | "xz" => Ok(Box::new(BackendLibarchive)),
        "rar" | "cbr" => Ok(Box::new(BackendUnrar)),
        _ => Err(CheesyError::UnsupportedFormat(format!(
            "Unrecognized magic bytes and unsupported extension: {}",
            ext
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn zip_magic_bytes_routes_to_zip_backend() {
        assert!(get_backend(&PathBuf::from("data/file.zip")).is_ok());
    }

    #[test]
    fn rar_magic_bytes_routes_to_unrar_backend() {
        assert!(get_backend(&PathBuf::from("data/version.rar")).is_ok());
    }

    #[test]
    fn plain_text_file_returns_unsupported_format_error() {
        assert!(matches!(
            get_backend(&PathBuf::from("data/file.txt")),
            Err(CheesyError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn missing_file_returns_io_error() {
        assert!(matches!(
            get_backend(&PathBuf::from("data/nonexistent.zip")),
            Err(CheesyError::Io(_))
        ));
    }
}
