mod application;
mod commands;
mod domain;
mod infrastructure;
mod ipc;

use application::DownloadManager;
use infrastructure::db::Database;
use ipc::progress::ProgressHub;
use std::path::PathBuf;

pub struct AppState {
    pub manager: DownloadManager,
}

fn default_data_dir() -> PathBuf {
    // Isolate native development/QA runs without touching a user's downloads.
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("CIRCLE_DOWNLOADER_DATA_DIR") {
        return PathBuf::from(path);
    }

    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("circle-downloader")
}

pub fn build_state(data_dir: PathBuf) -> Result<AppState, domain::AppError> {
    std::fs::create_dir_all(&data_dir).map_err(|e| {
        domain::AppError::new(
            domain::ErrorCode::PathNotAllowed,
            format!("Cannot create data dir: {e}"),
        )
    })?;
    let db = Database::open(data_dir.join("circle-downloader.db"))?;
    let downloads = data_dir.join("downloads");
    let manager = DownloadManager::new(db, downloads, ProgressHub::default())?;
    manager.reconcile_on_startup()?;
    Ok(AppState { manager })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = default_data_dir();
    let state = build_state(data_dir).expect("failed to initialize application state");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system::health_check,
            commands::downloads::list_downloads,
            commands::downloads::get_download,
            commands::downloads::create_download,
            commands::downloads::pause_download,
            commands::downloads::resume_download,
            commands::downloads::cancel_download,
            commands::downloads::retry_download,
            commands::downloads::remove_download,
            commands::downloads::subscribe_progress,
            commands::downloads::unsubscribe_progress,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::queues::list_queues,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Circle Downloader");
}

#[cfg(test)]
mod api_tests;
