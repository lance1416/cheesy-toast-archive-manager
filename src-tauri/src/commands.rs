use std::path::PathBuf;
use tauri::State;

use crate::archive::get_backend;
use crate::error::CheesyError;
use crate::models::VirtualFileSystem;
use crate::state::AppState;

#[tauri::command]
#[allow(dead_code)]
pub async fn open_archive(
    path: String,
    fallback_encoding: Option<String>,
    state: State<'_, AppState>,
) -> Result<VirtualFileSystem, CheesyError> {
    let archive_path = PathBuf::from(&path);

    let backend = get_backend(&archive_path)?;

    let vfs = backend.parse_upfront(&archive_path, fallback_encoding.as_deref())?;

    let mut current_vfs_lock = state
        .current_vfs
        .lock()
        .map_err(|_| CheesyError::Parse("Failed to acquire application state lock".to_string()))?;

    *current_vfs_lock = Some(vfs.clone());

    Ok(vfs)
}
