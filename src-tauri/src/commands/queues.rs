use crate::AppState;
use crate::domain::Queue;
use tauri::State;

fn map_err(err: crate::domain::AppError) -> String {
    err.into()
}

#[tauri::command]
pub fn list_queues(state: State<'_, AppState>) -> Result<Vec<Queue>, String> {
    state.manager.list_queues().map_err(map_err)
}
