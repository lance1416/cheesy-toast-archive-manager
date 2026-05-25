// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod archive;
mod encoding;

mod commands;
mod error;
mod models;
mod state;

fn main() {
    cheesy_toast_archive_manager_lib::run()
}
