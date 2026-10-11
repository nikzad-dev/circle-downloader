use crate::application::retry::{PER_HOST_LIMIT, RETRY_BUDGET, next_retry_at};
use crate::application::segmentation::{
    SegmentationRequest, coverage_contiguous, download_with_optional_segmentation,
    reconcile_segment_file, segment_file_path,
};
use crate::domain::filename::{filename_from_url, sanitize_filename};
use crate::domain::transitions::ensure_transition;
use crate::domain::url_policy::validate_download_url;
use crate::domain::{
    AppError, CreateDownloadRequest, Destination, DownloadStatus, DownloadTask, ErrorCode,
    GrantDestinationRequest, ProgressEvent, Queue, RemoveDownloadRequest, RemoveDownloadResult,
    Settings, TaskError, TransferMode, UpdateSettingsRequest,
};
use crate::infrastructure::db::{Database, is_unique_constraint};
use crate::infrastructure::files::ManagedFiles;
use crate::infrastructure::http::cancel_pair;
use crate::infrastructure::ownership::DataDirectoryLease;
use crate::ipc::progress::ProgressHub;
use chrono::Utc;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::watch;
use uuid::Uuid;

#[path = "advanced_management.rs"]
pub mod advanced;

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
    dispatch_lock: Mutex<()>,
    last_queue: Mutex<String>,
    global_limit: Arc<crate::infrastructure::rate_limit::RateLimiter>,
    queue_limits: Mutex<HashMap<String, Arc<crate::infrastructure::rate_limit::RateLimiter>>>,
    progress: ProgressHub,
    workers: Mutex<HashMap<String, WorkerHandle>>,
    mutations: Mutex<()>,
    controls: tokio::sync::Mutex<()>,
    shutting_down: AtomicBool,
    retry_wake: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl DownloadManager {
    pub fn new(
        db: Database,
        download_dir: PathBuf,
        progress: ProgressHub,
        data_dir_lease: DataDirectoryLease,
    ) -> Result<Self, AppError> {
        let files = ManagedFiles::new(&download_dir)?;
        let rate = db.network_settings()?.bytes_per_second;
        Ok(Self {
            inner: Arc::new(ManagerInner {
                db,
                _data_dir_lease: data_dir_lease,
                download_dir,
                files,
                dispatch_lock: Mutex::new(()),
                last_queue: Mutex::new(String::new()),
                global_limit: Arc::new(crate::infrastructure::rate_limit::RateLimiter::new(rate)),
                queue_limits: Mutex::new(HashMap::new()),
                progress,
                workers: Mutex::new(HashMap::new()),
                mutations: Mutex::new(()),
                controls: tokio::sync::Mutex::new(()),
                shutting_down: AtomicBool::new(false),
                retry_wake: Mutex::new(None),
            }),
        })
    }

    pub fn progress_hub(&self) -> ProgressHub {
        self.inner.progress.clone()
    }

    pub fn download_dir(&self) -> &Path {
        &self.inner.download_dir
    }

    pub fn prepare_category_folders(&self) -> Result<(), AppError> {
        for category in crate::domain::category::CATEGORIES {
            self.inner.files.child(Path::new(category))?;
        }
        Ok(())
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
                task.next_retry_at = None;
                task.queue_position = None;
            }
            // Interrupted active work becomes Failed; user-paused intent is left alone.
            if task.status == DownloadStatus::Paused {
                task.queue_position = None;
                task.next_retry_at = None;
            }
            task.bump_revision();
            self.inner.db.upsert_download(&task)?;
        }
        self.dispatch();
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<DownloadTask>, AppError> {
        self.inner.db.list_downloads()
    }

    pub fn get(&self, id: &str) -> Result<DownloadTask, AppError> {
        self.inner.db.get_download(id)
    }

    pub fn create(&self, req: CreateDownloadRequest) -> Result<DownloadTask, AppError> {
        self.create_in_category(req, "")
    }

    fn create_in_category(
        &self,
        req: CreateDownloadRequest,
        category: &str,
    ) -> Result<DownloadTask, AppError> {
        let _mutation = self.inner.mutations.lock();
        if let Some(request_id) = req.request_id.as_deref()
            && let Some(existing) = self.inner.db.find_by_request_id(request_id)?
        {
            return Ok(existing);
        }

        let url = validate_download_url(&req.url)?;
        let host = host_from_url(url.as_str());
        let expected_sha256 = crate::domain::task::normalize_sha256(req.expected_sha256)?;
        let settings = self.inner.db.get_settings()?;
        let mut destination_id = req
            .destination_id
            .unwrap_or(settings.default_destination_id);
        let file_name = match req.suggested_name {
            Some(name) => sanitize_filename(&name)?,
            None => filename_from_url(url.as_str()),
        };
        let inferred = crate::domain::category::from_filename(&file_name);
        let category = if category.is_empty() {
            &inferred
        } else {
            category
        };
        if destination_id == "default" && crate::domain::category::CATEGORIES.contains(&category) {
            destination_id = format!("circle-category:{category}");
        }
        let dest_root = resolve_root(&self.inner, &destination_id)?;
        let _files = destination_files(&self.inner, &destination_id)?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let temp_path = dest_root
            .join(format!(".{id}.{file_name}.part"))
            .to_string_lossy()
            .to_string();
        let reserved = self.inner.db.list_downloads()?;
        let final_path = unique_final_path(&dest_root, &file_name, &reserved)?;
        let file_name = final_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let start_now = req.start_now && settings.auto_start;
        let queue_id = "default".to_string();
        let queue_position = if start_now {
            Some(self.inner.db.next_queue_position(&queue_id)?)
        } else {
            None
        };
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
            request_id: req.request_id.clone(),
            retry_count: 0,
            expected_sha256,
            actual_sha256: None,
            checksum_result: None,
            finalization: None,
            next_retry_at: None,
            queue_id,
            queue_position,
            host,
            transfer_mode: TransferMode::Single,
        };
        if let Err(error) = self.inner.db.upsert_download(&task) {
            if is_unique_constraint(&error)
                && let Some(request_id) = req.request_id.as_deref()
                && let Some(existing) = self.inner.db.find_by_request_id(request_id)?
            {
                return Ok(existing);
            }
            return Err(error);
        }
        self.emit_progress(&task);

        if start_now {
            drop(_mutation);
            self.dispatch();
            task = self.inner.db.get_download(&id)?;
        }
        Ok(task)
    }

    pub async fn pause(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        self.set_stopped_status(id, DownloadStatus::Paused)?;
        self.stop_worker(id).await;
        let task = self.refresh_partial(id)?;
        self.dispatch();
        Ok(task)
    }

    pub async fn resume(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        ensure_transition(
            self.inner.db.get_download(id)?.status,
            DownloadStatus::Queued,
        )?;
        self.stop_worker(id).await;
        let task = {
            let _mutation = self.inner.mutations.lock();
            let mut task = self.inner.db.get_download(id)?;
            ensure_transition(task.status, DownloadStatus::Queued)?;
            if let Some(temp) = &task.temp_path {
                let root = resolve_root(&self.inner, &task.destination_id)?;
                let files = destination_files(&self.inner, &task.destination_id)?;
                let name = managed_name(&root, temp)?;
                task.downloaded_bytes = if task.transfer_mode == TransferMode::Segmented {
                    self.inner
                        .db
                        .list_segments(&task.id)?
                        .iter()
                        .map(|s| s.completed_bytes)
                        .sum()
                } else {
                    files.length(&name)?.unwrap_or(0)
                };
            }
            enqueue_runnable(&self.inner, &mut task)?;
            task.error = None;
            task.next_retry_at = None;
            task.bump_revision();
            self.inner.db.upsert_download(&task)?;
            self.emit_progress(&task);
            task
        };
        self.dispatch();
        Ok(task)
    }

    pub async fn cancel(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _control = self.inner.controls.lock().await;
        self.set_stopped_status(id, DownloadStatus::Canceled)?;
        self.stop_worker(id).await;
        let task = self.refresh_partial(id)?;
        self.dispatch();
        Ok(task)
    }

    fn set_stopped_status(&self, id: &str, status: DownloadStatus) -> Result<(), AppError> {
        let _mutation = self.inner.mutations.lock();
        let mut task = self.inner.db.get_download(id)?;
        ensure_transition(task.status, status)?;
        task.status = status;
        task.speed_bytes_per_second = 0;
        task.error = None;
        task.next_retry_at = None;
        if status == DownloadStatus::Paused || status == DownloadStatus::Canceled {
            task.queue_position = None;
        }
        task.bump_revision();
        self.inner.db.upsert_download(&task)?;
        self.emit_progress(&task);
        Ok(())
    }

    fn refresh_partial(&self, id: &str) -> Result<DownloadTask, AppError> {
        let _mutation = self.inner.mutations.lock();
        let mut task = self.inner.db.get_download(id)?;
        if let Some(temp) = &task.temp_path {
            let root = resolve_root(&self.inner, &task.destination_id)?;
            let files = destination_files(&self.inner, &task.destination_id)?;
            let name = managed_name(&root, temp)?;
            task.downloaded_bytes = if task.transfer_mode == TransferMode::Segmented {
                self.inner
                    .db
                    .list_segments(&task.id)?
                    .iter()
                    .map(|segment| segment.completed_bytes)
                    .sum()
            } else {
                files.length(&name)?.unwrap_or(0)
            };
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
        let task = {
            let _mutation = self.inner.mutations.lock();
            let mut task = self.inner.db.get_download(id)?;
            if task.status != DownloadStatus::Failed {
                return Err(AppError::new(
                    ErrorCode::InvalidTransition,
                    "Only failed tasks can be retried",
                ));
            }
            // Manual retry forces immediate dispatch without consuming an extra budget slot.
            enqueue_runnable(&self.inner, &mut task)?;
            task.error = None;
            task.next_retry_at = None;
            task.bump_revision();
            self.inner.db.upsert_download(&task)?;
            self.emit_progress(&task);
            task
        };
        self.dispatch();
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
            let root = resolve_root(&self.inner, &task.destination_id)?;
            let files = destination_files(&self.inner, &task.destination_id)?;
            for segment in self.inner.db.list_segments(&task.id)? {
                let path = segment_file_path(temp, segment.segment_index);
                files.remove(&managed_name(&root, &path.to_string_lossy())?)?;
            }
            files.remove(&managed_name(&root, temp)?)?;
        }
        self.inner.db.remove_download(&req.id)?;
        Ok(RemoveDownloadResult { removed: true })
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        self.inner.db.get_settings()
    }

    pub fn update_settings(&self, patch: UpdateSettingsRequest) -> Result<Settings, AppError> {
        if let Some(id) = patch.default_destination_id.as_deref()
            && !self.inner.db.destination_exists(id)?
        {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Choose an approved download folder.",
            ));
        }
        let settings = self.inner.db.update_settings(patch)?;
        self.dispatch();
        Ok(settings)
    }

    pub fn list_queues(&self) -> Result<Vec<Queue>, AppError> {
        self.inner.db.list_queues()
    }

    pub fn list_destinations(&self) -> Result<Vec<Destination>, AppError> {
        let mut items = vec![Destination {
            id: "default".into(),
            label: "Downloads / CIRCLE".into(),
            display_path: display_path(&self.inner.download_dir),
            is_default: true,
        }];
        for (id, label, root) in self.inner.db.list_destinations()? {
            items.push(Destination {
                id,
                label,
                display_path: display_path(Path::new(&root)),
                is_default: false,
            });
        }
        Ok(items)
    }

    pub fn grant_destination(&self, req: GrantDestinationRequest) -> Result<Destination, AppError> {
        let path = PathBuf::from(req.path.trim());
        if !path.is_absolute() {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Destination folders must be absolute paths.",
            ));
        }
        let canonical =
            std::fs::canonicalize(&path).map_err(crate::infrastructure::errors::map_io_err)?;
        if !canonical.is_dir() {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Choose an existing folder.",
            ));
        }
        // Ensure the directory is usable with the same no-follow writer model.
        let _ = ManagedFiles::new(&canonical)?;
        let id = Uuid::new_v4().to_string();
        let label = req
            .label
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Folder")
                    .to_string()
            });
        let root = canonical.to_string_lossy().into_owned();
        self.inner
            .db
            .insert_destination(&id, &label, &root, &Utc::now().to_rfc3339())?;
        Ok(Destination {
            id,
            label,
            display_path: display_path(&canonical),
            is_default: false,
        })
    }

    pub fn remove_destination(&self, id: &str) -> Result<(), AppError> {
        if id == "default" {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "The app-managed downloads folder cannot be removed.",
            ));
        }
        self.inner.db.remove_destination(id)
    }

    pub fn open_download(&self, id: &str) -> Result<(), AppError> {
        let task = self.inner.db.get_download(id)?;
        let path = task.final_path.as_deref().ok_or_else(|| {
            AppError::new(
                ErrorCode::StorageNotFound,
                "This download has no final file to open.",
            )
        })?;
        ensure_path_under_destination(&self.inner, &task.destination_id, path)?;
        if !Path::new(path).is_file() {
            return Err(AppError::new(
                ErrorCode::StorageNotFound,
                "The completed file is missing.",
            ));
        }
        open::that(path).map_err(|_| {
            AppError::new(
                ErrorCode::StorageError,
                "Could not open the file with the system handler.",
            )
        })
    }

    pub fn reveal_download(&self, id: &str) -> Result<(), AppError> {
        let task = self.inner.db.get_download(id)?;
        let candidate = task
            .final_path
            .as_deref()
            .or(task.temp_path.as_deref())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::StorageNotFound,
                    "This download has no folder to reveal.",
                )
            })?;
        ensure_path_under_destination(&self.inner, &task.destination_id, candidate)?;
        let path = Path::new(candidate);
        let target = if path.exists() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        reveal_path(target)
    }

    /// Cancel every worker, await drains, and refuse further dispatch.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        if let Some(handle) = self.inner.retry_wake.lock().take() {
            handle.abort();
        }
        let ids: Vec<String> = self.inner.workers.lock().keys().cloned().collect();
        for id in ids {
            self.stop_worker(&id).await;
        }
    }

    pub fn dispatch(&self) {
        let _dispatch = self.inner.dispatch_lock.lock();
        if self.inner.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let settings = match self.inner.db.get_settings() {
            Ok(settings) => settings,
            Err(_) => return,
        };
        let max_global = settings.max_concurrent_downloads as usize;
        let now = Utc::now();
        let runnable = match self.inner.db.list_runnable_queued(now) {
            Ok(tasks) => tasks,
            Err(_) => return,
        };

        let policies = match self.inner.db.queue_policies() {
            Ok(p) => p,
            Err(_) => return,
        };
        let mut remaining = runnable;
        let mut queue_ids: Vec<_> = policies
            .iter()
            .filter(|p| !p.paused)
            .map(|p| p.id.clone())
            .collect();
        queue_ids.sort();
        if let Some(index) = queue_ids
            .iter()
            .position(|id| *id == *self.inner.last_queue.lock())
        {
            let n = queue_ids.len();
            queue_ids.rotate_left((index + 1) % n);
        }
        loop {
            let mut admitted = false;
            for queue_id in &queue_ids {
                let workers = self.inner.workers.lock();
                if workers.len() >= max_global {
                    break;
                }
                let active: Vec<_> = workers
                    .keys()
                    .filter_map(|id| self.inner.db.get_download(id).ok())
                    .collect();
                let Some(policy) = policies.iter().find(|p| p.id == *queue_id) else {
                    continue;
                };
                if active.iter().filter(|t| t.queue_id == *queue_id).count()
                    >= policy.max_concurrent as usize
                {
                    continue;
                }
                let Some(index) = remaining.iter().position(|t| {
                    t.queue_id == *queue_id
                        && !workers.contains_key(&t.id)
                        && active.iter().filter(|a| a.host == t.host).count() < PER_HOST_LIMIT
                }) else {
                    continue;
                };
                let task = remaining.remove(index);
                drop(workers);
                *self.inner.last_queue.lock() = queue_id.clone();
                self.spawn_worker(task.id);
                admitted = true;
            }
            if !admitted {
                break;
            }
        }
        self.schedule_retry_wake();
    }

    fn schedule_retry_wake(&self) {
        let Some(when) = self.inner.db.earliest_retry_at().ok().flatten() else {
            return;
        };
        let delay = (when - Utc::now())
            .to_std()
            .unwrap_or(std::time::Duration::from_millis(0))
            .min(std::time::Duration::from_secs(30));
        let mut slot = self.inner.retry_wake.lock();
        if let Some(handle) = slot.take() {
            handle.abort();
        }
        let manager = DownloadManager {
            inner: Arc::clone(&self.inner),
        };
        *slot = Some(tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            manager.dispatch();
        }));
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
        let (cancel_tx, mut cancel_token) = cancel_pair();
        cancel_token.limits.push(self.inner.global_limit.clone());
        if let Ok(task) = self.inner.db.get_download(&task_id) {
            let rate = self
                .inner
                .db
                .queue_policies()
                .ok()
                .and_then(|p| p.into_iter().find(|p| p.id == task.queue_id))
                .map(|p| p.bytes_per_second)
                .unwrap_or(0);
            let limit = self
                .inner
                .queue_limits
                .lock()
                .entry(task.queue_id)
                .or_insert_with(|| {
                    Arc::new(crate::infrastructure::rate_limit::RateLimiter::new(rate))
                })
                .clone();
            cancel_token.limits.push(limit);
        }
        // Hold registry until insertion so a fast worker cannot leave a stale entry.
        let mut workers = self.inner.workers.lock();
        if workers.contains_key(&task_id) {
            return;
        }
        let worker_id = task_id.clone();
        let join = tauri::async_runtime::spawn(async move {
            let measured = cancel_token.measured_bytes.clone();
            let started = std::time::Instant::now();
            run_download_job(inner.clone(), worker_id.clone(), cancel_token).await;
            let outcome = inner
                .db
                .get_download(&worker_id)
                .map(|t| {
                    if t.status == DownloadStatus::Completed {
                        "completed"
                    } else if t.error.is_some() {
                        "failed"
                    } else {
                        "interrupted"
                    }
                })
                .unwrap_or("interrupted");
            let _ = inner.db.record_measurement(
                &Uuid::new_v4().to_string(),
                &worker_id,
                measured.load(Ordering::Relaxed),
                started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                outcome,
            );
            inner.workers.lock().remove(&worker_id);
            let manager = DownloadManager { inner };
            manager.dispatch();
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
        Ok(()) => {
            if let Ok(mut task) = inner.db.get_download(&task_id) {
                task.next_retry_at = None;
                let _ = inner.db.upsert_download(&task);
            }
        }
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
                        task.queue_position = None;
                        task.next_retry_at = None;
                        task.bump_revision();
                        let _ = inner.db.upsert_download(&task);
                        emit(&inner, &task);
                    }
                    return;
                }
                let retry_after = err.retry_after_secs;
                let retryable = err.retryable;
                task.speed_bytes_per_second = 0;
                task.error = Some(TaskError {
                    code: err.code,
                    message: err.message,
                    retryable,
                });
                if retryable && task.retry_count < RETRY_BUDGET {
                    task.retry_count = task.retry_count.saturating_add(1);
                    task.status = DownloadStatus::Queued;
                    task.next_retry_at = Some(next_retry_at(task.retry_count, retry_after));
                    if task.queue_position.is_none() {
                        task.queue_position = Some(
                            inner
                                .db
                                .next_queue_position(&task.queue_id)
                                .unwrap_or(i64::MAX),
                        );
                    }
                } else {
                    task.status = DownloadStatus::Failed;
                    task.next_retry_at = None;
                    task.queue_position = None;
                }
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

    let root = resolve_root(inner, &task.destination_id)?;
    let files = destination_files(inner, &task.destination_id)?;
    let temp_name = managed_name(&root, &temp_path)?;
    managed_name(&root, &final_path)?;
    if task.finalization.is_some() {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "Restart Circle to recover the verified publication before retrying.",
        ));
    }
    let settings = inner.db.get_settings()?;

    let network = inner.db.network_settings()?;
    let options = inner.db.task_options(task_id)?;
    let max_connections = options
        .connections
        .unwrap_or(settings.max_connections_per_download);
    let proxy_secret = network
        .proxy_credential_id
        .as_deref()
        .map(|id| {
            crate::infrastructure::credentials::load(&inner.db, id, "proxy", &network.proxy_url)
        })
        .transpose()?;
    let origin_secret = options
        .credential_id
        .as_deref()
        .map(|id| crate::infrastructure::credentials::load(&inner.db, id, "origin", &task.url))
        .transpose()?;
    let origin = url::Url::parse(&task.url)
        .ok()
        .map(|u| u.origin().ascii_serialization());
    let configured = crate::infrastructure::network::configured_client(
        &network,
        proxy_secret.as_ref(),
        origin_secret.as_ref(),
        if origin_secret.is_some() {
            origin.as_deref()
        } else {
            None
        },
        cfg!(debug_assertions),
    )?;
    if task.transfer_mode == TransferMode::Segmented {
        let segments = inner.db.list_segments(task_id)?;
        task.downloaded_bytes = segments.iter().map(|s| s.completed_bytes).sum();
    } else {
        task.downloaded_bytes = files.length(&temp_name)?.unwrap_or(0);
    }

    {
        let _mutation = inner.mutations.lock();
        task = inner.db.get_download(task_id)?;
        ensure_transition(task.status, DownloadStatus::Downloading)?;
        task.status = DownloadStatus::Downloading;
        task.bump_revision();
        inner.db.upsert_download(&task)?;
        emit(inner, &task);
    }

    let download = download_with_optional_segmentation(
        SegmentationRequest {
            client: &configured,
            files: &files,
            db: &inner.db,
            task: &mut task,
            temp_path: Path::new(&temp_path),
            max_connections,
            cancel,
        },
        |metadata| {
            let _mutation = inner.mutations.lock();
            let mut current = inner.db.get_download(task_id)?;
            if current.status != DownloadStatus::Downloading {
                return Err(AppError::new(
                    ErrorCode::InvalidTransition,
                    "Download canceled",
                ));
            }
            inner
                .db
                .save_download_source(task_id, &metadata.effective_url)?;
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
                current.total_bytes = update.total_bytes;
                current.speed_bytes_per_second = update.bytes_per_second;
                current.bump_revision();
                if inner.db.upsert_download(&current).is_ok() {
                    emit(inner, &current);
                }
            }
        },
    )
    .await;

    // Persist final transfer mode chosen by the coordinator.
    {
        let _mutation = inner.mutations.lock();
        if let Ok(mut current) = inner.db.get_download(task_id) {
            current.transfer_mode = task.transfer_mode;
            let _ = inner.db.upsert_download(&current);
        }
    }

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
    files.publish(
        &temp_name,
        &managed_name(&root, &final_path)?,
        &result.sha256_hex,
        Some(&result.verified_file),
    )?;
    complete_and_cleanup(inner, &files, &root, &mut task)?;
    emit(inner, &task);
    Ok(())
}

