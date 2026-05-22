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
mod native_engine;
mod upload;
mod project;
mod computer_use;
mod ask_user;
mod document;
mod sandbox;
mod github;
mod db;
mod multiagent;
mod orchestration;
mod permissions;

use bridge::BridgeServer;
use native_engine::engine_core::NativeEngine;
use native_engine::provider_manager::ProviderManager;
use mcp::McpServerManager;
use permissions::{AuditLogger, PermissionManager};
use tauri::Manager;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::sync::Mutex;

fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Arc::new(Mutex::new(None::<NativeEngine>)))
        .manage(Arc::new(Mutex::new(None::<Arc<Mutex<ProviderManager>>>)))
        .manage(Arc::new(claude_desktop_tauri_lib::db::DbManager::new(PathBuf::from(":memory:")).unwrap()))
        .manage(Arc::new(Mutex::new(None::<Arc<Mutex<McpServerManager>>>)))
        .setup(|app| {
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
            let bridge_ready = Arc::new(Notify::new());
            let bridge_ready_clone = bridge_ready.clone();

            let mcp_config_path = data_dir.join("mcp-servers.json");
            let mcp_manager = Arc::new(Mutex::new(McpServerManager::new(mcp_config_path)));
            
            {
                let mcp_manager_ref = mcp_manager.clone();
                tauri::async_runtime::block_on(async move {
                    let manager = mcp_manager_ref.lock().await;
                    if let Err(e) = manager.initialize().await {
                        eprintln!("[MCP] Failed to initialize: {}", e);
                    } else {
                        println!("[MCP] Initialized successfully");
                    }
                });
            }
            
            {
                let app_handle = app.handle().clone();
                let mcp_manager_clone = mcp_manager.clone();
                tauri::async_runtime::block_on(async move {
                    *app_handle.state::<Arc<Mutex<Option<Arc<Mutex<McpServerManager>>>>>>().lock().await = Some(mcp_manager_clone);
                });
            }

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
            commands::native_engine_init,
            commands::native_chat,
            commands::native_create_conversation,
            commands::native_list_conversations,
            commands::native_delete_conversation,
            commands::native_get_messages,
            commands::native_list_providers,
            commands::native_update_provider,
            commands::native_delete_provider,
            commands::mcp_list_servers,
            commands::mcp_start_server,
            commands::mcp_stop_server,
            commands::mcp_restart_server,
            commands::mcp_add_server,
            commands::mcp_update_server,
            commands::mcp_remove_server,
            commands::mcp_toggle_server,
            commands::mcp_list_tools,
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
