use serde::Serialize;

/// Placeholder data model. Replace list_downloads with an actual service/database read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[tauri::command]
pub fn list_downloads() -> Vec<DownloadTask> {
    // Setup test only: downloading/persistence is deliberately not implemented.
    Vec::new()
}
