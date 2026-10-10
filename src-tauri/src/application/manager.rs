use crate::domain::filename::{filename_from_url, sanitize_filename};
use crate::domain::transitions::ensure_transition;
use crate::domain::url_policy::validate_download_url;
use crate::domain::{
    AppError, CreateDownloadRequest, DownloadStatus, DownloadTask, ErrorCode, ProgressEvent, Queue,
    RemoveDownloadRequest, RemoveDownloadResult, Settings, TaskError, UpdateSettingsRequest,
};
use crate::infrastructure::db::Database;
use crate::infrastructure::files::ManagedFiles;
use crate::infrastructure::http::{ResumeMetadata, cancel_pair, download_managed};
use crate::infrastructure::ownership::DataDirectoryLease;
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
    _data_dir_lease: DataDirectoryLease,
    download_dir: PathBuf,
    files: ManagedFiles,
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
        data_dir_lease: DataDirectoryLease,
    ) -> Result<Self, AppError> {
        let files = ManagedFiles::new(&download_dir)?;
        let http = crate::infrastructure::network::client()?;
        Ok(Self {
            inner: Arc::new(ManagerInner {
                db,
                _data_dir_lease: data_dir_lease,
                download_dir,
                files,
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
            let recovery = reconcile_task(&self.inner, &mut task);
            if let Err(error) = recovery {
                task.status = DownloadStatus::Failed;
                task.error = Some(TaskError {
                    code: error.code,
                    message: error.message,
                    retryable: error.retryable,
                });
                task.speed_bytes_per_second = 0;
            }
            task.bump_revision();
            self.inner.db.upsert_download(&task)?;
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
        let expected_sha256 = crate::domain::task::normalize_sha256(req.expected_sha256)?;
        let settings = self.inner.db.get_settings()?;
        let destination_id = req
            .destination_id
            .unwrap_or(settings.default_destination_id);
        if destination_id != "default" {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Only the app-managed download folder is approved.",
            ));
        }
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
            expected_sha256,
            actual_sha256: None,
            checksum_result: None,
            finalization: None,
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
        if let Some(temp) = &task.temp_path {
            let name = managed_name(&self.inner, temp)?;
            task.downloaded_bytes = self.inner.files.length(&name)?.unwrap_or(0);
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
        if let Some(temp) = &task.temp_path {
            let name = managed_name(&self.inner, temp)?;
            task.downloaded_bytes = self.inner.files.length(&name)?.unwrap_or(0);
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
        {
            self.inner.files.remove(&managed_name(&self.inner, temp)?)?;
        }
        self.inner.db.remove_download(&req.id)?;
        Ok(RemoveDownloadResult { removed: true })
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        self.inner.db.get_settings()
    }

    pub fn update_settings(&self, patch: UpdateSettingsRequest) -> Result<Settings, AppError> {
        if patch
            .default_destination_id
            .as_deref()
            .is_some_and(|id| id != "default")
        {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Only the app-managed download folder is approved.",
            ));
        }
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

    let temp_name = managed_name(inner, &temp_path)?;
    managed_name(inner, &final_path)?;
    if task.finalization.is_some() {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "Restart Circle to recover the verified publication before retrying.",
        ));
    }
    let resume_from = inner.files.length(&temp_name)?.unwrap_or(0);
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

    let download = download_managed(
        (&inner.http, &inner.files),
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
                && matches!(
                    current.status,
                    DownloadStatus::Downloading | DownloadStatus::Verifying
                )
            {
                if update.verifying {
                    current.status = DownloadStatus::Verifying;
                }
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
    if task.status != DownloadStatus::Verifying {
        ensure_transition(task.status, DownloadStatus::Verifying)?;
    }
    task.status = DownloadStatus::Verifying;
    task.downloaded_bytes = result.bytes_written;
    task.total_bytes = result.total_bytes.or(task.total_bytes);
    task.speed_bytes_per_second = 0;
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    emit(inner, &task);

    // Length validation is separate from cryptographic checksum comparison.
    if task
        .total_bytes
        .is_some_and(|total| task.downloaded_bytes != total)
    {
        return Err(AppError::new(
            ErrorCode::LengthMismatch,
            "The received byte length did not match the response.",
        ));
    }
    task.actual_sha256 = Some(result.sha256_hex.clone());
    task.checksum_result = Some(
        match &task.expected_sha256 {
            Some(expected) if expected != &result.sha256_hex => "mismatch",
            Some(_) => "matched",
            None => "computed",
        }
        .into(),
    );
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    if task.checksum_result.as_deref() == Some("mismatch") {
        return Err(AppError::new(
            ErrorCode::ChecksumMismatch,
            "SHA-256 did not match the supplied checksum. The partial file was kept; obtain a trusted link or checksum.",
        ));
    }
    // File sync precedes the FULL-synchronous SQLite publication-intent checkpoint.
    task.finalization = Some("ready".into());
    task.bump_revision();
    inner.db.upsert_download(&task)?;
    inner.files.publish(
        &temp_name,
        &managed_name(inner, &final_path)?,
        &result.sha256_hex,
        Some(&result.verified_file),
    )?;
    complete_and_cleanup(inner, &mut task)?;
    emit(inner, &task);
    Ok(())
}

fn managed_name(inner: &ManagerInner, path: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(path);
    if path.parent() != Some(inner.download_dir.as_path()) {
        return Err(AppError::new(
            ErrorCode::PathNotAllowed,
            "The saved download path is outside the approved folder.",
        ));
    }
    Ok(PathBuf::from(inner.files.name(path)?))
}

fn complete_and_cleanup(inner: &ManagerInner, task: &mut DownloadTask) -> Result<(), AppError> {
    task.status = DownloadStatus::Completed;
    task.error = None;
    task.speed_bytes_per_second = 0;
    task.finalization = Some("cleanup".into());
    task.bump_revision();
    inner.db.upsert_download(task)?; // Keep partial path until completed state is durable.
    if let Some(temp) = &task.temp_path {
        inner.files.remove(&managed_name(inner, temp)?)?;
    }
    task.temp_path = None;
    task.finalization = None;
    task.bump_revision();
    inner.db.upsert_download(task)
}

fn reconcile_task(inner: &ManagerInner, task: &mut DownloadTask) -> Result<(), AppError> {
    if task.status == DownloadStatus::Canceled {
        task.finalization = None;
        return Ok(());
    }
    let Some(final_path) = task.final_path.as_deref() else {
        return Ok(());
    };
    let final_name = managed_name(inner, final_path)?;
    let final_exists = inner.files.length(&final_name)?.is_some();
    if task.finalization.is_some() {
        if !matches!(task.finalization.as_deref(), Some("ready" | "cleanup")) {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The publication checkpoint is unsupported. Restore the database from a compatible version.",
            ));
        }
        let digest = task.actual_sha256.as_deref().ok_or_else(|| {
            AppError::new(
                ErrorCode::ChecksumMismatch,
                "The publication checkpoint has no verified digest.",
            )
        })?;
        if task.checksum_result.as_deref() == Some("mismatch")
            || task
                .expected_sha256
                .as_deref()
                .is_some_and(|expected| expected != digest)
        {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "The saved checksum comparison failed.",
            ));
        }
        if final_exists {
            if inner.files.digest(&final_name)? != digest {
                return Err(AppError::new(
                    ErrorCode::ChecksumMismatch,
                    "The final file differs from the verified checkpoint. Both files were kept.",
                ));
            }
        } else {
            let temp = task.temp_path.as_deref().ok_or_else(|| {
                AppError::new(
                    ErrorCode::StorageNotFound,
                    "The verified partial and final file are missing.",
                )
            })?;
            inner
                .files
                .publish(&managed_name(inner, temp)?, &final_name, digest, None)?;
        }
        return complete_and_cleanup(inner, task);
    }
    if task.status == DownloadStatus::Completed {
        if !final_exists {
            return Err(AppError::new(
                ErrorCode::StorageNotFound,
                "The completed file is missing. Restore it or add a fresh download.",
            ));
        }
        if let Some(digest) = &task.actual_sha256
            && inner.files.digest(&final_name)? != *digest
        {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "The completed file changed after verification.",
            ));
        }
        return Ok(());
    }
    if final_exists {
        return Err(AppError::new(
            ErrorCode::AlreadyExists,
            "A final file exists without a verified publication checkpoint. It was kept; add a fresh download.",
        ));
    }
    if let Some(temp) = &task.temp_path {
        let length = inner.files.length(&managed_name(inner, temp)?)?;
        match length {
            Some(length) => {
                if task.total_bytes.is_some_and(|total| length > total) {
                    return Err(AppError::new(
                        ErrorCode::LengthMismatch,
                        "The partial file exceeds the saved response length.",
                    ));
                }
                task.downloaded_bytes = length;
            }
            None if task.downloaded_bytes > 0 => {
                task.downloaded_bytes = 0;
                task.etag = None;
                task.last_modified = None;
                task.actual_sha256 = None;
                task.checksum_result = None;
                return Err(AppError::new(
                    ErrorCode::StorageNotFound,
                    "The partial file is missing. Add a fresh download.",
                ));
            }
            None => {}
        }
    }
    if task.status.is_active() {
        task.status = DownloadStatus::Failed;
        task.speed_bytes_per_second = 0;
        task.error = Some(TaskError {
            code: ErrorCode::NetworkInterrupted,
            message: "Interrupted by application restart. Retry to continue safely.".into(),
            retryable: true,
        });
    }
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
