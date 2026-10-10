use crate::AppState;
use crate::domain::{Settings, UpdateSettingsRequest};
use tauri::State;

fn map_err(err: crate::domain::AppError) -> String {
    err.into()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.manager.get_settings().map_err(map_err)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    input: UpdateSettingsRequest,
) -> Result<Settings, String> {
    state.manager.update_settings(input).map_err(map_err)
}
