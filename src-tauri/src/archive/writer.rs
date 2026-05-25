use std::path::{Path, PathBuf};

use crate::error::CheesyError;

#[derive(Default)]
pub struct WriteOptions {
    /// Deflate compression level (1–9). None means format default.
    pub compression_level: Option<u8>,
}

pub trait ArchiveWriter: Send + Sync {
    /// Creates a new archive at `dest` from the given `(host_path, archive_path)` pairs.
    ///
    /// `host_path` — absolute path to the source file on disk.
    /// `archive_path` — the entry name stored inside the archive (e.g. `"docs/report.pdf"`).
    fn create(
        &self,
        entries: &[(PathBuf, String)],
        dest: &Path,
        options: &WriteOptions,
    ) -> Result<(), CheesyError>;
}

/// Walks `source_paths` and returns a flat `(host_path, archive_path)` list.
///
/// For a plain file `/a/b/c.txt`, the archive path is `c.txt`.
/// For a directory `/a/b/dir/`, the archive paths are `dir/file`, `dir/sub/file`, …
pub fn collect_entries(source_paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>, CheesyError> {
    let mut entries = Vec::new();
    for source in source_paths {
        if source.is_file() {
            let name = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            entries.push((source.clone(), name));
        } else if source.is_dir() {
            let base_name = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            walk_dir(source, &base_name, &mut entries)?;
        }
    }
    Ok(entries)
}

fn walk_dir(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String)>) -> Result<(), CheesyError> {
    for entry in std::fs::read_dir(dir).map_err(CheesyError::Io)? {
        let entry = entry.map_err(CheesyError::Io)?;
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let archive_path = format!("{}/{}", prefix, name);

        if path.is_file() {
            out.push((path, archive_path));
        } else if path.is_dir() {
            walk_dir(&path, &archive_path, out)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_entries_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hello.txt");
        fs::write(&file, b"hi").unwrap();

        let entries = collect_entries(&[file.clone()]).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, file);
        assert_eq!(entries[0].1, "hello.txt");
    }

    #[test]
    fn collect_entries_directory_recurses() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("mydir");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        let sub = root.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.txt"), b"b").unwrap();

        let mut entries = collect_entries(&[root]).unwrap();
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        let names: Vec<&str> = entries.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["mydir/a.txt", "mydir/sub/b.txt"]);
    }

    #[test]
    fn collect_entries_mixed_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("top.txt");
        fs::write(&file, b"t").unwrap();
        let subdir = dir.path().join("pack");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("inner.txt"), b"i").unwrap();

        let mut entries = collect_entries(&[file, subdir]).unwrap();
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        let names: Vec<&str> = entries.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["pack/inner.txt", "top.txt"]);
    }
}