fn resolve_root(inner: &ManagerInner, destination_id: &str) -> Result<PathBuf, AppError> {
    if destination_id == "default" {
        return Ok(inner.download_dir.clone());
    }
    if let Some(category) = category_destination(destination_id) {
        return Ok(inner.download_dir.join(category));
    }
    let path = inner
        .db
        .get_destination_path(destination_id)?
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::PathNotAllowed,
                "Choose an approved download folder.",
            )
        })?;
    Ok(PathBuf::from(path))
}

fn category_destination(id: &str) -> Option<&str> {
    id.strip_prefix("circle-category:")
        .filter(|name| crate::domain::category::CATEGORIES.contains(name))
}
fn destination_files(inner: &ManagerInner, id: &str) -> Result<ManagedFiles, AppError> {
    if let Some(category) = category_destination(id) {
        return inner.files.child(Path::new(category));
    }
    ManagedFiles::new(&resolve_root(inner, id)?)
}

fn ensure_path_under_destination(
    inner: &ManagerInner,
    destination_id: &str,
    path: &str,
) -> Result<(), AppError> {
    let root = resolve_root(inner, destination_id)?;
    let path = Path::new(path);
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            ErrorCode::PathNotAllowed,
            "The saved download path is outside the approved folder.",
        )
    })?;
    if parent != root.as_path() {
        return Err(AppError::new(
            ErrorCode::PathNotAllowed,
            "The saved download path is outside the approved folder.",
        ));
    }
    Ok(())
}

