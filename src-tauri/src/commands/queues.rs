use crate::domain::Queue;
use crate::AppState;
use tauri::State;

fn map_err(err: crate::domain::AppError) -> String {
    err.into()
}

#[tauri::command]
pub fn list_queues(state: State<'_, AppState>) -> Result<Vec<Queue>, String> {
    state.manager.list_queues().map_err(map_err)
}
