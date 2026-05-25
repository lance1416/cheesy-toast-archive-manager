pub mod backend_libarchive;
pub mod backend_sevenz;
pub mod backend_unrar;
pub mod backend_zip;
pub mod writer;
pub mod writer_libarchive;
pub mod writer_sevenz;
pub mod writer_zip;

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
    /// `paths` is an ordered list of volume files; single-file archives have exactly one element.
    fn parse_upfront(
        &self,
        paths: &[PathBuf],
        filename_encoding: Option<&str>,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError>;

    /// Extracts a specific file from the archive.
    /// `paths` is an ordered list of volume files; single-file archives have exactly one element.
    fn extract_node(
        &self,
        paths: &[PathBuf],
        node: &VfsNode,
        dest: &Path,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<(), CheesyError>;
}

/// The Factory Router: Inspects the file's magic bytes and naming to select the right backend
/// and collect all volume paths.
pub fn get_backend(path: &Path) -> Result<(Box<dyn ArchiveBackend>, Vec<PathBuf>), CheesyError> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 8];

    let bytes_read = file.read(&mut buffer)?;
    if bytes_read < 2 {
        return Err(CheesyError::UnsupportedFormat(
            "File is too small to be an archive.".into(),
        ));
    }

    // ZIP (PK\x03\x04) — also the magic for the first volume of a split ZIP
    if bytes_read >= 4 && buffer[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        let volumes = detect_volumes(path);
        return if volumes.len() > 1 {
            Ok((Box::new(BackendLibarchive), volumes))
        } else {
            Ok((Box::new(BackendZip), volumes))
        };
    }

    // RAR (v4: Rar!\x1A\x07\x00 | v5: Rar!\x1A\x07\x01\x00)
    // UnRAR SDK follows multi-volume chains from the first part path automatically.
    if bytes_read >= 7 && buffer[0..7] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00]
        || bytes_read >= 8 && buffer[0..8] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00]
    {
        return Ok((Box::new(BackendUnrar), vec![path.to_owned()]));
    }

    // 7z (\x37\x7A\xBC\xAF\x27\x1C) — also the magic for the first volume of a split 7z
    if bytes_read >= 6 && buffer[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        let volumes = detect_volumes(path);
        return Ok((Box::new(BackendSevenZ), volumes));
    }

    // GZip (\x1F\x8B)
    if bytes_read >= 2 && buffer[0..2] == [0x1F, 0x8B] {
        return Ok((Box::new(BackendLibarchive), vec![path.to_owned()]));
    }

    // Bzip2 (BZh)
    if bytes_read >= 3 && buffer[0..3] == [0x42, 0x5A, 0x68] {
        return Ok((Box::new(BackendLibarchive), vec![path.to_owned()]));
    }

    // XZ (\xFD7zXZ\x00)
    if bytes_read >= 6 && buffer[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        return Ok((Box::new(BackendLibarchive), vec![path.to_owned()]));
    }

    // -----------------------------------------------

    // Edge cases — fall back to extension
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "zip" | "cbz" => {
            let volumes = detect_volumes(path);
            if volumes.len() > 1 {
                Ok((Box::new(BackendLibarchive), volumes))
            } else {
                Ok((Box::new(BackendZip), volumes))
            }
        }
        "7z" => {
            let volumes = detect_volumes(path);
            Ok((Box::new(BackendSevenZ), volumes))
        }
        "tar" | "gz" | "tgz" | "bz2" | "xz" => {
            Ok((Box::new(BackendLibarchive), vec![path.to_owned()]))
        }
        "rar" | "cbr" => Ok((Box::new(BackendUnrar), vec![path.to_owned()])),
        // Split ZIP volumes: .z01, .z02, ..., .z10, .z100, etc.
        _ if !ext.is_empty()
            && ext.starts_with('z')
            && ext[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            Ok((Box::new(BackendLibarchive), detect_volumes(path)))
        }
        // 7z multi-volume: .7z.001, .7z.002, ... — extension is purely digits, stem ends ".7z"
        _ if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_digit()) => {
            Ok((Box::new(BackendSevenZ), detect_volumes(path)))
        }
        _ => Err(CheesyError::UnsupportedFormat(format!(
            "Unrecognized magic bytes and unsupported extension: {}",
            ext
        ))),
    }
}

