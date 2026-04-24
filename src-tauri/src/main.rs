#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod commands;
mod engine;
mod mcp;
mod skills;
mod tools;
mod remote;

#[cfg(target_os = "android")]
mod android_jni;

use bridge::BridgeServer;
use tauri::Manager;

fn setup_app<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Start bridge server
            tauri::async_runtime::spawn(async move {
                let bridge = BridgeServer::new();
                if let Err(e) = bridge.start(30080).await {
                    eprintln!("[Bridge] Failed to start: {}", e);
                }
            });
            // Start remote WebSocket server for mobile communication
            tauri::async_runtime::spawn(async move {
                if let Err(e) = remote::start_remote_server(30081).await {
                    eprintln!("[Remote] Failed to start: {}", e);
                }
            });
            #[cfg(not(target_os = "android"))]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
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
            commands::export_workspace,
            commands::get_system_status,
            commands::chat_send,
            commands::chat_stream,
            commands::execute_tool,
            commands::get_app_path,
            commands::check_update,
            commands::install_update,
            commands::get_remote_connection_info,
        ])
}

#[cfg(mobile)]
fn setup_mobile<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_haptics::init())
        .plugin(tauri_plugin_barcode_scanner::init())
}

#[cfg(not(mobile))]
fn setup_mobile<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() {
    let builder = tauri::Builder::default();
    let builder = setup_app(builder);
    let builder = setup_mobile(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "android")]
fn main() {
    let builder = tauri::Builder::default();
    let builder = setup_app(builder);
    let builder = setup_mobile(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "ios")]
fn main() {
    let builder = tauri::Builder::default();
    let builder = setup_app(builder);
    let builder = setup_mobile(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
