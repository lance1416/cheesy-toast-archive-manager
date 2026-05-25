use sevenz_rust2::{ArchiveReader, Password};
use std::fs::File;
use std::path::Path;

use crate::archive::ArchiveBackend;
use crate::encoding::{decode_bytes, encode_string};
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};

pub struct BackendSevenZ;

/// Builds a sevenz-rust2 Password from the user-supplied UTF-8 string and an optional
/// legacy encoding label.
///
/// 7z's AES key derivation operates on the password bytes directly. Standard 7-Zip encodes
/// the passphrase as UTF-16 LE (Password::new). Archives created by legacy tools that passed
/// raw GBK/Shift-JIS bytes can be opened by supplying the matching encoding label, which
/// re-encodes the UTF-8 input to those raw bytes via Password::from_raw.
fn make_password(password: Option<&str>, encoding: Option<&str>) -> Result<Password, CheesyError> {
    match password {
        None => Ok(Password::empty()),
        Some(pass) => match encoding {
            None => Ok(Password::new(pass)),
            Some(_) => {
                let bytes = encode_string(pass, encoding).map_err(CheesyError::Encoding)?;
                Ok(Password::from_raw(&bytes))
            }
        },
    }
}

impl ArchiveBackend for BackendSevenZ {
    fn parse_upfront(
        &self,
        path: &Path,
        filename_encoding: Option<&str>,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError> {
        let pass = make_password(password, password_encoding)?;

        // Metadata lives in the archive header — no decompression needed.
        let reader =
            ArchiveReader::open(path, pass).map_err(|e| CheesyError::Parse(e.to_string()))?;

        let entries = reader
            .archive()
            .files
            .iter()
            .map(|entry| {
                // 7z stores filenames as UTF-16 LE internally; sevenz-rust2 yields them as
                // UTF-8. decode_bytes still honours an explicit hint for rare non-compliant
                // archives that embed raw legacy bytes in the name field.
                let (decoded_path, encoding_used) =
                    decode_bytes(entry.name.as_bytes(), filename_encoding);

                let name = Path::new(&decoded_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&decoded_path)
                    .to_string();

                VfsNode {
                    path: decoded_path,
                    name,
                    size: entry.size,
                    is_dir: entry.is_directory,
                    encoding_used,
                }
            })
            .collect::<Vec<_>>();

        Ok(VirtualFileSystem {
            archive_path: path.to_string_lossy().to_string(),
            total_entries: entries.len(),
            entries,
        })
    }

    fn extract_node(
        &self,
        archive_path: &Path,
        node: &VfsNode,
        dest: &Path,
        password: Option<&str>,
        password_encoding: Option<&str>,
    ) -> Result<(), CheesyError> {
        let pass = make_password(password, password_encoding)?;

        let mut reader = ArchiveReader::open(archive_path, pass)
            .map_err(|e| CheesyError::Parse(e.to_string()))?;

        let target = &node.path;
        let mut found = false;

        // Accumulate I/O errors that happen inside the closure (the closure must return
        // sevenz_rust2::Error, not CheesyError).
        let mut io_err: Option<std::io::Error> = None;

        reader
            .for_each_entries(|entry, r| {
                if &entry.name != target {
                    return Ok(true); // skip — sevenz-rust2 advances the block stream internally
                }

                if let Some(parent) = dest.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        io_err = Some(e);
                        return Ok(false);
                    }
                }

                match File::create(dest) {
                    Err(e) => {
                        io_err = Some(e);
                        Ok(false)
                    }
                    Ok(mut out) => match std::io::copy(r, &mut out) {
                        Err(e) => {
                            io_err = Some(e);
                            Ok(false)
                        }
                        Ok(_) => {
                            found = true;
                            Ok(false) // stop iterating
                        }
                    },
                }
            })
            .map_err(|e| CheesyError::Parse(e.to_string()))?;

        if let Some(e) = io_err {
            return Err(CheesyError::Io(e));
        }