/// Returns an ordered list of all volume paths for the archive that `first` belongs to.
/// For single-file formats the list contains only `first`.
fn detect_volumes(first: &Path) -> Vec<PathBuf> {
    let dir = first
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let stem = match first.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return vec![first.to_owned()],
    };

    let ext = first
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 7z multi-volume: "archive.7z.001" → stem="archive.7z", ext="001"
    if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_digit()) && stem.ends_with(".7z") {
        return collect_numbered_volumes(dir, stem);
    }

    // Split ZIP: user selected a non-final part (.z01, .z02, ...)
    if !ext.is_empty() && ext.starts_with('z') && ext[1..].chars().all(|c| c.is_ascii_digit()) {
        return collect_zip_volumes(dir, stem);
    }

    // Split ZIP: user selected the final .zip file; check for a .z01 sibling
    if ext == "zip" {
        let z01 = dir.join(format!("{}.z01", stem));
        if z01.exists() {
            return collect_zip_volumes(dir, stem);
        }
    }

    vec![first.to_owned()]
}

/// Collects *.7z.NNN volumes (where stem ends with ".7z") sorted lexicographically.
/// Zero-padded numbering (001, 002, …) means lex order == numeric order.
fn collect_numbered_volumes(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut parts: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let p_stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let p_ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            p_stem == stem && !p_ext.is_empty() && p_ext.chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    parts.sort();
    if parts.is_empty() {
        vec![dir.join(format!("{}.001", stem))]
    } else {
        parts
    }
}

/// Collects split-ZIP volumes (.z01, .z02, … + .zip) sorted so the .zNN parts come first.
fn collect_zip_volumes(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut parts: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let p_stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let p_ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            p_stem == stem
                && !p_ext.is_empty()
                && p_ext.starts_with('z')
                && p_ext[1..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    parts.sort();
    let zip_cap = dir.join(format!("{}.zip", stem));
    if zip_cap.exists() {
        parts.push(zip_cap);
    }
    if parts.is_empty() {
        vec![dir.join(format!("{}.zip", stem))]
    } else {
        parts
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

    // ── detect_volumes ───────────────────────────────────────────────────────

    #[test]
    fn detect_volumes_single_zip_returns_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let zip = tmp.path().join("archive.zip");
        std::fs::write(&zip, b"").unwrap();
        assert_eq!(detect_volumes(&zip), vec![zip]);
    }

    #[test]
    fn detect_volumes_zip_with_z01_sibling_returns_all_parts() {
        let tmp = tempfile::tempdir().unwrap();
        let z01 = tmp.path().join("archive.z01");
        let z02 = tmp.path().join("archive.z02");
        let zip = tmp.path().join("archive.zip");
        std::fs::write(&z01, b"").unwrap();
        std::fs::write(&z02, b"").unwrap();
        std::fs::write(&zip, b"").unwrap();

        let volumes = detect_volumes(&zip);
        assert_eq!(volumes, vec![z01, z02, zip]);
    }

    #[test]
    fn detect_volumes_z01_selected_returns_all_parts() {
        let tmp = tempfile::tempdir().unwrap();
        let z01 = tmp.path().join("archive.z01");
        let z02 = tmp.path().join("archive.z02");
        let zip = tmp.path().join("archive.zip");
        std::fs::write(&z01, b"").unwrap();
        std::fs::write(&z02, b"").unwrap();
        std::fs::write(&zip, b"").unwrap();

        let volumes = detect_volumes(&z01);
        assert_eq!(volumes, vec![z01, z02, zip]);
    }

    #[test]
    fn detect_volumes_7z_multivolume_returns_all_parts() {
        let tmp = tempfile::tempdir().unwrap();
        let part1 = tmp.path().join("archive.7z.001");
        let part2 = tmp.path().join("archive.7z.002");
        let part3 = tmp.path().join("archive.7z.003");
        std::fs::write(&part1, b"").unwrap();
        std::fs::write(&part2, b"").unwrap();
        std::fs::write(&part3, b"").unwrap();

        let volumes = detect_volumes(&part1);
        assert_eq!(volumes, vec![part1, part2, part3]);
    }

    #[test]
    fn detect_volumes_single_7z_returns_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join("archive.7z");
        std::fs::write(&archive, b"").unwrap();
        assert_eq!(detect_volumes(&archive), vec![archive]);
    }

    #[test]
    fn detect_volumes_rar_returns_only_first_part() {
        let tmp = tempfile::tempdir().unwrap();
        let part1 = tmp.path().join("archive.part1.rar");
        let part2 = tmp.path().join("archive.part2.rar");
        std::fs::write(&part1, b"").unwrap();
        std::fs::write(&part2, b"").unwrap();
        // RAR: return only the first path; UnRAR SDK handles the chain internally
        assert_eq!(detect_volumes(&part1), vec![part1]);
    }
}
