use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

use crate::archive::ArchiveBackend;
use crate::encoding::decode_bytes;
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};

pub struct BackendZip;

impl ArchiveBackend for BackendZip {
    fn parse_upfront(
        &self,
        path: &Path,
        fallback_encoding: Option<&str>,
    ) -> Result<VirtualFileSystem, CheesyError> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let mut entries = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let file_in_zip = archive.by_index_raw(i)?;

            // THE FIX: Do not use file_in_zip.name()!
            // We intercept the raw bytes directly from the ZIP header.
            let raw_name_bytes = file_in_zip.name_raw();

            // Pass the raw bytes to our Secret Sauce detector
            let (decoded_path, encoding_used) = decode_bytes(raw_name_bytes, fallback_encoding);

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
        _archive_path: &Path,
        _node: &VfsNode,
        _dest: &Path,
        _password: Option<&str>,
    ) -> Result<(), CheesyError> {
        let file = File::open(_archive_path)?;
        let mut archive = ZipArchive::new(file)?;

        // Find the specific file in the zip
        let mut zipped_file = archive.by_name(&_node.path)?;

        // Create the destination file on the OS
        let mut output_file = File::create(_dest)?;

        std::io::copy(&mut zipped_file, &mut output_file)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // Helper to resolve the test file path
    fn get_test_zip() -> PathBuf {
        let path = PathBuf::from("data/file.zip");
        assert!(
            path.exists(),
            "Test archive not found at {:?}",
            path.canonicalize()
        );
        path
    }

    #[test]
    fn test_zip_parse_upfront() {
        let backend = BackendZip;
        let archive_path = get_test_zip();

        let vfs = backend
            .parse_upfront(&archive_path, None)
            .expect("Failed to parse ZIP");

        // We expect at least 2 files (file.txt and another_file.txt).
        assert!(
            vfs.total_entries >= 2,
            "VFS should contain at least 2 entries"
        );

        // Verify the root file was parsed correctly
        let root_file = vfs
            .entries
            .iter()
            .find(|n| n.name == "file.txt" && !n.is_dir);
        assert!(root_file.is_some(), "file.txt is missing from the VFS root");

        // Verify the nested file was parsed correctly (checking the full internal path)
        let nested_file = vfs
            .entries
            .iter()
            .find(|n| n.path == "folder/another_file.txt" && !n.is_dir);
        assert!(
            nested_file.is_some(),
            "folder/another_file.txt is missing from the VFS"
        );
    }

    #[test]
    fn test_zip_extract_node() {
        let backend = BackendZip;
        let archive_path = get_test_zip();

        // Get the VFS so we have a valid node object to pass to the extractor
        let vfs = backend
            .parse_upfront(&archive_path, None)
            .expect("Failed to parse ZIP");
        let node_to_extract = vfs
            .entries
            .into_iter()
            .find(|n| n.name == "file.txt" && !n.is_dir)
            .expect("Could not find file.txt in test archive");

        // Create a secure temporary directory that auto-deletes when the test finishes
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let dest_path = temp_dir.path().join("file_extracted.txt");

        // Fire the extraction engine
        backend
            .extract_node(&archive_path, &node_to_extract, &dest_path, None)
            .expect("Extraction failed");

        // Verify the file exists on the physical disk
        assert!(dest_path.exists(), "Extracted file does not exist on disk");

        // Verify the extracted file has actual content
        let metadata = fs::metadata(&dest_path).unwrap();
        assert!(
            metadata.len() > 0,
            "Extracted file is completely empty (0 bytes)"
        );
    }
}