        if !found {
            return Err(CheesyError::Parse(format!(
                "Entry '{}' not found in archive",
                node.path
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Creates a single-file, unencrypted 7z archive in a temp dir.
    /// Returns (TempDir — keep alive!, archive path).
    fn make_7z(filename: &str, content: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join(filename);
        std::fs::write(&src, content).unwrap();
        let archive_path = dir.path().join("test.7z");
        sevenz_rust2::compress_to_path(&src, &archive_path).unwrap();
        (dir, archive_path)
    }

    /// Creates a password-encrypted 7z archive.
    fn make_encrypted_7z(
        filename: &str,
        content: &[u8],
        password: &str,
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join(filename);
        std::fs::write(&src, content).unwrap();
        let archive_path = dir.path().join("test.7z");
        sevenz_rust2::compress_to_path_encrypted(&src, &archive_path, password.into()).unwrap();
        (dir, archive_path)
    }

    // ── parse_upfront ────────────────────────────────────────────────────────

    #[test]
    fn parse_upfront_returns_correct_entry_count_and_archive_path() {
        let content = b"hello from 7z";
        let (_dir, archive_path) = make_7z("hello.txt", content);

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, None, None)
            .unwrap();

        assert_eq!(vfs.total_entries, 1);
        assert_eq!(vfs.entries.len(), 1);
        assert_eq!(vfs.archive_path, archive_path.to_string_lossy().as_ref());
    }

    #[test]
    fn parse_upfront_file_has_correct_metadata() {
        let content = b"hello from 7z";
        let (_dir, archive_path) = make_7z("hello.txt", content);

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, None, None)
            .unwrap();

        let node = vfs.entries.iter().find(|n| n.name == "hello.txt").unwrap();
        assert_eq!(node.path, "hello.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, content.len() as u64);
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn parse_upfront_with_explicit_encoding_hint_is_recorded() {
        let (_dir, archive_path) = make_7z("hello.txt", b"data");

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, Some("GBK"), None, None)
            .unwrap();

        assert!(vfs.entries.iter().all(|n| n.encoding_used == "GBK"));
    }

    // ── extract_node ─────────────────────────────────────────────────────────

    #[test]
    fn extract_node_produces_exact_file_content() {
        let content = b"exact content check";
        let (_dir, archive_path) = make_7z("file.txt", content);

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "file.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendSevenZ
            .extract_node(&archive_path, &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_creates_intermediate_directories() {
        let content = b"nested";
        let (_dir, archive_path) = make_7z("file.txt", content);

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, None, None)
            .unwrap();
        let node = vfs.entries.iter().next().unwrap().clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a").join("b").join("out.txt");
        BackendSevenZ
            .extract_node(&archive_path, &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_missing_entry_returns_parse_error() {
        let (_dir, archive_path) = make_7z("real.txt", b"data");

        let ghost_node = VfsNode {
            path: "ghost.txt".to_string(),
            name: "ghost.txt".to_string(),
            size: 0,
            is_dir: false,
            encoding_used: "UTF-8".to_string(),
        };

        let tmp = tempfile::tempdir().unwrap();
        let result = BackendSevenZ.extract_node(
            &archive_path,
            &ghost_node,
            &tmp.path().join("x"),
            None,
            None,
        );

        assert!(matches!(result, Err(CheesyError::Parse(_))));
    }

    #[test]
    fn extract_node_with_correct_password_succeeds() {
        let content = b"secret data";
        let (_dir, archive_path) = make_encrypted_7z("secret.txt", content, "correct-pass");

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, Some("correct-pass"), None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "secret.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendSevenZ
            .extract_node(&archive_path, &node, &dest, Some("correct-pass"), None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_with_wrong_password_returns_error() {
        let (_dir, archive_path) = make_encrypted_7z("secret.txt", b"data", "correct");

        let vfs = BackendSevenZ
            .parse_upfront(&archive_path, None, Some("correct"), None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "secret.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        let result = BackendSevenZ.extract_node(&archive_path, &node, &dest, Some("wrong"), None);

        assert!(result.is_err());
    }
}
