use std::path::{Path, PathBuf};

use unrar::Archive;

use crate::archive::ArchiveBackend;
use crate::encoding::{decode_bytes, encode_string};
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};

pub struct BackendUnrar;

/// Encodes the UTF-8 password into the raw bytes the UnRAR DLL will pass to its AES key
/// derivation. Without an explicit legacy encoding hint the password is kept as-is UTF-8 bytes,
/// which is what modern RAR5 tools expect. With a hint (e.g. "GBK"), the password is re-encoded
/// to match archives created by legacy tools that used raw GBK bytes as the passphrase.
fn make_password_bytes(password: &str, encoding: Option<&str>) -> Result<Vec<u8>, CheesyError> {
    match encoding {
        None => Ok(password.as_bytes().to_vec()),
        Some(_) => encode_string(password, encoding).map_err(CheesyError::Encoding),
    }
}

impl ArchiveBackend for BackendUnrar {
    fn parse_upfront(
        &self,
        paths: &[PathBuf],
        filename_encoding: Option<&str>,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError> {
        // UnRAR SDK follows multi-volume chains automatically from the first part path.
        let path = &paths[0];

        let password_bytes: Option<Vec<u8>> = password
            .map(|p| make_password_bytes(p, password_encoding))
            .transpose()?;

        let archive = match &password_bytes {
            None => Archive::new(path),
            Some(bytes) => Archive::with_password(path, bytes.as_slice()),
        };

        let open_archive = archive
            .open_for_listing()
            .map_err(|e| CheesyError::Parse(e.to_string()))?;

        let mut entries = Vec::new();
        for header_result in open_archive {
            let header = header_result.map_err(|e| CheesyError::Parse(e.to_string()))?;

            let raw_name = header.filename.to_string_lossy().to_string();
            let (decoded_path, encoding_used) =
                decode_bytes(raw_name.as_bytes(), filename_encoding);
            let name = Path::new(&decoded_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&decoded_path)
                .to_string();

            entries.push(VfsNode {
                path: decoded_path,
                name,
                size: header.unpacked_size,
                is_dir: header.is_directory(),
                encoding_used,
            });
        }

        Ok(VirtualFileSystem {
            archive_path: path.to_string_lossy().to_string(),
            total_entries: entries.len(),
            entries,
        })
    }

    fn extract_node(
        &self,
        paths: &[PathBuf],
        node: &VfsNode,
        dest: &Path,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<(), CheesyError> {
        // UnRAR SDK follows multi-volume chains automatically from the first part path.
        let archive_path = &paths[0];

        let password_bytes: Option<Vec<u8>> = password
            .map(|p| make_password_bytes(p, password_encoding))
            .transpose()?;

        let archive = match &password_bytes {
            None => Archive::new(archive_path),
            Some(bytes) => Archive::with_password(archive_path, bytes.as_slice()),
        };

        let mut cursor = archive
            .open_for_processing()
            .map_err(|e| CheesyError::Parse(e.to_string()))?;

        loop {
            match cursor
                .read_header()
                .map_err(|e| CheesyError::Parse(e.to_string()))?
            {
                None => break,
                Some(archive_with_file) => {
                    let raw_name = archive_with_file
                        .entry()
                        .filename
                        .to_string_lossy()
                        .to_string();

                    if raw_name == node.path {
                        if let Some(parent) = dest.parent() {
                            std::fs::create_dir_all(parent).map_err(CheesyError::Io)?;
                        }
                        archive_with_file
                            .extract_to(dest)
                            .map_err(|e| CheesyError::Parse(e.to_string()))?;
                        return Ok(());
                    }

                    cursor = archive_with_file
                        .skip()
                        .map_err(|e| CheesyError::Parse(e.to_string()))?;
                }
            }
        }

        Err(CheesyError::Parse(format!(
            "Entry '{}' not found in archive",
            node.path
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Fixtures are copied from the unrar crate's own test suite into src-tauri/data/.
    // Tests must run from the src-tauri/ directory (see CLAUDE.md).
    //
    //   version.rar      — single file "VERSION" containing "unrar-0.4.0", unencrypted
    //   crypted.rar      — single file ".gitignore" containing "target\nCargo.lock\n", password "unrar"
    //   multi.part1.rar  — first volume of a two-part RAR5 set:
    //                       vol1_content.txt (4809 B, starts "fileone:ABCDEFGH…")
    //                       vol2_content.txt (4809 B, starts "filetwo:IJKLMNOP…")
    //   multi.part2.rar  — second volume of the above set

    // ── parse_upfront ────────────────────────────────────────────────────────

    #[test]
    fn parse_upfront_returns_correct_entry_count_and_archive_path() {
        let path = PathBuf::from("data/version.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path.clone()], None, None, None)
            .unwrap();

        assert_eq!(vfs.total_entries, 1);
        assert_eq!(vfs.entries.len(), 1);
        assert_eq!(vfs.archive_path, path.to_string_lossy().as_ref());
    }

    #[test]
    fn parse_upfront_file_has_correct_metadata() {
        let path = PathBuf::from("data/version.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path], None, None, None)
            .unwrap();

        let node = vfs.entries.iter().find(|n| n.name == "VERSION").unwrap();
        assert_eq!(node.path, "VERSION");
        assert!(!node.is_dir);
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn parse_upfront_with_explicit_encoding_hint_is_recorded() {
        let path = PathBuf::from("data/version.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path], Some("GBK"), None, None)
            .unwrap();

        assert!(vfs.entries.iter().all(|n| n.encoding_used == "GBK"));
    }

    #[test]
    fn parse_upfront_encrypted_archive_listing_does_not_require_password() {
        // RAR4 encrypted-data archives expose headers without a password
        let path = PathBuf::from("data/crypted.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path], None, None, None)
            .unwrap();

        assert_eq!(vfs.total_entries, 1);
        assert_eq!(vfs.entries[0].name, ".gitignore");
    }

    // ── extract_node ─────────────────────────────────────────────────────────

    #[test]
    fn extract_node_produces_exact_file_content() {
        let path = PathBuf::from("data/version.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "VERSION")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendUnrar
            .extract_node(&[path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read_to_string(dest).unwrap(), "unrar-0.4.0");
    }

    #[test]
    fn extract_node_creates_intermediate_directories() {
        let path = PathBuf::from("data/version.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path.clone()], None, None, None)
            .unwrap();
        let node = vfs.entries.iter().next().unwrap().clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a").join("b").join("out.txt");
        BackendUnrar
            .extract_node(&[path], &node, &dest, None, None)
            .unwrap();

        assert!(dest.exists());
    }

    #[test]
    fn extract_node_missing_entry_returns_parse_error() {
        let path = PathBuf::from("data/version.rar");

        let ghost_node = VfsNode {
            path: "ghost.txt".to_string(),
            name: "ghost.txt".to_string(),
            size: 0,
            is_dir: false,
            encoding_used: "UTF-8".to_string(),
        };

        let tmp = tempfile::tempdir().unwrap();
        let result =
            BackendUnrar.extract_node(&[path], &ghost_node, &tmp.path().join("x"), None, None);

        assert!(matches!(result, Err(CheesyError::Parse(_))));
    }

    #[test]
    fn extract_node_with_correct_password_succeeds() {
        let path = PathBuf::from("data/crypted.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == ".gitignore")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendUnrar
            .extract_node(&[path], &node, &dest, Some("unrar"), None)
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(dest).unwrap(),
            "target\nCargo.lock\n"
        );
    }

    #[test]
    fn extract_node_with_wrong_password_returns_error() {
        let path = PathBuf::from("data/crypted.rar");

        let vfs = BackendUnrar
            .parse_upfront(&[path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == ".gitignore")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        let result = BackendUnrar.extract_node(&[path], &node, &dest, Some("wrong"), None);

        assert!(result.is_err());
    }

    // ── multi-volume ─────────────────────────────────────────────────────────
    //
    // NOTE: The `unrar` crate (v0.5.8) has a crash bug in its UCM_CHANGEVOLUMEW
    // callback for RAR5 multi-volume archives — `p1` can be null and
    // `WideCString::from_ptr_truncate` will SIGABRT. RAR4 multi-volume listing
    // works (the unrar crate's own `list_missing_volume` test confirms this), but
    // we cannot create RAR4 archives with RAR 7.x tools (RAR5 is the default).
    //
    // Multi-volume RAR support is architecturally sound: `BackendUnrar::parse_upfront`
    // and `extract_node` pass `paths[0]` to the UnRAR SDK, which follows the volume
    // chain automatically. When the crate ships a fixed callback, the
    // `data/multi.part1.rar` / `data/multi.part2.rar` fixtures (RAR5, two-part) can
    // be used to add a round-trip test here.
}
