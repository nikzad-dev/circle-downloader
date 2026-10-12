use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct ProgressWindowState(Mutex<WindowState>);
#[derive(Default)]
struct WindowState {
    ready: bool,
    visible: bool,
}

fn show(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("download-progress")
        .ok_or("Progress window is unavailable.")?;
    window
        .show()
        .map_err(|_| "Could not show download progress.")?;
    window
        .unminimize()
        .map_err(|_| "Could not restore download progress.")?;
    window
        .set_focus()
        .map_err(|_| "Could not focus download progress.".into())
}

#[tauri::command]
pub async fn open_progress_window(
    app: AppHandle,
    state: State<'_, ProgressWindowState>,
) -> Result<(), String> {
    let mut state = state
        .0
        .lock()
        .map_err(|_| "Progress window state is unavailable.")?;
    if app.get_webview_window("download-progress").is_none() {
        WebviewWindowBuilder::new(
            &app,
            "download-progress",
            WebviewUrl::App("index.html?progressWindow=1".into()),
        )
        .title("Download progress — Circle")
        .inner_size(680., 720.)
        .min_inner_size(440., 460.)
        .resizable(true)
        .visible(false)
        .center()
        .build()
        .map_err(|_| "Could not open download progress.")?;
        state.ready = false;
    }
    state.visible = true;
    if state.ready {
        app.get_webview_window("download-progress")
            .ok_or("Progress window is unavailable.")?
            .emit("progress-window-visibility", true)
            .map_err(|_| "Could not update download progress.")?;
        show(&app)?;
    }
    Ok(())
}

#[tauri::command]
pub fn progress_window_ready(
    app: AppHandle,
    state: State<'_, ProgressWindowState>,
) -> Result<bool, String> {
    let mut state = state
        .0
        .lock()
        .map_err(|_| "Progress window state is unavailable.")?;
    state.ready = true;
    if state.visible {
        show(&app)?;
    }
    Ok(state.visible)
}

#[tauri::command]
pub fn close_progress_window(
    app: AppHandle,
    state: State<'_, ProgressWindowState>,
) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|_| "Progress window state is unavailable.")?
        .visible = false;
    if let Some(window) = app.get_webview_window("download-progress") {
        window
            .hide()
            .map_err(|_| "Could not close download progress.")?;
        window
            .emit("progress-window-visibility", false)
            .map_err(|_| "Could not stop progress updates.")?;
    }
    Ok(())
}
