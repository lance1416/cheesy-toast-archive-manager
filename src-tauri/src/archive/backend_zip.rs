use std::fs::File;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::archive::ArchiveBackend;
use crate::encoding::decode_bytes;
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};

pub struct BackendZip;

impl ArchiveBackend for BackendZip {
    fn parse_upfront(
        &self,
        paths: &[PathBuf],
        filename_encoding: Option<&str>,
        _password: Option<&str>,
        _password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError> {
        // BackendZip only handles single-file ZIPs; split ZIPs are routed to BackendLibarchive.
        let path = &paths[0];
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let mut entries = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let file_in_zip = archive.by_index_raw(i)?;

            let raw_name_bytes = file_in_zip.name_raw();
            let (decoded_path, encoding_used) = decode_bytes(raw_name_bytes, filename_encoding);

            // Extract the base name from the decoded path (handling both / and \ separators)
            let name = Path::new(&decoded_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&decoded_path)
                .to_string();

            entries.push(VfsNode {
                path: decoded_path,
                name,
                size: file_in_zip.size(),
                is_dir: file_in_zip.is_dir(),
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
        let archive_path = &paths[0];
        let file = File::open(archive_path)?;
        let mut archive = ZipArchive::new(file)?;

        let mut zipped_file = match password {
            Some(pass_text) => {
                let password_bytes = crate::encoding::encode_string(pass_text, password_encoding)
                    .map_err(CheesyError::Encoding)?;

                match archive.by_name_decrypt(&node.path, &password_bytes) {
                    Ok(f) => f,
                    Err(zip::result::ZipError::InvalidPassword) => {
                        return Err(CheesyError::Parse("Invalid password".to_string()))
                    }
                    Err(e) => return Err(CheesyError::Zip(e)),
                }
            }
            None => archive.by_name(&node.path)?,
        };

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(dest)?;
        std::io::copy(&mut zipped_file, &mut out_file)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use zip::unstable::write::FileOptionsExt;
    use zip::write::SimpleFileOptions;

    fn test_zip_path() -> PathBuf {
        PathBuf::from("data/file.zip")
    }

    /// Creates a ZipCrypto-encrypted single-file archive in a temp dir.
    /// Returns the TempDir (must stay alive for the path to remain valid) and the zip path.
    fn make_encrypted_zip(
        password: &[u8],
        filename: &str,
        content: &[u8],
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("protected.zip");
        let f = File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(f);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .with_deprecated_encryption(password)
            .unwrap();
        writer.start_file(filename, options).unwrap();
        writer.write_all(content).unwrap();
        writer.finish().unwrap();
        (dir, zip_path)
    }

    // ── parse_upfront ────────────────────────────────────────────────────────

    #[test]
    fn parse_upfront_returns_correct_entry_count_and_archive_path() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 3);
        assert_eq!(vfs.entries.len(), 3);
        assert_eq!(vfs.archive_path, test_zip_path().to_string_lossy().as_ref());
    }

    #[test]
    fn parse_upfront_root_file_has_correct_metadata() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        let node = vfs.entries.iter().find(|n| n.path == "file.txt").unwrap();
        assert_eq!(node.name, "file.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, 9); // b"file.txt\n"
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn parse_upfront_directory_entry_is_marked_as_dir() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        let dir_node = vfs.entries.iter().find(|n| n.name == "folder").unwrap();
        assert!(dir_node.is_dir);
        assert_eq!(dir_node.size, 0);
    }

    #[test]
    fn parse_upfront_nested_file_has_correct_path_and_name() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt")
            .unwrap();
        assert_eq!(node.name, "another_file.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, 17); // b"another_file.txt\n"
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn parse_upfront_explicit_encoding_hint_is_recorded_on_all_nodes() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], Some("GBK"), None, None)
            .unwrap();
        assert!(vfs.entries.iter().all(|n| n.encoding_used == "GBK"));
    }

    // ── extract_node ─────────────────────────────────────────────────────────

    #[test]
    fn extract_node_produces_exact_file_content() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "file.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendZip
            .extract_node(&[test_zip_path()], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), b"file.txt\n");
    }

    #[test]
    fn extract_node_creates_intermediate_directories() {
        let vfs = BackendZip
            .parse_upfront(&[test_zip_path()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("deep").join("nested").join("out.txt");
        BackendZip
            .extract_node(&[test_zip_path()], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), b"another_file.txt\n");
    }

    #[test]
    fn extract_node_with_correct_utf8_password_succeeds() {
        let content = b"sensitive data";
        let (_dir, zip_path) = make_encrypted_zip(b"correct-password", "secret.txt", content);

        let vfs = BackendZip
            .parse_upfront(&[zip_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "secret.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendZip
            .extract_node(&[zip_path], &node, &dest, Some("correct-password"), None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_with_wrong_password_returns_parse_error() {
        let (_dir, zip_path) = make_encrypted_zip(b"correct", "secret.txt", b"data");

        let vfs = BackendZip
            .parse_upfront(&[zip_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "secret.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        let result = BackendZip.extract_node(&[zip_path], &node, &dest, Some("wrong"), None);

        assert!(matches!(result, Err(CheesyError::Parse(_))));
    }

    #[test]
    fn extract_node_gbk_password_roundtrip() {
        // Simulates an archive created on a Chinese Windows system where the password "测试"
        // was encoded as GBK bytes before being passed to the ZIP encryption layer.
        let gbk_bytes: [u8; 4] = [0xB2, 0xE2, 0xCA, 0xD4]; // GBK for "测试"
        let content = b"protected content";
        let (_dir, zip_path) = make_encrypted_zip(&gbk_bytes, "secret.txt", content);

        let vfs = BackendZip
            .parse_upfront(&[zip_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "secret.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        // The user types the UTF-8 string "测试"; the backend re-encodes it to GBK before decryption
        BackendZip
            .extract_node(&[zip_path], &node, &dest, Some("测试"), Some("GBK"))
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }
}
