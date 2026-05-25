pub mod archive;
pub mod encoding;

pub mod commands;
pub mod error;
pub mod models;
pub mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::open_archive,
            commands::extract_nodes,
            commands::create_archive,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
