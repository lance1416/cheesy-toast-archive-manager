use serde::{Deserialize, Serialize};

/// Represents a single file or directory inside the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsNode {
    /// The full path of the node inside the archive (e.g., "folder/file.txt")
    pub path: String,
    /// The base name of the node (e.g., "file.txt")
    pub name: String,
    /// Uncompressed size of the file in bytes (0 for directories)
    pub size: u64,
    /// True if this node represents a directory
    pub is_dir: bool,
    /// The encoding used to decode this node's name (e.g., "UTF-8", "GBK")
    pub encoding_used: String,
}

/// The entire virtual file system parsed upfront.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualFileSystem {
    /// The absolute path to the archive file on the host OS
    pub archive_path: String,
    /// A flat list of all files and folders in the archive
    pub entries: Vec<VfsNode>,
    /// Total number of entries
    pub total_entries: usize,
}
