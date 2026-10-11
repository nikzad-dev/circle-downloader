use super::*;
use crate::domain::advanced::*;
use crate::infrastructure::{credentials, network};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkImport {
    pub urls: Vec<String>,
    pub options: TaskOptions,
    pub start_now: bool,
    pub batch_id: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkResult {
    pub created: Vec<DownloadTask>,
    pub errors: Vec<String>,
}

impl DownloadManager {
    pub async fn create_configured(
        &self,
        mut input: CreateDownloadRequest,
        mut options: TaskOptions,
    ) -> Result<DownloadTask, AppError> {
        options.validate()?;
        let url = validate_download_url(&input.url)?;
        if !self
            .queue_policies()?
            .iter()
            .any(|q| q.id == options.queue_id)
        {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Choose an existing queue.",
            ));
        }
        if let Some(id) = &options.credential_id {
            validate_grant(&self.inner.db, id, "origin", url.as_str())?;
        }
        let start = input.start_now && self.get_settings()?.auto_start;
        input.start_now = false; // Persist controls before dispatch can begin.
        let task = self.create_in_category(input, &options.category)?;
        if task.status == DownloadStatus::Paused {
            options.id = task.id.clone();
            self.update_task_options(options)?;
            if start {
                self.resume(&task.id).await?;
            }
        }
        self.get(&task.id)
    }

    pub fn task_options(&self) -> Result<Vec<TaskOptions>, AppError> {
        self.inner.db.all_task_options()
    }
    pub fn update_task_options(&self, mut options: TaskOptions) -> Result<TaskOptions, AppError> {
        let dispatch_guard = self.inner.dispatch_lock.lock();
        let _mutation = self.inner.mutations.lock();
        if self.inner.workers.lock().contains_key(&options.id) {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Pause the download before changing its queue, schedule or authorization.",
            ));
        }
        let task = self.inner.db.get_download(&options.id)?;
        options.validate()?;
        if let Some(id) = &options.credential_id {
            validate_grant(&self.inner.db, id, "origin", &task.url)?;
        }
        self.inner.db.save_task_options(&options)?;
        drop(_mutation);
        drop(dispatch_guard);
        self.dispatch();
        self.inner.db.task_options(&options.id)
    }
    pub fn queue_policies(&self) -> Result<Vec<QueuePolicy>, AppError> {
        self.inner.db.queue_policies()
    }
    pub async fn save_queue(&self, mut policy: QueuePolicy) -> Result<QueuePolicy, AppError> {
        policy.validate()?;
        if policy.id.is_empty() {
            policy.id = Uuid::new_v4().to_string();
        }
        {
            let _dispatch = self.inner.dispatch_lock.lock();
            self.inner.db.save_queue(&policy)?;
        }
        if let Some(limit) = self.inner.queue_limits.lock().get(&policy.id) {
            limit.set_rate(policy.bytes_per_second);
        }
        self.dispatch();
        Ok(policy)
    }
    pub fn network_settings(&self) -> Result<NetworkSettings, AppError> {
        self.inner.db.network_settings()
    }
    pub fn save_network(&self, settings: NetworkSettings) -> Result<NetworkSettings, AppError> {
        settings.validate()?;
        let _dispatch = self.inner.dispatch_lock.lock();
        let current = self.inner.db.network_settings()?;
        if !self.inner.workers.lock().is_empty()
            && (current.proxy_url != settings.proxy_url
                || current.proxy_credential_id != settings.proxy_credential_id
                || current.connect_timeout_secs != settings.connect_timeout_secs)
        {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Pause active downloads before changing proxy or connection settings.",
            ));
        }
        if let Some(id) = &settings.proxy_credential_id {
            validate_grant(&self.inner.db, id, "proxy", &settings.proxy_url)?;
        }
        // Construct first: failed configuration never replaces saved settings.
        let secret = settings
            .proxy_credential_id
            .as_deref()
            .map(|id| credentials::load(&self.inner.db, id, "proxy", &settings.proxy_url))
            .transpose()?;
        network::configured_client(
            &settings,
            secret.as_ref(),
            None,
            None,
            cfg!(debug_assertions),
        )?;
        self.inner.db.save_network(&settings)?;
        self.inner.global_limit.set_rate(settings.bytes_per_second);
        Ok(settings)
    }
    pub fn grants(&self) -> Result<Vec<CredentialGrant>, AppError> {
        self.inner.db.grants()
    }
    pub fn save_credential(&self, input: SaveCredential) -> Result<CredentialGrant, AppError> {
        credentials::save(&self.inner.db, input)
    }
    pub fn revoke_credential(&self, id: &str) -> Result<(), AppError> {
        let _dispatch = self.inner.dispatch_lock.lock();
        if !self.inner.workers.lock().is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Pause active downloads before revoking credential authorization.",
            ));
        }
        credentials::revoke(&self.inner.db, id)
    }
    pub fn statistics(&self) -> Result<Statistics, AppError> {
        self.inner.db.statistics()
    }
    pub async fn bulk_import(&self, input: BulkImport) -> Result<BulkResult, AppError> {
        if input.urls.is_empty()
            || input.urls.len() > 200
            || input.batch_id.is_empty()
            || input.batch_id.len() > 80
        {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Import 1–200 URLs and provide a batch identifier.",
            ));
        }
        let mut options = input.options;
        options.validate()?;
        if options.queue_id.is_empty() {
            options.queue_id = "default".into();
        }
        if !self
            .queue_policies()?
            .iter()
            .any(|q| q.id == options.queue_id)
        {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Choose an existing queue.",
            ));
        }
        let urls = input
            .urls
            .into_iter()
            .map(|raw| validate_download_url(raw.trim()).map(|u| u.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        for url in &urls {
            if let Some(id) = &options.credential_id {
                validate_grant(&self.inner.db, id, "origin", url)?;
            }
        }
        let start = input.start_now && self.get_settings()?.auto_start;
        let mut result = BulkResult {
            created: vec![],
            errors: vec![],
        };
        for (index, url) in urls.into_iter().enumerate() {
            let create = self.create_in_category(
                CreateDownloadRequest {
                    url,
                    destination_id: None,
                    suggested_name: None,
                    start_now: false,
                    request_id: Some(format!("{}:{index}", input.batch_id)),
                    expected_sha256: None,
                },
                &options.category,
            );
            match create {
                Ok(task) => {
                    // Retry of a partially imported batch must not reconfigure or restart existing work.
                    if task.revision == 1 && task.status == DownloadStatus::Paused {
                        let mut value = options.clone();
                        value.id = task.id.clone();
                        if let Err(e) = self.update_task_options(value) {
                            result
                                .errors
                                .push(format!("Line {}: {}", index + 1, e.message));
                            result.created.push(task);
                            continue;
                        }
                        if start && let Err(e) = self.resume(&task.id).await {
                            result
                                .errors
                                .push(format!("Line {}: {}", index + 1, e.message));
                        }
                    }
                    result.created.push(self.get(&task.id)?);
                }
                Err(e) => result
                    .errors
                    .push(format!("Line {}: {}", index + 1, e.message)),
            }
        }
        Ok(result)
    }
}
fn validate_grant(db: &Database, id: &str, kind: &str, url: &str) -> Result<(), AppError> {
    if !db
        .grants()?
        .iter()
        .any(|g| g.id == id && authorized(g, kind, url))
    {
        return Err(AppError::new(
            ErrorCode::UrlNotAllowed,
            "Choose a credential authorized for this exact origin.",
        ));
    }
    Ok(())
}
