use crate::{
    AppState,
    application::manager::advanced::{BulkImport, BulkResult},
    domain::advanced::*,
};
use tauri::State;
#[tauri::command]
pub fn get_task_options(state: State<'_, AppState>) -> Result<Vec<TaskOptions>, String> {
    state.manager.task_options().map_err(Into::into)
}
#[tauri::command]
pub fn update_task_options(
    state: State<'_, AppState>,
    input: TaskOptions,
) -> Result<TaskOptions, String> {
    state.manager.update_task_options(input).map_err(Into::into)
}
#[tauri::command]
pub fn get_queue_policies(state: State<'_, AppState>) -> Result<Vec<QueuePolicy>, String> {
    state.manager.queue_policies().map_err(Into::into)
}
#[tauri::command]
pub async fn save_queue_policy(
    state: State<'_, AppState>,
    input: QueuePolicy,
) -> Result<QueuePolicy, String> {
    state.manager.save_queue(input).await.map_err(Into::into)
}
#[tauri::command]
pub fn get_network_settings(state: State<'_, AppState>) -> Result<NetworkSettings, String> {
    state.manager.network_settings().map_err(Into::into)
}
#[tauri::command]
pub fn save_network_settings(
    state: State<'_, AppState>,
    input: NetworkSettings,
) -> Result<NetworkSettings, String> {
    state.manager.save_network(input).map_err(Into::into)
}
#[tauri::command]
pub fn list_credential_grants(state: State<'_, AppState>) -> Result<Vec<CredentialGrant>, String> {
    state.manager.grants().map_err(Into::into)
}
#[tauri::command]
pub fn save_credential(
    state: State<'_, AppState>,
    input: SaveCredential,
) -> Result<CredentialGrant, String> {
    state.manager.save_credential(input).map_err(Into::into)
}
#[tauri::command]
pub fn revoke_credential(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.manager.revoke_credential(&id).map_err(Into::into)
}
#[tauri::command]
pub fn get_statistics(state: State<'_, AppState>) -> Result<Statistics, String> {
    state.manager.statistics().map_err(Into::into)
}
#[tauri::command]
pub async fn bulk_import(
    state: State<'_, AppState>,
    input: BulkImport,
) -> Result<BulkResult, String> {
    state.manager.bulk_import(input).await.map_err(Into::into)
}

#[tauri::command]
pub async fn create_configured_download(
    state: State<'_, AppState>,
    input: crate::domain::CreateDownloadRequest,
    options: TaskOptions,
) -> Result<crate::domain::DownloadTask, String> {
    state
        .manager
        .create_configured(input, options)
        .await
        .map_err(Into::into)
}
