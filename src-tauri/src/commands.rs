use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Semaphore;

use crate::archive::get_backend;
use crate::archive::writer::{collect_entries, ArchiveWriter, WriteOptions};
use crate::archive::writer_zip::ZipArchiveWriter;
use crate::error::CheesyError;
use crate::models::{VfsNode, VirtualFileSystem};
use crate::state::AppState;

#[derive(Clone, serde::Serialize)]
pub struct CompressionProgress {
    pub total_files: usize,
    pub files_completed: usize,
    pub current_file_name: String,
}

#[derive(Clone, serde::Serialize)]
pub struct ExtractionProgress {
    pub total_files: usize,
    pub files_completed: usize,
    pub current_file_name: String,
}

#[tauri::command]
pub async fn open_archive(
    path: String,
    fallback_encoding: Option<String>,
    state: State<'_, AppState>,
) -> Result<VirtualFileSystem, CheesyError> {
    let archive_path = PathBuf::from(&path);

    let backend = get_backend(&archive_path)?;

    let vfs = backend.parse_upfront(&archive_path, fallback_encoding.as_deref(), None, None)?;

    let mut current_vfs_lock = state
        .current_vfs
        .lock()
        .map_err(|_| CheesyError::Parse("Failed to acquire application state lock".to_string()))?;

    *current_vfs_lock = Some(vfs.clone());

    Ok(vfs)
}

#[tauri::command]
pub async fn extract_nodes(
    archive_path_str: String,
    nodes: Vec<VfsNode>,
    dest_path_str: String,
    password: Option<String>,
    password_encoding: Option<String>,
    app_handle: AppHandle,
    _state: State<'_, AppState>,
) -> Result<(), CheesyError> {
    let archive_path = PathBuf::from(archive_path_str);
    let dest_dir = PathBuf::from(dest_path_str);

    let backend = get_backend(&archive_path)?;
    // We wrap the backend in an Arc so we can share it safely across Tokios threads
    let shared_backend = Arc::new(backend);

    // Filter out directories (we create them synchronously first, or just let the file extractor do it)
    let files_to_extract: Vec<VfsNode> = nodes.into_iter().filter(|n| !n.is_dir).collect();
    let total_files = files_to_extract.len();

    // Thread-safe progress counter
    let completed_counter = Arc::new(Mutex::new(0));

    // Bounded concurrency: max 8 files extracting simultaneously to prevent disk thrashing
    let semaphore = Arc::new(Semaphore::new(8));
    let mut handles = vec![];

    let password_arc = password.map(Arc::new);
    let password_encoding_arc = password_encoding.map(Arc::new);

    for node in files_to_extract {
        let permit = semaphore.clone().acquire_owned().await.unwrap();

        let backend_clone = Arc::clone(&shared_backend);
        let archive_path_clone = archive_path.clone();
        let dest_file_path = dest_dir.join(&node.path);
        let app_handle_clone = app_handle.clone();
        let counter_clone = Arc::clone(&completed_counter);
        let pass_clone = password_arc.clone();
        let pass_enc_clone = password_encoding_arc.clone();

        handles.push(tokio::spawn(async move {
            // 1. Extract the file (Streaming + Decryption happens here)
            let pass_deref = pass_clone.as_deref().map(|s| s.as_str());
            let pass_enc_deref = pass_enc_clone.as_deref().map(|s| s.as_str());

            let result = backend_clone.extract_node(
                &archive_path_clone,
                &node,
                &dest_file_path,
                pass_deref,
                pass_enc_deref,
            );

            // 2. Update progress
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            // 3. Emit progress event to Vue
            let _ = app_handle_clone.emit(
                "extraction-progress",
                ExtractionProgress {
                    total_files,
                    files_completed: *count,
                    current_file_name: node.name.clone(),
                },
            );

            drop(permit);
            result
        }));
    }

    // Await all background tasks and check for any internal errors
    for handle in futures::future::join_all(handles).await {
        // handle.unwrap() handles Tokio panics, the inner ? handles our CheesyError
        handle.map_err(|_| CheesyError::Parse("Tokio thread panicked".into()))??;
    }

    Ok(())
}

#[tauri::command]
pub async fn create_archive(
    source_paths: Vec<String>,
    dest_path: String,
    compression_level: Option<u8>,
    app_handle: AppHandle,
) -> Result<(), CheesyError> {
    let dest = PathBuf::from(dest_path);
    let sources: Vec<PathBuf> = source_paths.into_iter().map(PathBuf::from).collect();

    let entries = collect_entries(&sources)?;
    let total_files = entries.len();

    let _ = app_handle.emit(
        "compression-progress",
        CompressionProgress {
            total_files,
            files_completed: 0,
            current_file_name: String::new(),
        },
    );

    let options = WriteOptions { compression_level };

    // ZipWriter is not Send, so we run it on a dedicated blocking thread.
    tokio::task::spawn_blocking(move || ZipArchiveWriter.create(&entries, &dest, &options))
        .await
        .map_err(|_| CheesyError::Parse("Compression thread panicked".into()))??;

    let _ = app_handle.emit(
        "compression-progress",
        CompressionProgress {
            total_files,
            files_completed: total_files,
            current_file_name: String::new(),
        },
    );

    Ok(())
}
