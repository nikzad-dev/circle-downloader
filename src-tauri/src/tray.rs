use crate::AppState;
use crate::commands::progress_window::{ProgressWindowState, open_progress_window};
use crate::domain::{DownloadStatus, ProgressEvent};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

#[derive(Default)]
struct ActiveTransfers {
    tasks: HashMap<String, (u64, bool)>,
    count: usize,
}

impl ActiveTransfers {
    fn update(&mut self, id: &str, revision: u64, status: DownloadStatus) -> bool {
        let previous = self.tasks.get(id).copied();
        if previous.is_some_and(|(current, _)| revision <= current) {
            return false;
        }
        let was_visible = self.count > 0;
        if previous.is_some_and(|(_, active)| active) {
            self.count -= 1;
        }
        if status.is_active() {
            self.count += 1;
        }
        self.tasks.insert(id.into(), (revision, status.is_active()));
        was_visible != (self.count > 0)
    }
}

pub fn install(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let progress = MenuItem::with_id(
        app,
        "download-progress",
        "Download progress",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("missing window icon")?;
    TrayIconBuilder::new()
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
            "download-progress" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<ProgressWindowState>();
                    if let Err(error) = open_progress_window(app.clone(), state).await {
                        eprintln!("Could not open tray download progress: {error}");
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    let transfers = Arc::new(Mutex::new(ActiveTransfers::default()));
    let handle = app.handle().clone();
    let event_transfers = transfers.clone();
    let event_menu = menu.clone();
    let event_progress = progress.clone();
    // Subscribe before the snapshot; revisions reject delayed, older events.
    // Native status tracking stays connected even when every window is hidden.
    app.state::<AppState>()
        .manager
        .progress_hub()
        .subscribe(Channel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body {
                let event: ProgressEvent = serde_json::from_str(&json)?;
                let transfers = event_transfers.clone();
                let menu = event_menu.clone();
                let progress = event_progress.clone();
                // Never touch the native menu or manager inside the hub's listener lock.
                handle.run_on_main_thread(move || {
                    let mut transfers = transfers.lock();
                    if transfers.update(&event.task_id, event.revision, event.status) {
                        let result = if transfers.count > 0 {
                            menu.insert(&progress, 1)
                        } else {
                            menu.remove(&progress)
                        };
                        if let Err(error) = result {
                            eprintln!("Could not update tray download progress: {error}");
                        }
                    }
                })?;
            }
            Ok(())
        }));
    let mut initial = transfers.lock();
    for task in app.state::<AppState>().manager.list()? {
        initial.update(&task.id, task.revision, task.status);
    }
    if initial.count > 0 {
        menu.insert(&progress, 1)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_progress_available_until_last_active_transfer_stops() {
        let mut state = ActiveTransfers::default();
        assert!(state.update("a", 1, DownloadStatus::Queued));
        assert!(!state.update("a", 2, DownloadStatus::Connecting));
        assert!(!state.update("b", 1, DownloadStatus::Downloading));
        assert!(!state.update("a", 3, DownloadStatus::Paused));
        assert!(!state.update("b", 2, DownloadStatus::Verifying));
        assert!(state.update("b", 3, DownloadStatus::Completed));
        assert_eq!(state.count, 0);
        assert!(state.update("a", 4, DownloadStatus::Downloading));
        assert!(state.update("a", 5, DownloadStatus::Canceled));
    }

    #[test]
    fn stale_events_cannot_restore_a_finished_transfer() {
        let mut state = ActiveTransfers::default();
        state.update("a", 8, DownloadStatus::Completed);
        assert!(!state.update("a", 7, DownloadStatus::Downloading));
        assert!(!state.update("a", 8, DownloadStatus::Downloading));
        assert_eq!(state.count, 0);
        assert!(state.update("a", 9, DownloadStatus::Queued));
        assert!(state.update("a", 10, DownloadStatus::Failed));
    }
}
