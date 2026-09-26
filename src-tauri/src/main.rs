//! Rocktier OCR — make scanned PDFs searchable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod menu;
mod pdf;

use tauri::Emitter;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ocr_process,
            commands::ocr_export_pdf,
            commands::ocr_export_txt,
            commands::ocr_cancel,
            menu::build_menu,
            menu::open_url,
        ])
        // 家族菜单规范：自定义项 → 前端 menu-action 事件（复用现有动作链，
        // 禁用态与运行中守卫都在前端的按钮逻辑里）。OCR 没有未保存内容，
        // 退出不走关窗守卫，直接 app.exit(0)。
        .on_menu_event(|app, event| {
            if event.id().0.as_str() == "quit" {
                app.exit(0);
                return;
            }
            let _ = app.emit("menu-action", event.id().0.as_str());
        })
        .run(tauri::generate_context!())
        .expect("error while running Rocktier OCR");
}
