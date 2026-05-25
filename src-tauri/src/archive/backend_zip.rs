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
