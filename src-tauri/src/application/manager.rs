use crate::domain::filename::{filename_from_url, sanitize_filename};
use crate::domain::transitions::ensure_transition;
use crate::domain::url_policy::validate_download_url;
use crate::domain::{
    AppError, CreateDownloadRequest, DownloadStatus, DownloadTask, ErrorCode, ProgressEvent, Queue,
    RemoveDownloadRequest, RemoveDownloadResult, Settings, TaskError, UpdateSettingsRequest,
};
use crate::infrastructure::db::Database;
use crate::infrastructure::http::{cancel_pair, download_to_file};
use crate::ipc::progress::ProgressHub;
use chrono::Utc;
use parking_lot::Mutex;
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

struct WorkerHandle {
    cancel_tx: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct DownloadManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    db: Database,
    download_dir: PathBuf,
    http: Client,
    progress: ProgressHub,
    workers: Mutex<HashMap<String, WorkerHandle>>,
}

impl DownloadManager {
    pub fn new(
        db: Database,
        download_dir: PathBuf,
        progress: ProgressHub,
    ) -> Result<Self, AppError> {
        std::fs::create_dir_all(&download_dir).map_err(|e| {
            AppError::new(
                ErrorCode::PathNotAllowed,
                format!("Cannot create download dir: {e}"),
            )
        })?;
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| AppError::new(ErrorCode::Internal, e.to_string()))?;
        Ok(Self {
            inner: Arc::new(ManagerInner {
                db,
                download_dir,
                http,
                progress,
                workers: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn progress_hub(&self) -> ProgressHub {
        self.inner.progress.clone()
    }

    pub fn download_dir(&self) -> &Path {
        &self.inner.download_dir
    }

    pub fn reconcile_on_startup(&self) -> Result<(), AppError> {
        for mut task in self.inner.db.list_downloads()? {
            if matches!(
                task.status,
                DownloadStatus::Connecting
                    | DownloadStatus::Downloading
                    | DownloadStatus::Verifying
            ) {
                let partial_exists = task
                    .temp_path
                    .as_ref()
                    .map(|p| Path::new(p).exists())
                    .unwrap_or(false);
                task.status = DownloadStatus::Failed;
                task.error = Some(TaskError {
                    code: ErrorCode::Internal,
                    message: if partial_exists {
                        "Interrupted by application restart; retry to continue".into()
                    } else {
                        "Interrupted by application restart; partial file missing".into()
                    },
                    retryable: true,
                });
                task.speed_bytes_per_second = 0;
                task.bump_revision();
                self.inner.db.upsert_download(&task)?;
                self.emit_progress(&task);
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<DownloadTask>, AppError> {
        self.inner.db.list_downloads()
    }

    pub fn get(&self, id: &str) -> Result<DownloadTask, AppError> {
        self.inner.db.get_download(id)
    }

    pub fn create(&self, req: CreateDownloadRequest) -> Result<DownloadTask, AppError> {
        if let Some(request_id) = req.request_id.as_deref() {
            if let Some(existing) = self.inner.db.find_by_request_id(request_id)? {
                return Ok(existing);
            }
        }

        let url = validate_download_url(&req.url)?;
        let settings = self.inner.db.get_settings()?;
        let destination_id = req
            .destination_id
            .unwrap_or(settings.default_destination_id);
        let file_name = match req.suggested_name {
            Some(name) => sanitize_filename(&name)?,
            None => filename_from_url(url.as_str()),
        };

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let temp_path = self
            .inner
            .download_dir
            .join(format!(".{id}.{file_name}.part"))
            .to_string_lossy()
            .to_string();
        let final_path = unique_final_path(&self.inner.download_dir, &file_name)?;

        let start_now = req.start_now && settings.auto_start;
        let mut task = DownloadTask {
            id: id.clone(),
            url: url.to_string(),
            file_name,
            destination_id,
            status: if start_now {
                DownloadStatus::Queued
            } else {
                DownloadStatus::Paused
            },
            downloaded_bytes: 0,
            total_bytes: None,
            speed_bytes_per_second: 0,
            revision: 1,
            error: None,
            created_at: now,
            updated_at: now,
            temp_path: Some(temp_path),
            final_path: Some(final_path.to_string_lossy().to_string()),
            etag: None,
            last_modified: None,
            request_id: req.request_id,
            retry_count: 0,
        };
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);

        if start_now {
            self.spawn_worker(task.id.clone());
            task = self.inner.db.get_download(&id)?;
        }
        Ok(task)
    }

    pub fn pause(&self, id: &str) -> Result<DownloadTask, AppError> {
        let mut task = self.inner.db.get_download(id)?;
        ensure_transition(task.status, DownloadStatus::Paused)?;
        self.cancel_worker(id);
        // Wait briefly for worker to notice cancellation before persisting pause,
        // but don't require worker exit — status wins.
        task = self.inner.db.get_download(id)?;
        if task.status != DownloadStatus::Paused {
            ensure_transition(task.status, DownloadStatus::Paused)?;
            task.status = DownloadStatus::Paused;
            task.speed_bytes_per_second = 0;
            task.error = None;
            task.bump_revision();
            // Refresh on-disk bytes if temp exists
            if let Some(temp) = &task.temp_path {
                if let Ok(meta) = std::fs::metadata(temp) {
                    task.downloaded_bytes = meta.len();
                }
            }
            self.inner.db.upsert_download(&task)?;
            self.emit_progress(&task);
        }
        Ok(task)
    }

    pub fn resume(&self, id: &str) -> Result<DownloadTask, AppError> {
        let mut task = self.inner.db.get_download(id)?;
        ensure_transition(task.status, DownloadStatus::Queued)?;
        if let Some(temp) = &task.temp_path {
            if let Ok(meta) = std::fs::metadata(temp) {
                task.downloaded_bytes = meta.len();
            }
        }
        task.status = DownloadStatus::Queued;
        task.error = None;
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        self.spawn_worker(task.id.clone());
        self.inner.db.get_download(id)
    }

    pub fn cancel(&self, id: &str) -> Result<DownloadTask, AppError> {
        let mut task = self.inner.db.get_download(id)?;
        if task.status.is_terminal() {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                format!("Cannot cancel task in status {}", task.status.as_str()),
            ));
        }
        ensure_transition(task.status, DownloadStatus::Canceled)?;
        self.cancel_worker(id);
        task.status = DownloadStatus::Canceled;
        task.speed_bytes_per_second = 0;
        task.error = None;
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        Ok(task)
    }

    pub fn retry(&self, id: &str) -> Result<DownloadTask, AppError> {
        let mut task = self.inner.db.get_download(id)?;
        if task.status != DownloadStatus::Failed {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Only failed tasks can be retried",
            ));
        }
        ensure_transition(task.status, DownloadStatus::Queued)?;
        task.status = DownloadStatus::Queued;
        task.error = None;
        task.retry_count = task.retry_count.saturating_add(1);
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        self.spawn_worker(task.id.clone());
        self.inner.db.get_download(id)
    }

    pub fn remove(&self, req: RemoveDownloadRequest) -> Result<RemoveDownloadResult, AppError> {
        let task = self.inner.db.get_download(&req.id)?;
        if task.status.is_active() {
            self.cancel_worker(&req.id);
        }
        if req.delete_partial {
            if let Some(temp) = &task.temp_path {
                let _ = std::fs::remove_file(temp);
            }
        }
        self.inner.db.remove_download(&req.id)?;
        Ok(RemoveDownloadResult { removed: true })
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        self.inner.db.get_settings()
    }

    pub fn update_settings(&self, patch: UpdateSettingsRequest) -> Result<Settings, AppError> {
        self.inner.db.update_settings(patch)
    }

    pub fn list_queues(&self) -> Result<Vec<Queue>, AppError> {
        self.inner.db.list_queues()
    }

    fn cancel_worker(&self, id: &str) {
        if let Some(handle) = self.inner.workers.lock().remove(id) {
            let _ = handle.cancel_tx.send(true);
        }
    }

    fn spawn_worker(&self, task_id: String) {
        let inner = Arc::clone(&self.inner);
        let (cancel_tx, cancel_token) = cancel_pair();
        self.inner.workers.lock().insert(
            task_id.clone(),
            WorkerHandle {
                cancel_tx: cancel_tx.clone(),
            },
        );

        tauri::async_runtime::spawn(async move {
            run_download_job(inner, task_id, cancel_token).await;
        });
    }

    fn emit_progress(&self, task: &DownloadTask) {
        self.inner.progress.emit(ProgressEvent {
            task_id: task.id.clone(),
            revision: task.revision,
            transferred_bytes: task.downloaded_bytes,
            total_bytes: task.total_bytes,
            bytes_per_second: task.speed_bytes_per_second,
            status: task.status,
            timestamp: Utc::now(),
        });
    }
}

async fn run_download_job(
    inner: Arc<ManagerInner>,
    task_id: String,
    cancel: crate::infrastructure::http::CancelToken,
) {
    let result = execute_download(&inner, &task_id, &cancel).await;
    inner.workers.lock().remove(&task_id);

    match result {
        Ok(()) => {}
        Err(err) => {
            // Cancellation during pause/cancel is expected; don't overwrite terminal statuses.
            if let Ok(mut task) = inner.db.get_download(&task_id) {
                if matches!(
                    task.status,
                    DownloadStatus::Canceled | DownloadStatus::Paused | DownloadStatus::Completed
                ) {
                    return;
                }
                if err.code == ErrorCode::InvalidTransition
                    && err.message.to_lowercase().contains("canceled")
                {
                    // Pause path: persist downloaded bytes and leave paused if already set.
                    if task.status != DownloadStatus::Paused {
                        task.status = DownloadStatus::Canceled;
                        task.speed_bytes_per_second = 0;
                        task.bump_revision();
                        let _ = inner.db.upsert_download(&task);
                        emit(&inner, &task);
                    }
                    return;
                }
                task.status = DownloadStatus::Failed;
                task.speed_bytes_per_second = 0;
                task.error = Some(TaskError {
                    code: err.code,
                    message: err.message,
                    retryable: err.retryable,
                });
                task.bump_revision();
                let _ = inner.db.upsert_download(&task);
                emit(&inner, &task);
            }
        }
    }
}

async fn execute_download(
    inner: &ManagerInner,
    task_id: &str,
    cancel: &crate::infrastructure::http::CancelToken,
) -> Result<(), AppError> {
    let mut task = inner.db.get_download(task_id)?;
    if task.status == DownloadStatus::Queued {
        ensure_transition(task.status, DownloadStatus::Connecting)?;
        task.status = DownloadStatus::Connecting;
        task.bump_revision();
        inner.db.upsert_download(&task)?;
        emit(inner, &task);
    } else if task.status != DownloadStatus::Connecting {
        return Err(AppError::new(
            ErrorCode::InvalidTransition,
            format!("Worker cannot start from status {}", task.status.as_str()),
        ));
    }

    let temp_path = task
        .temp_path
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Missing temp path"))?;
    let final_path = task
        .final_path
        .clone()
        .ok_or_else(|| AppError::new(ErrorCode::Internal, "Missing final path"))?;

    let resume_from = if Path::new(&temp_path).exists() {
        std::fs::metadata(&temp_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    task.downloaded_bytes = resume_from;

    ensure_transition(task.status, DownloadStatus::Downloading)?;
    task.status = DownloadStatus::Downloading;
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    emit(inner, &task);

    let progress = inner.progress.clone();
    let db = inner.db.clone();
    let task_id_owned = task_id.to_string();

    let download = download_to_file(
        &inner.http,
        &task.url,
        Path::new(&temp_path),
        resume_from,
        task.etag.as_deref(),
        cancel,
        move |update| {
            if let Ok(mut current) = db.get_download(&task_id_owned) {
                if matches!(
                    current.status,
                    DownloadStatus::Canceled | DownloadStatus::Paused
                ) {
                    return;
                }
                current.downloaded_bytes = update.transferred_bytes;
                current.total_bytes = update.total_bytes.or(current.total_bytes);
                current.speed_bytes_per_second = update.bytes_per_second;
                current.status = DownloadStatus::Downloading;
                current.bump_revision();
                let _ = db.upsert_download(&current);
                progress.emit(ProgressEvent {
                    task_id: current.id.clone(),
                    revision: current.revision,
                    transferred_bytes: current.downloaded_bytes,
                    total_bytes: current.total_bytes,
                    bytes_per_second: current.speed_bytes_per_second,
                    status: current.status,
                    timestamp: Utc::now(),
                });
            }
        },
    )
    .await;

    // If paused/canceled meanwhile, stop without failing.
    let latest = inner.db.get_download(task_id)?;
    if matches!(
        latest.status,
        DownloadStatus::Paused | DownloadStatus::Canceled
    ) {
        return Ok(());
    }

    let result = download?;

    let mut task = inner.db.get_download(task_id)?;
    ensure_transition(task.status, DownloadStatus::Verifying)?;
    task.status = DownloadStatus::Verifying;
    task.downloaded_bytes = result.bytes_written;
    task.total_bytes = result.total_bytes.or(task.total_bytes);
    task.etag = result.etag.or(task.etag);
    task.last_modified = result.last_modified.or(task.last_modified);
    task.speed_bytes_per_second = 0;
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    emit(inner, &task);

    if let Some(total) = task.total_bytes {
        if task.downloaded_bytes != total {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                format!(
                    "Byte length mismatch: got {} expected {total}",
                    task.downloaded_bytes
                ),
            ));
        }
    }

    // Atomic rename within same directory.
    tokio::fs::rename(&temp_path, &final_path)
        .await
        .map_err(|e| AppError::new(ErrorCode::PathNotAllowed, format!("Finalize failed: {e}")))?;

    let mut task = inner.db.get_download(task_id)?;
    ensure_transition(task.status, DownloadStatus::Completed)?;
    task.status = DownloadStatus::Completed;
    task.error = None;
    task.speed_bytes_per_second = 0;
    task.temp_path = None;
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    emit(inner, &task);
    let _ = result.sha256_hex;
    Ok(())
}

fn emit(inner: &ManagerInner, task: &DownloadTask) {
    inner.progress.emit(ProgressEvent {
        task_id: task.id.clone(),
        revision: task.revision,
        transferred_bytes: task.downloaded_bytes,
        total_bytes: task.total_bytes,
        bytes_per_second: task.speed_bytes_per_second,
        status: task.status,
        timestamp: Utc::now(),
    });
}

fn unique_final_path(dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = Path::new(file_name)
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for i in 1..10_000 {
        let path = dir.join(format!("{stem} ({i}){ext}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(AppError::new(
        ErrorCode::AlreadyExists,
        "Could not allocate unique filename",
    ))
}
