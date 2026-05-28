use std::path::{Path, PathBuf};

use libarchive2::{FileType, ReadArchive};

use crate::archive::ArchiveBackend;
use crate::encoding::decode_bytes;
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};

pub struct BackendLibarchive;

fn open_archive<'a>(
    paths: &[PathBuf],
    password: Option<&str>,
) -> Result<ReadArchive<'a>, CheesyError> {
    let result = match password {
        None => ReadArchive::open_filenames(paths),
        Some(pass) => ReadArchive::open_filenames_with_passphrase(paths, pass),
    };
    result.map_err(|e| CheesyError::Parse(e.to_string()))
}

impl ArchiveBackend for BackendLibarchive {
    fn parse_upfront(
        &self,
        paths: &[PathBuf],
        filename_encoding: Option<&str>,
        password: Option<&str>,
        _password_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError> {
        let mut archive = open_archive(paths, password)?;
        let mut entries = Vec::new();

        while let Some(entry) = archive
            .next_entry()
            .map_err(|e| CheesyError::Parse(e.to_string()))?
        {
            let pathname = entry.pathname().unwrap_or_default();
            let is_dir = matches!(entry.file_type(), FileType::Directory);
            let size = entry.size().max(0) as u64;

            let (decoded_path, encoding_used) =
                decode_bytes(pathname.as_bytes(), filename_encoding);
            let name = Path::new(&decoded_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&decoded_path)
                .to_string();

            entries.push(VfsNode {
                path: decoded_path,
                name,
                size,
                is_dir,
                encoding_used,
            });
        }

        Ok(VirtualFileSystem {
            archive_path: paths[0].to_string_lossy().to_string(),
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
        _password_encoding: Option<&str>,
    ) -> Result<(), CheesyError> {
        let mut archive = open_archive(paths, password)?;

        loop {
            match archive
                .next_entry()
                .map_err(|e| CheesyError::Parse(e.to_string()))?
            {
                None => break,
                Some(entry) => {
                    let pathname = entry.pathname().unwrap_or_default();
                    let is_match = pathname == node.path;

                    if is_match {
                        if let Some(parent) = dest.parent() {
                            std::fs::create_dir_all(parent).map_err(CheesyError::Io)?;
                        }
                        let data = archive
                            .read_data_to_vec()
                            .map_err(|e| CheesyError::Parse(e.to_string()))?;
                        std::fs::write(dest, data).map_err(CheesyError::Io)?;
                        return Ok(());
                    }
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
    use crate::archive::writer::{collect_entries, ArchiveWriter, WriteOptions};
    use crate::archive::writer_libarchive::{TarArchiveWriter, TarCompression as TC};
    use std::path::PathBuf;

    /// Creates a 3-entry canonical archive (file.txt + folder/ + folder/another_file.txt)
    /// using the given compression. Mirrors the data/file.zip structure so all backends
    /// are exercised on identical logical content without committing binary blobs.
    fn make_canonical_tar(compression: TC, ext: &str) -> (tempfile::TempDir, PathBuf) {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("file.txt"), b"file.txt\n").unwrap();
        let folder = src.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("another_file.txt"), b"another_file.txt\n").unwrap();
        let entries = collect_entries(&[src.path().join("file.txt"), folder.clone()]).unwrap();
        let out = tempfile::tempdir().unwrap();
        let archive = out.path().join(format!("fixture.{ext}"));
        TarArchiveWriter { compression }
            .create(&entries, &archive, &WriteOptions::default())
            .unwrap();
        (out, archive)
    }

    fn make_tar_gz(filename: &str, content: &[u8]) -> (tempfile::TempDir, PathBuf) {
        use libarchive2::{ArchiveFormat, CompressionFormat, WriteArchive};
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.tar.gz");
        let mut writer = WriteArchive::new()
            .format(ArchiveFormat::TarGnu)
            .compression(CompressionFormat::Gzip)
            .open_file(&archive_path)
            .unwrap();
        writer.add_file(filename, content).unwrap();
        writer.finish().unwrap();
        (dir, archive_path)
    }

    fn make_tar_bz2(filename: &str, content: &[u8]) -> (tempfile::TempDir, PathBuf) {
        use libarchive2::{ArchiveFormat, CompressionFormat, WriteArchive};
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.tar.bz2");
        let mut writer = WriteArchive::new()
            .format(ArchiveFormat::TarGnu)
            .compression(CompressionFormat::Bzip2)
            .open_file(&archive_path)
            .unwrap();
        writer.add_file(filename, content).unwrap();
        writer.finish().unwrap();
        (dir, archive_path)
    }

    fn make_tar_xz(filename: &str, content: &[u8]) -> (tempfile::TempDir, PathBuf) {
        use libarchive2::{ArchiveFormat, CompressionFormat, WriteArchive};
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("test.tar.xz");
        let mut writer = WriteArchive::new()
            .format(ArchiveFormat::TarGnu)
            .compression(CompressionFormat::Xz)
            .open_file(&archive_path)
            .unwrap();
        writer.add_file(filename, content).unwrap();
        writer.finish().unwrap();
        (dir, archive_path)
    }

    // ── parse_upfront ────────────────────────────────────────────────────────

    #[test]
    fn parse_upfront_returns_correct_entry_count_and_archive_path() {
        let content = b"hello from tar";
        let (_dir, archive_path) = make_tar_gz("hello.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();

        assert_eq!(vfs.total_entries, 1);
        assert_eq!(vfs.entries.len(), 1);
        assert_eq!(vfs.archive_path, archive_path.to_string_lossy().as_ref());
    }

    #[test]
    fn parse_upfront_file_has_correct_metadata() {
        let content = b"hello from tar";
        let (_dir, archive_path) = make_tar_gz("hello.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path], None, None, None)
            .unwrap();

        let node = vfs.entries.iter().find(|n| n.name == "hello.txt").unwrap();
        assert_eq!(node.path, "hello.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, content.len() as u64);
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn parse_upfront_with_explicit_encoding_hint_is_recorded() {
        let (_dir, archive_path) = make_tar_gz("hello.txt", b"data");

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path], Some("GBK"), None, None)
            .unwrap();

        assert!(vfs.entries.iter().all(|n| n.encoding_used == "GBK"));
    }

    // ── extract_node ─────────────────────────────────────────────────────────

    #[test]
    fn extract_node_produces_exact_file_content() {
        let content = b"exact content check";
        let (_dir, archive_path) = make_tar_gz("file.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "file.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_creates_intermediate_directories() {
        let content = b"nested";
        let (_dir, archive_path) = make_tar_gz("file.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();
        let node = vfs.entries.iter().next().unwrap().clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("a").join("b").join("out.txt");
        BackendLibarchive
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn extract_node_missing_entry_returns_parse_error() {
        let (_dir, archive_path) = make_tar_gz("real.txt", b"data");

        let ghost_node = VfsNode {
            path: "ghost.txt".to_string(),
            name: "ghost.txt".to_string(),
            size: 0,
            is_dir: false,
            encoding_used: "UTF-8".to_string(),
        };

        let tmp = tempfile::tempdir().unwrap();
        let result = BackendLibarchive.extract_node(
            &[archive_path],
            &ghost_node,
            &tmp.path().join("x"),
            None,
            None,
        );

        assert!(matches!(result, Err(CheesyError::Parse(_))));
    }

    #[test]
    fn bzip2_archive_parse_and_extract_round_trip() {
        let content = b"bzip2 content";
        let (_dir, archive_path) = make_tar_bz2("data.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "data.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    #[test]
    fn xz_archive_parse_and_extract_round_trip() {
        let content = b"xz content";
        let (_dir, archive_path) = make_tar_xz("data.txt", content);

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "data.txt")
            .unwrap()
            .clone();

        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(std::fs::read(dest).unwrap(), content);
    }

    // ── canonical-tree fixtures (generated on-the-fly) ───────────────────────
    // Generated archives contain 2 file entries: file.txt and folder/another_file.txt.
    // collect_entries walks directories but does not add directory entries, so no
    // is_dir entry appears here — that behaviour is tested via data/file.zip (BackendZip).

    #[test]
    fn tar_gz_fixture_parse_upfront_has_correct_entry_count() {
        let (_dir, archive) = make_canonical_tar(TC::Gzip, "tar.gz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 2);
    }

    #[test]
    fn tar_gz_fixture_root_file_has_correct_metadata() {
        let (_dir, archive) = make_canonical_tar(TC::Gzip, "tar.gz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive], None, None, None)
            .unwrap();
        let node = vfs.entries.iter().find(|n| n.path == "file.txt").unwrap();
        assert_eq!(node.name, "file.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, 9);
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn tar_gz_fixture_nested_file_has_correct_metadata() {
        let (_dir, archive) = make_canonical_tar(TC::Gzip, "tar.gz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt")
            .unwrap();
        assert_eq!(node.name, "another_file.txt");
        assert!(!node.is_dir);
        assert_eq!(node.size, 17);
        assert_eq!(node.encoding_used, "UTF-8");
    }

    #[test]
    fn tar_gz_fixture_extract_produces_exact_content() {
        let (_dir, archive) = make_canonical_tar(TC::Gzip, "tar.gz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "file.txt")
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive], &node, &dest, None, None)
            .unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"file.txt\n");
    }

    #[test]
    fn tar_gz_fixture_extract_nested_file_produces_exact_content() {
        let (_dir, archive) = make_canonical_tar(TC::Gzip, "tar.gz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt")
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive], &node, &dest, None, None)
            .unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"another_file.txt\n");
    }

    #[test]
    fn tar_bz2_fixture_parse_and_extract_round_trip() {
        let (_dir, archive) = make_canonical_tar(TC::Bzip2, "tar.bz2");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive.clone()], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 2);
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt")
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive], &node, &dest, None, None)
            .unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"another_file.txt\n");
    }

    #[test]
    fn tar_xz_fixture_parse_and_extract_round_trip() {
        let (_dir, archive) = make_canonical_tar(TC::Xz, "tar.xz");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive.clone()], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 2);
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "file.txt")
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive], &node, &dest, None, None)
            .unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"file.txt\n");
    }

    #[test]
    fn plain_tar_fixture_parse_and_extract_round_trip() {
        let (_dir, archive) = make_canonical_tar(TC::None, "tar");
        let vfs = BackendLibarchive
            .parse_upfront(&[archive.clone()], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 2);
        let node = vfs
            .entries
            .iter()
            .find(|n| n.path == "file.txt")
            .unwrap()
            .clone();
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive], &node, &dest, None, None)
            .unwrap();
        assert_eq!(std::fs::read(dest).unwrap(), b"file.txt\n");
    }
}
