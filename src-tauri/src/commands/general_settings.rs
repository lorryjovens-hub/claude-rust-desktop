use serde_json::Value;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;

fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("general-settings.json"))
}

#[tauri::command]
pub fn read_general_settings(app: AppHandle) -> Result<Option<Value>, String> {
    let path = settings_path(&app)?;
    if !path.exists() { return Ok(None); }
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    if !value.is_object() { return Err("General settings must be a JSON object".into()); }
    Ok(Some(value))
}

#[tauri::command]
pub fn write_general_settings(app: AppHandle, settings: Value) -> Result<(), String> {
    if !settings.is_object() { return Err("General settings must be a JSON object".into()); }
    let path = settings_path(&app)?;
    let contents = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_general_settings(app: AppHandle) -> Result<(), String> {
    let path = settings_path(&app)?;
    #[allow(deprecated)]
    app.shell().open(path.to_string_lossy().to_string(), None).map_err(|e| e.to_string())
}
