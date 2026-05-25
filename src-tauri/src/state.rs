use crate::models::VirtualFileSystem;
use std::sync::Mutex;

#[derive(Default)]
#[allow(dead_code)]
pub struct AppState {
    // We wrap the VFS in an Option because when the app first launches,
    // no archive is opened yet.
    pub current_vfs: Mutex<Option<VirtualFileSystem>>,
}
