use crate::domain::filename::{filename_from_url, sanitize_filename};
use crate::domain::transitions::ensure_transition;
use crate::domain::url_policy::validate_download_url;
use crate::domain::{
    AppError, CreateDownloadRequest, DownloadStatus, DownloadTask, ErrorCode, ProgressEvent, Queue,
    RemoveDownloadRequest, RemoveDownloadResult, Settings, TaskError, UpdateSettingsRequest,
};
use crate::infrastructure::db::Database;
use crate::infrastructure::http::{ResumeMetadata, cancel_pair, download_to_file};
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
    join: tauri::async_runtime::JoinHandle<()>,
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
    mutations: Mutex<()>,
    controls: tokio::sync::Mutex<()>,
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
                mutations: Mutex::new(()),
                controls: tokio::sync::Mutex::new(()),
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
        let _mutation = self.inner.mutations.lock();
        if let Some(request_id) = req.request_id.as_deref()
            && let Some(existing) = self.inner.db.find_by_request_id(request_id)?
        {
            return Ok(existing);
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
        let reserved = self.inner.db.list_downloads()?;
        let final_path = unique_final_path(&self.inner.download_dir, &file_name, &reserved)?;
        let file_name = final_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

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

    pub async fn pause(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        self.set_stopped_status(id, DownloadStatus::Paused)?;
        self.stop_worker(id).await;
        self.refresh_partial(id)
    }

    pub async fn resume(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        ensure_transition(
            self.inner.db.get_download(id)?.status,
            DownloadStatus::Queued,
        )?;
        self.stop_worker(id).await;
        let _mutation = self.inner.mutations.lock();
        let mut task = self.inner.db.get_download(id)?;
        ensure_transition(task.status, DownloadStatus::Queued)?;
        if let Some(temp) = &task.temp_path
            && let Ok(meta) = std::fs::metadata(temp)
        {
            task.downloaded_bytes = meta.len();
        }
        task.status = DownloadStatus::Queued;
        task.error = None;
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        self.spawn_worker(task.id.clone());
        Ok(task)
    }

    pub async fn cancel(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        self.set_stopped_status(id, DownloadStatus::Canceled)?;
        self.stop_worker(id).await;
        self.refresh_partial(id)
    }

    fn set_stopped_status(&self, id: &str, status: DownloadStatus) -> Result<(), AppError> {
        let _mutation = self.inner.mutations.lock();
        let mut task = self.inner.db.get_download(id)?;
        ensure_transition(task.status, status)?;
        task.status = status;
        task.speed_bytes_per_second = 0;
        task.error = None;
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        Ok(())
    }

    fn refresh_partial(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _mutation = self.inner.mutations.lock();
        let mut task = self.inner.db.get_download(id)?;
        if let Some(temp) = &task.temp_path
            && let Ok(meta) = std::fs::metadata(temp)
        {
            task.downloaded_bytes = meta.len();
            task.bump_revision();
            self.inner.db.upsert_download(&task)?;
            self.emit_progress(&task);
        }
        Ok(task)
    }

    pub async fn retry(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        if self.inner.db.get_download(id)?.status != DownloadStatus::Failed {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Only failed tasks can be retried",
            ));
        }
        self.stop_worker(id).await;
        let _mutation = self.inner.mutations.lock();
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
        Ok(task)
    }

    pub async fn remove(
        &self,
        req: RemoveDownloadRequest,
    ) -> Result<RemoveDownloadResult, AppError> {
        let _control = self.inner.controls.lock().await;
        // Block callbacks before stopping and draining the writer.
        {
            let _mutation = self.inner.mutations.lock();
            let mut task = self.inner.db.get_download(&req.id)?;
            if task.status.is_active() {
                task.status = DownloadStatus::Canceled;
                task.bump_revision();
                self.inner.db.upsert_download(&task)?;
            }
        }
        self.stop_worker(&req.id).await;
        let _mutation = self.inner.mutations.lock();
        let task = self.inner.db.get_download(&req.id)?;
        if req.delete_partial
            && let Some(temp) = &task.temp_path
            && let Err(error) = std::fs::remove_file(temp)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(crate::infrastructure::http::map_io_err(error));
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

    async fn stop_worker(&self, id: &str) {
        let handle = self.inner.workers.lock().remove(id);
        if let Some(handle) = handle {
            let _ = handle.cancel_tx.send(true);
            let _ = handle.join.await;
        }
    }

    fn spawn_worker(&self, task_id: String) {
        let inner = Arc::clone(&self.inner);
        let (cancel_tx, cancel_token) = cancel_pair();
        // Hold registry until insertion so a fast worker cannot leave a stale entry.
        let mut workers = self.inner.workers.lock();
        let worker_id = task_id.clone();
        let join = tauri::async_runtime::spawn(async move {
            run_download_job(inner.clone(), worker_id.clone(), cancel_token).await;
            inner.workers.lock().remove(&worker_id);
        });
        workers.insert(task_id, WorkerHandle { cancel_tx, join });
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
    let _mutation = inner.mutations.lock();

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
    let mut task = {
        let _mutation = inner.mutations.lock();
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

        task
    };
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

    {
        let _mutation = inner.mutations.lock();
        task = inner.db.get_download(task_id)?;
        ensure_transition(task.status, DownloadStatus::Downloading)?;
        task.status = DownloadStatus::Downloading;
        task.bump_revision();
        inner.db.upsert_download(&task)?;
        emit(inner, &task);
    }

    let download = download_to_file(
        &inner.http,
        &task.url,
        Path::new(&temp_path),
        ResumeMetadata {
            offset: resume_from,
            etag: task.etag.clone(),
        },
        cancel,
        |metadata| {
            let _mutation = inner.mutations.lock();
            let mut current = inner.db.get_download(task_id)?;
            if current.status != DownloadStatus::Downloading {
                return Err(AppError::new(
                    ErrorCode::InvalidTransition,
                    "Download canceled",
                ));
            }
            current.etag = metadata.etag.clone();
            current.last_modified = metadata.last_modified.clone();
            current.total_bytes = metadata.total_bytes;
            current.bump_revision();
            inner.db.upsert_download(&current)?;
            emit(inner, &current);
            Ok(())
        },
        |update| {
            let _mutation = inner.mutations.lock();
            if let Ok(mut current) = inner.db.get_download(task_id)
                && current.status == DownloadStatus::Downloading
            {
                current.downloaded_bytes = update.transferred_bytes;
                current.total_bytes = update.total_bytes.or(current.total_bytes);
                current.speed_bytes_per_second = update.bytes_per_second;
                current.bump_revision();
                if inner.db.upsert_download(&current).is_ok() {
                    emit(inner, &current);
                }
            }
        },
    )
    .await;

    let _mutation = inner.mutations.lock();
    // Stop and finalization share a mutation boundary; neither can resurrect the other.
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
    task.speed_bytes_per_second = 0;
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    emit(inner, &task);

    if let Some(total) = task.total_bytes
        && task.downloaded_bytes != total
    {
        return Err(AppError::new(
            ErrorCode::ChecksumMismatch,
            format!(
                "Byte length mismatch: got {} expected {total}",
                task.downloaded_bytes
            ),
        ));
    }

    // A hard link publishes the complete file atomically without replacing an existing path.
    // Both names are in the managed directory and therefore on the same filesystem.
    std::fs::hard_link(&temp_path, &final_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            AppError::new(
                ErrorCode::AlreadyExists,
                "The destination filename is already in use; the partial file was kept",
            )
        } else {
            crate::infrastructure::http::map_io_err(error)
        }
    })?;
    std::fs::remove_file(&temp_path).map_err(crate::infrastructure::http::map_io_err)?;

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

fn unique_final_path(
    dir: &Path,
    file_name: &str,
    reserved: &[DownloadTask],
) -> Result<PathBuf, AppError> {
    let available = |path: &Path| {
        std::fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            && !reserved
                .iter()
                .any(|task| task.final_path.as_deref() == path.to_str())
    };
    let candidate = dir.join(file_name);
    if available(&candidate) {
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
        if available(&path) {
            return Ok(path);
        }
    }
    Err(AppError::new(
        ErrorCode::AlreadyExists,
        "Could not allocate unique filename",
    ))
}
