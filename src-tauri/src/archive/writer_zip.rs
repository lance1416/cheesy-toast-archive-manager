use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::archive::writer::{ArchiveWriter, WriteOptions};
use crate::error::CheesyError;

pub struct ZipArchiveWriter;

impl ArchiveWriter for ZipArchiveWriter {
    fn create(
        &self,
        entries: &[(PathBuf, String)],
        dest: &Path,
        options: &WriteOptions,
    ) -> Result<(), CheesyError> {
        let file = File::create(dest).map_err(CheesyError::Io)?;
        let mut writer = ZipWriter::new(file);

        let mut file_opts =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        if let Some(level) = options.compression_level {
            file_opts = file_opts.compression_level(Some(i64::from(level)));
        }

        for (host_path, archive_path) in entries {
            writer
                .start_file(archive_path, file_opts)
                .map_err(CheesyError::Zip)?;
            let mut src = File::open(host_path).map_err(CheesyError::Io)?;
            io::copy(&mut src, &mut writer).map_err(CheesyError::Io)?;
        }

        writer.finish().map_err(CheesyError::Zip)?;
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

    // ── round-trip ───────────────────────────────────────────────────────────

    #[test]
    fn creates_valid_zip_readable_by_zip_crate() {
        let src_dir = tempfile::tempdir().unwrap();
        make_source_tree(src_dir.path());
        let entries =
            collect_entries(&[src_dir.path().join("root.txt"), src_dir.path().join("sub")])
                .unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let zip_path = out_dir.path().join("out.zip");
        ZipArchiveWriter
            .create(&entries, &zip_path, &WriteOptions::default())
            .unwrap();

        let f = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        assert_eq!(archive.len(), 2);

        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["root.txt", "sub/nested.txt"]);
    }

    #[test]
    fn extracted_content_matches_source() {
        let src_dir = tempfile::tempdir().unwrap();
        fs::write(src_dir.path().join("data.txt"), b"exact bytes").unwrap();
        let entries = collect_entries(&[src_dir.path().join("data.txt")]).unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let zip_path = out_dir.path().join("out.zip");
        ZipArchiveWriter
            .create(&entries, &zip_path, &WriteOptions::default())
            .unwrap();

        let f = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        let mut entry = archive.by_name("data.txt").unwrap();
        let mut buf = Vec::new();
        io::Read::read_to_end(&mut entry, &mut buf).unwrap();
        assert_eq!(buf, b"exact bytes");
    }

    #[test]
    fn explicit_compression_level_produces_deflated_archive() {
        let src_dir = tempfile::tempdir().unwrap();
        fs::write(src_dir.path().join("f.txt"), b"hello world hello world").unwrap();
        let entries = collect_entries(&[src_dir.path().join("f.txt")]).unwrap();

        let out_dir = tempfile::tempdir().unwrap();
        let zip_path = out_dir.path().join("leveled.zip");
        ZipArchiveWriter
            .create(
                &entries,
                &zip_path,
                &WriteOptions {
                    compression_level: Some(9),
                },
            )
            .unwrap();

        let f = File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(f).unwrap();
        let entry = archive.by_name("f.txt").unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
    }

    #[test]
    fn empty_entry_list_produces_valid_empty_zip() {
        let out_dir = tempfile::tempdir().unwrap();
        let zip_path = out_dir.path().join("empty.zip");
        ZipArchiveWriter
            .create(&[], &zip_path, &WriteOptions::default())
            .unwrap();

        let f = File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(f).unwrap();
        assert_eq!(archive.len(), 0);
    }
}
