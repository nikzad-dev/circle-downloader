mod application;
mod commands;
mod domain;
mod infrastructure;
mod ipc;

use application::DownloadManager;
use infrastructure::db::Database;
use infrastructure::ownership::DataDirectoryLease;
use ipc::progress::ProgressHub;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};

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
    // Acquire before opening SQLite or reconciling files belonging to another process.
    let lease = DataDirectoryLease::acquire(&data_dir)?;
    let db = Database::open(data_dir.join("circle-downloader.db"))?;
    let downloads = default_download_dir(&data_dir)?;
    let legacy = data_dir.join("downloads");
    if downloads != legacy {
        db.preserve_legacy_downloads(&legacy)?;
    }
    let manager = DownloadManager::new(db, downloads, ProgressHub::default(), lease)?;
    #[cfg(not(test))]
    manager.prepare_category_folders()?;
    manager.reconcile_on_startup()?;
    Ok(AppState { manager })
}

fn default_download_dir(data_dir: &std::path::Path) -> Result<PathBuf, domain::AppError> {
    // Tests and explicitly isolated debug runs must never use real user downloads.
    #[cfg(test)]
    return Ok(data_dir.join("downloads"));
    #[cfg(not(test))]
    {
        #[cfg(debug_assertions)]
        if std::env::var_os("CIRCLE_DOWNLOADER_DATA_DIR").is_some() {
            let root = data_dir.join("user-downloads");
            std::fs::create_dir_all(&root).map_err(infrastructure::errors::map_io_err)?;
            return Ok(root.join("CIRCLE"));
        }
        let _ = data_dir;
        let root = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
            .ok_or_else(|| {
                domain::AppError::new(
                    domain::ErrorCode::StorageNotFound,
                    "Your Downloads folder could not be located.",
                )
            })?;
        std::fs::create_dir_all(&root).map_err(infrastructure::errors::map_io_err)?;
        Ok(root.join("CIRCLE"))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = default_data_dir();
    let state = match build_state(data_dir) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("Circle Downloader could not start: {}", error.message);
            std::process::exit(1);
        }
    };

    let allow_exit = std::sync::Arc::new(AtomicBool::new(false));
    let allow_exit_setup = allow_exit.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(commands::download_window::DownloadWindowState::default())
        .setup(move |app| {
            let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("missing window icon")?;
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .tooltip("Circle Downloader")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;
            let _ = allow_exit_setup;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "add-download"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let state = window.state::<commands::download_window::DownloadWindowState>();
                let _ = commands::download_window::close_download_window(
                    window.app_handle().clone(),
                    state,
                );
            }
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                // Hide-to-tray: closing the window must not kill transfers.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::health_check,
            commands::download_window::open_download_window,
            commands::download_window::prepare_download_window,
            commands::download_window::download_window_ready,
            commands::download_window::close_download_window,
            commands::advanced::create_configured_download,
            commands::advanced::get_task_options,
            commands::advanced::update_task_options,
            commands::advanced::get_queue_policies,
            commands::advanced::save_queue_policy,
            commands::advanced::get_network_settings,
            commands::advanced::save_network_settings,
            commands::advanced::list_credential_grants,
            commands::advanced::save_credential,
            commands::advanced::revoke_credential,
            commands::advanced::get_statistics,
            commands::advanced::bulk_import,
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
            commands::destinations::list_destinations,
            commands::destinations::grant_destination,
            commands::destinations::remove_destination,
            commands::downloads::open_download,
            commands::downloads::reveal_download,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Circle Downloader")
        .run(move |app, event| match &event {
            RunEvent::ExitRequested { api, .. } => {
                if allow_exit.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                let handle = app.clone();
                let allow_exit = allow_exit.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    state.manager.shutdown().await;
                    allow_exit.store(true, Ordering::SeqCst);
                    handle.exit(0);
                });
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen {
                has_visible_windows,
                ..
            } => {
                if !*has_visible_windows && let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod api_tests;

#[cfg(test)]
mod contract_tests;

#[cfg(test)]
mod ownership_tests;
