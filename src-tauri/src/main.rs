//! Rocktier OCR — make scanned PDFs searchable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod pdf;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ocr_process,
            commands::extract_text,
            commands::ocr_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rocktier OCR");
}
