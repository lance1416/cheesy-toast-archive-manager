use std::path::{Path, PathBuf};

use libarchive2::{ArchiveFormat, CompressionFormat, WriteArchive};

use crate::archive::writer::{ArchiveWriter, WriteOptions};
use crate::error::CheesyError;

#[derive(Debug, Clone, Copy)]
pub enum TarCompression {
    None,
    Gzip,
    Bzip2,
    Xz,
}

pub struct TarArchiveWriter {
    pub compression: TarCompression,
}

impl ArchiveWriter for TarArchiveWriter {
    fn create(
        &self,
        entries: &[(PathBuf, String)],
        dest: &Path,
        _options: &WriteOptions,
    ) -> Result<(), CheesyError> {
        let compression = match self.compression {
            TarCompression::None => CompressionFormat::None,
            TarCompression::Gzip => CompressionFormat::Gzip,
            TarCompression::Bzip2 => CompressionFormat::Bzip2,
            TarCompression::Xz => CompressionFormat::Xz,
        };

        let mut writer = WriteArchive::new()
            .format(ArchiveFormat::TarGnu)
            .compression(compression)
            .open_file(dest)
            .map_err(|e| CheesyError::Parse(e.to_string()))?;

        for (host_path, archive_path) in entries {
            let data = std::fs::read(host_path).map_err(CheesyError::Io)?;
            writer
                .add_file(archive_path, &data)
                .map_err(|e| CheesyError::Parse(e.to_string()))?;
        }

        writer
            .finish()
            .map_err(|e| CheesyError::Parse(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::writer::collect_entries;
    use std::fs;

    fn make_source_tree(dir: &Path) {
        fs::write(dir.join("root.txt"), b"root content").unwrap();
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.txt"), b"nested content").unwrap();
    }

    fn round_trip(compression: TarCompression, ext: &str) {
        let src_dir = tempfile::tempdir().unwrap();
        make_source_tree(src_dir.path());
        let entries =
            collect_entries(&[src_dir.path().join("root.txt"), src_dir.path().join("sub")])
                .unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join(format!("out.{ext}"));
        TarArchiveWriter { compression }
            .create(&entries, &archive_path, &WriteOptions::default())
            .unwrap();

        // Verify by round-tripping through BackendLibarchive
        use crate::archive::backend_libarchive::BackendLibarchive;
        use crate::archive::ArchiveBackend;

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path], None, None, None)
            .unwrap();

        let mut names: Vec<&str> = vfs.entries.iter().map(|n| n.path.as_str()).collect();
        names.sort();
        assert_eq!(names, ["root.txt", "sub/nested.txt"]);
    }

    fn content_round_trip(compression: TarCompression, ext: &str) {
        let src_dir = tempfile::tempdir().unwrap();
        fs::write(src_dir.path().join("data.txt"), b"tar content check").unwrap();
        let entries = collect_entries(&[src_dir.path().join("data.txt")]).unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join(format!("out.{ext}"));
        TarArchiveWriter { compression }
            .create(&entries, &archive_path, &WriteOptions::default())
            .unwrap();

        use crate::archive::backend_libarchive::BackendLibarchive;
        use crate::archive::ArchiveBackend;

        let vfs = BackendLibarchive
            .parse_upfront(&[archive_path.clone()], None, None, None)
            .unwrap();
        let node = vfs
            .entries
            .iter()
            .find(|n| n.name == "data.txt")
            .unwrap()
            .clone();

        let extract_dir = tempfile::tempdir().unwrap();
        let dest = extract_dir.path().join("out.txt");
        BackendLibarchive
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(fs::read(dest).unwrap(), b"tar content check");
    }

    // ── plain tar ────────────────────────────────────────────────────────────

    #[test]
    fn tar_creates_readable_archive() {
        round_trip(TarCompression::None, "tar");
    }

    #[test]
    fn tar_content_matches_source() {
        content_round_trip(TarCompression::None, "tar");
    }

    // ── tar.gz ───────────────────────────────────────────────────────────────

    #[test]
    fn tar_gz_creates_readable_archive() {
        round_trip(TarCompression::Gzip, "tar.gz");
    }

    #[test]
    fn tar_gz_content_matches_source() {
        content_round_trip(TarCompression::Gzip, "tar.gz");
    }

    // ── tar.bz2 ──────────────────────────────────────────────────────────────

    #[test]
    fn tar_bz2_creates_readable_archive() {
        round_trip(TarCompression::Bzip2, "tar.bz2");
    }

    #[test]
    fn tar_bz2_content_matches_source() {
        content_round_trip(TarCompression::Bzip2, "tar.bz2");
    }

    // ── tar.xz ───────────────────────────────────────────────────────────────

    #[test]
    fn tar_xz_creates_readable_archive() {
        round_trip(TarCompression::Xz, "tar.xz");
    }

    #[test]
    fn tar_xz_content_matches_source() {
        content_round_trip(TarCompression::Xz, "tar.xz");
    }
}
