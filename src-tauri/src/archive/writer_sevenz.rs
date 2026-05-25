use std::fs::File;
use std::path::{Path, PathBuf};

use sevenz_rust2::{ArchiveEntry, ArchiveWriter as SevenZWriter};

use crate::archive::writer::{ArchiveWriter, WriteOptions};
use crate::error::CheesyError;

pub struct SevenZArchiveWriter;

impl ArchiveWriter for SevenZArchiveWriter {
    fn create(
        &self,
        entries: &[(PathBuf, String)],
        dest: &Path,
        _options: &WriteOptions,
    ) -> Result<(), CheesyError> {
        let mut writer =
            SevenZWriter::create(dest).map_err(|e| CheesyError::Parse(e.to_string()))?;

        for (host_path, archive_path) in entries {
            let entry = ArchiveEntry::from_path(host_path, archive_path.clone());
            let src = File::open(host_path).map_err(CheesyError::Io)?;
            writer
                .push_archive_entry(entry, Some(src))
                .map_err(|e| CheesyError::Parse(e.to_string()))?;
        }

        writer.finish().map_err(CheesyError::Io)?;
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

    // ── round-trip via BackendSevenZ ─────────────────────────────────────────

    #[test]
    fn creates_readable_7z_archive() {
        let src_dir = tempfile::tempdir().unwrap();
        make_source_tree(src_dir.path());
        let entries =
            collect_entries(&[src_dir.path().join("root.txt"), src_dir.path().join("sub")])
                .unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join("out.7z");
        SevenZArchiveWriter
            .create(&entries, &archive_path, &WriteOptions::default())
            .unwrap();

        // Verify by round-tripping through BackendSevenZ
        use crate::archive::backend_sevenz::BackendSevenZ;
        use crate::archive::ArchiveBackend;

        let vfs = BackendSevenZ
            .parse_upfront(&[archive_path], None, None, None)
            .unwrap();

        let mut names: Vec<&str> = vfs.entries.iter().map(|n| n.path.as_str()).collect();
        names.sort();
        assert_eq!(names, ["root.txt", "sub/nested.txt"]);
    }

    #[test]
    fn extracted_content_matches_source() {
        let src_dir = tempfile::tempdir().unwrap();
        fs::write(src_dir.path().join("data.txt"), b"exact bytes 7z").unwrap();
        let entries = collect_entries(&[src_dir.path().join("data.txt")]).unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join("out.7z");
        SevenZArchiveWriter
            .create(&entries, &archive_path, &WriteOptions::default())
            .unwrap();

        use crate::archive::backend_sevenz::BackendSevenZ;
        use crate::archive::ArchiveBackend;

        let vfs = BackendSevenZ
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
        BackendSevenZ
            .extract_node(&[archive_path], &node, &dest, None, None)
            .unwrap();

        assert_eq!(fs::read(dest).unwrap(), b"exact bytes 7z");
    }

    #[test]
    fn empty_entry_list_produces_valid_archive() {
        let out_dir = tempfile::tempdir().unwrap();
        let archive_path = out_dir.path().join("empty.7z");
        SevenZArchiveWriter
            .create(&[], &archive_path, &WriteOptions::default())
            .unwrap();

        use crate::archive::backend_sevenz::BackendSevenZ;
        use crate::archive::ArchiveBackend;

        let vfs = BackendSevenZ
            .parse_upfront(&[archive_path], None, None, None)
            .unwrap();
        assert_eq!(vfs.total_entries, 0);
    }
}
