use crate::AppState;
use crate::application::DownloadManager;
use crate::domain::{
    CreateDownloadRequest, DownloadTask, ProgressEvent, RemoveDownloadRequest, RemoveDownloadResult,
};
use tauri::State;
use tauri::ipc::Channel;

fn map_err(err: crate::domain::AppError) -> String {
    err.into()
}

#[tauri::command]
pub fn list_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadTask>, String> {
    state.manager.list().map_err(map_err)
}

#[tauri::command]
pub fn get_download(state: State<'_, AppState>, id: String) -> Result<DownloadTask, String> {
    state.manager.get(&id).map_err(map_err)
}

#[tauri::command]
pub fn create_download(
    state: State<'_, AppState>,
    input: CreateDownloadRequest,
) -> Result<DownloadTask, String> {
    state.manager.create(input).map_err(map_err)
}

#[tauri::command]
pub async fn pause_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadTask, String> {
    state.manager.pause(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadTask, String> {
    state.manager.resume(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadTask, String> {
    state.manager.cancel(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn retry_download(
    state: State<'_, AppState>,
    id: String,
) -> Result<DownloadTask, String> {
    state.manager.retry(&id).await.map_err(map_err)
}

#[tauri::command]
pub async fn remove_download(
    state: State<'_, AppState>,
    input: RemoveDownloadRequest,
) -> Result<RemoveDownloadResult, String> {
    state.manager.remove(input).await.map_err(map_err)
}

#[tauri::command]
pub fn subscribe_progress(
    state: State<'_, AppState>,
    on_event: Channel<ProgressEvent>,
) -> Result<u32, String> {
    Ok(state.manager.progress_hub().subscribe(on_event))
}

/// Test helper re-export surface — not a command.
#[allow(dead_code)]
pub fn manager_from_state(state: &AppState) -> &DownloadManager {
    &state.manager
}

#[tauri::command]
pub fn unsubscribe_progress(state: State<'_, AppState>, subscription_id: u32) {
    state.manager.progress_hub().unsubscribe(subscription_id);
}

#[tauri::command]
pub fn open_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.open_download(&id).map_err(map_err)
}

#[tauri::command]
pub fn reveal_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.reveal_download(&id).map_err(map_err)
}
