use crate::AppState;
use crate::domain::{Destination, GrantDestinationRequest};
use tauri::State;

fn map_err(err: crate::domain::AppError) -> String {
    err.into()
}

#[tauri::command]
pub fn list_destinations(state: State<'_, AppState>) -> Result<Vec<Destination>, String> {
    state.manager.list_destinations().map_err(map_err)
}

#[tauri::command]
pub fn grant_destination(
    state: State<'_, AppState>,
    input: GrantDestinationRequest,
) -> Result<Destination, String> {
    state.manager.grant_destination(input).map_err(map_err)
}

#[tauri::command]
pub fn remove_destination(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.remove_destination(&id).map_err(map_err)
}