fn managed_name(root: &Path, path: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(path);
    if path.parent() != Some(root) {
        return Err(AppError::new(
            ErrorCode::PathNotAllowed,
            "The saved download path is outside the approved folder.",
        ));
    }
    let name = path.file_name().ok_or_else(|| {
        AppError::new(
            ErrorCode::PathNotAllowed,
            "The saved download path is outside the approved folder.",
        )
    })?;
    Ok(PathBuf::from(name))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn reveal_path(path: &Path) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn()
            .map_err(|_| {
                AppError::new(
                    ErrorCode::StorageError,
                    "Could not reveal the file in Finder.",
                )
            })?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.to_string_lossy()))
            .spawn()
            .map_err(|_| {
                AppError::new(
                    ErrorCode::StorageError,
                    "Could not reveal the file in Explorer.",
                )
            })?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let folder = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        open::that(folder).map_err(|_| {
            AppError::new(
                ErrorCode::StorageError,
                "Could not open the download folder.",
            )
        })
    }
}

fn complete_and_cleanup(
    inner: &ManagerInner,
    files: &ManagedFiles,
    root: &Path,
    task: &mut DownloadTask,
) -> Result<(), AppError> {
    task.status = DownloadStatus::Completed;
    task.error = None;
    task.speed_bytes_per_second = 0;
    task.finalization = Some("cleanup".into());
    task.bump_revision();
    inner.db.upsert_download(task)?; // Keep partial path until completed state is durable.
    if let Some(temp) = &task.temp_path {
        let segments = inner.db.list_segments(&task.id)?;
        for segment in &segments {
            let seg = segment_file_path(temp, segment.segment_index);
            if let Ok(name) = managed_name(root, &seg.to_string_lossy()) {
                files.remove(&name)?;
            }
        }
        files.remove(&managed_name(root, temp)?)?;
        inner.db.clear_segments(&task.id)?;
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
    let root = resolve_root(inner, &task.destination_id)?;
    let files = destination_files(inner, &task.destination_id)?;
    let final_name = managed_name(&root, final_path)?;
    let final_exists = files.length(&final_name)?.is_some();
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
            if files.digest(&final_name)? != digest {
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
            files.publish(&managed_name(&root, temp)?, &final_name, digest, None)?;
        }
        return complete_and_cleanup(inner, &files, &root, task);
    }
    if task.status == DownloadStatus::Completed {
        if !final_exists {
            return Err(AppError::new(
                ErrorCode::StorageNotFound,
                "The completed file is missing. Restore it or add a fresh download.",
            ));
        }
        if let Some(digest) = &task.actual_sha256
            && files.digest(&final_name)? != *digest
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
    if task.transfer_mode == TransferMode::Segmented {
        if let Some(temp) = task.temp_path.as_deref() {
            let segments = inner.db.list_segments(&task.id)?;
            if !coverage_contiguous(&segments, task.total_bytes.unwrap_or(0)) {
                return Err(AppError::new(
                    ErrorCode::ResourceChanged,
                    "The saved segment plan is inconsistent. Add a fresh download.",
                ));
            }
            if !segments.is_empty() {
                for segment in &segments {
                    let seg = segment_file_path(temp, segment.segment_index);
                    let on_disk = reconcile_segment_file(&files, &seg, segment)?;
                    if on_disk != segment.completed_bytes {
                        inner.db.update_segment_checkpoint(
                            &task.id,
                            segment.segment_index,
                            on_disk,
                            if on_disk == segment.expected_len() {
                                crate::domain::SegmentStatus::Complete
                            } else {
                                crate::domain::SegmentStatus::Pending
                            },
                        )?;
                    }
                }
                let segments = inner.db.list_segments(&task.id)?;
                task.downloaded_bytes = segments.iter().map(|s| s.completed_bytes).sum();
            }
        }
    } else if let Some(temp) = &task.temp_path {
        let length = files.length(&managed_name(&root, temp)?)?;
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
        task.queue_position = None;
        task.next_retry_at = None;
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

fn enqueue_runnable(inner: &ManagerInner, task: &mut DownloadTask) -> Result<(), AppError> {
    ensure_transition(task.status, DownloadStatus::Queued)?;
    task.status = DownloadStatus::Queued;
    task.queue_position = Some(inner.db.next_queue_position(&task.queue_id)?);
    if task.host.is_empty() {
        task.host = host_from_url(&task.url);
    }
    Ok(())
}

fn host_from_url(raw: &str) -> String {
    url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_else(|| "unknown".into())
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
