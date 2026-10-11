use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadWindowRequest {
    initial_url: String,
    initial_urls: Vec<String>,
    bulk: bool,
    generation: u64,
    requires_confirmation: bool,
}
#[derive(Default)]
pub struct DownloadWindowState(Mutex<WindowState>);
#[derive(Default)]
struct WindowState {
    ready: bool,
    requested: bool,
    request: DownloadWindowRequest,
}

fn prepare(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("add-download").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        "add-download",
        WebviewUrl::App("index.html?downloadWindow=1".into()),
    )
    .title("New download — Circle")
    .inner_size(600., 520.)
    .min_inner_size(440., 420.)
    .resizable(true)
    .visible(false)
    .center()
    .build()
    .map_err(|_| "Could not prepare the download window.".into())
    .map(|_| ())
}
fn show(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("add-download")
        .ok_or("Download window is unavailable.")?;
    window
        .show()
        .map_err(|_| "Could not show the download window.")?;
    window
        .unminimize()
        .map_err(|_| "Could not restore the download window.")?;
    window
        .set_focus()
        .map_err(|_| "Could not focus the download window.".into())
}

#[tauri::command]
pub async fn prepare_download_window(
    app: AppHandle,
    state: State<'_, DownloadWindowState>,
) -> Result<(), String> {
    let _state = state
        .0
        .lock()
        .map_err(|_| "Download window state is unavailable.")?;
    prepare(&app)
}

#[tauri::command]
pub async fn open_download_window(
    app: AppHandle,
    state: State<'_, DownloadWindowState>,
    initial_url: Option<String>,
    bulk: Option<bool>,
    initial_urls: Option<Vec<String>>,
) -> Result<(), String> {
    let url = initial_url.unwrap_or_default();
    let urls = initial_urls.unwrap_or_default();
    if url.len() > 8192 || urls.len() > 200 || urls.iter().any(|url| url.len() > 8192) {
        return Err("Paste up to 200 links of at most 8,192 bytes each.".into());
    }
    let mut state = state
        .0
        .lock()
        .map_err(|_| "Download window state is unavailable.")?;
    prepare(&app)?;
    // Explicit clipboard input is delivered without silently replacing a visible draft.
    if !state.requested || !url.is_empty() || !urls.is_empty() {
        state.request = DownloadWindowRequest {
            initial_url: url,
            initial_urls: urls,
            bulk: bulk.unwrap_or(false),
            generation: state.request.generation + 1,
            requires_confirmation: state.requested,
        };
        state.requested = true;
        if state.ready {
            app.get_webview_window("add-download")
                .ok_or("Download window is unavailable.")?
                .emit("download-window-request", &state.request)
                .map_err(|_| "Could not initialize the download window.")?;
        }
    }
    if state.ready {
        show(&app)?;
    }
    Ok(())
}

// Called after the form mounts and its request listener is installed. Cold opens
// stay hidden until then; warm opens only need show/focus, with no new webview.
#[tauri::command]
pub fn download_window_ready(
    app: AppHandle,
    state: State<'_, DownloadWindowState>,
) -> Result<DownloadWindowRequest, String> {
    let mut state = state
        .0
        .lock()
        .map_err(|_| "Download window state is unavailable.")?;
    state.ready = true;
    if state.requested {
        show(&app)?;
    }
    Ok(state.request.clone())
}

#[tauri::command]
pub fn close_download_window(
    app: AppHandle,
    state: State<'_, DownloadWindowState>,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("add-download") {
        window
            .hide()
            .map_err(|_| "Could not close the download window.")?;
    }
    state
        .0
        .lock()
        .map_err(|_| "Download window state is unavailable.")?
        .requested = false;
    Ok(())
}
