use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Semaphore;

use crate::archive::get_backend;
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};
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

#[tauri::command]
#[allow(dead_code)]
pub async fn extract_nodes(
    nodes: Vec<VfsNode>,
    dest_path: String,
    _state: State<'_, AppState>,
) -> Result<(), CheesyError> {
    // Limit to 8 concurrent file extractions
    let semaphore = Arc::new(Semaphore::new(8));
    let mut handles = vec![];

    // Create all directories first (synchronously)
    // TODO: directory creation logic here

    // Spawn bounded async tasks for files
    for _node in nodes.into_iter().filter(|n| !n.is_dir) {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let _dest = dest_path.clone();

        handles.push(tokio::spawn(async move {
            // TODO: stream extraction logic here
            drop(permit); // Release the slot for the next file
        }));
    }

    // Wait for all extractions to finish
    futures::future::join_all(handles).await;

    Ok(())
}
