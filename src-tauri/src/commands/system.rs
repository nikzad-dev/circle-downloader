#[tauri::command]
pub fn health_check() -> &'static str {
    "ok"
}
