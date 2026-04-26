#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod commands;
mod engine;
mod tools;
mod research;
mod prompt;
mod mcp;
mod streaming;
mod task;
mod skills;
mod git;
mod config;
mod fs;
mod terminal;
mod process;
mod watcher;
mod clipboard;
mod notification;
mod logger;
mod updater;
mod worktree;
mod ide;
mod analytics;
mod slash_commands;
mod cost_tracker;

use bridge::BridgeServer;
use tauri::Manager;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Notify;

fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
            let bridge_ready = Arc::new(Notify::new());
            let bridge_ready_clone = bridge_ready.clone();

            tauri::async_runtime::spawn(async move {
                let bridge = BridgeServer::new(data_dir);
                println!("[Bridge] Starting server on port 30080...");
                match bridge.start(30080).await {
                    Ok(()) => println!("[Bridge] Server stopped."),
                    Err(e) => eprintln!("[Bridge] Failed to start: {}", e),
                }
            });

            if let Some(window) = app.webview_windows().get("main") {
                let window = window.clone();
                let _ = window.open_devtools();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
                    let _ = window.show();
                    println!("[App] Window shown after bridge startup delay.");
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_platform,
            commands::select_directory,
            commands::show_item_in_folder,
            commands::open_folder,
            commands::open_external_url,
            commands::resize_window,
            commands::show_main_window,
            commands::export_workspace,
            commands::get_system_status,
            commands::chat_send,
            commands::chat_stream,
            commands::execute_tool,
            commands::get_app_path,
            commands::check_update,
            commands::install_update,
            commands::list_slash_commands,
            commands::search_slash_commands,
            commands::get_slash_command_categories,
            commands::get_cost_summary,
            commands::get_all_session_costs,
        ]);

    #[cfg(mobile)]
    {
        builder = builder
            .plugin(tauri_plugin_haptics::init())
            .plugin(tauri_plugin_barcode_scanner::init());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
