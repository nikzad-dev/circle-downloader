use super::error::ErrorCode;
use super::status::DownloadStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    pub id: String,
    #[serde(serialize_with = "serialize_redacted_url")]
    pub url: String,
    pub file_name: String,
    pub destination_id: String,
    pub status: DownloadStatus,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_second: u64,
    pub revision: u64,
    pub error: Option<TaskError>,
    pub expected_sha256: Option<String>,
    pub actual_sha256: Option<String>,
    /// computed, matched, or mismatch; length validation is independent.
    pub checksum_result: Option<String>,
    #[serde(skip)]
    pub finalization: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub temp_path: Option<String>,
    #[serde(skip)]
    pub final_path: Option<String>,
    #[serde(skip)]
    pub etag: Option<String>,
    #[serde(skip)]
    pub last_modified: Option<String>,
    #[serde(skip)]
    pub request_id: Option<String>,
    #[serde(skip)]
    pub retry_count: u32,
}

impl DownloadTask {
    pub fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDownloadRequest {
    pub url: String,
    pub destination_id: Option<String>,
    pub suggested_name: Option<String>,
    #[serde(default = "default_start_now")]
    pub start_now: bool,
    pub request_id: Option<String>,
    pub expected_sha256: Option<String>,
}

fn default_start_now() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub task_id: String,
    pub revision: u64,
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
    pub status: DownloadStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub max_concurrent_downloads: u32,
    pub default_destination_id: String,
    pub auto_start: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: 3,
            default_destination_id: "default".to_string(),
            auto_start: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub max_concurrent_downloads: Option<u32>,
    pub default_destination_id: Option<String>,
    pub auto_start: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub id: String,
    pub name: String,
    pub task_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDownloadRequest {
    pub id: String,
    #[serde(default = "default_delete_partial")]
    pub delete_partial: bool,
}

fn default_delete_partial() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveDownloadResult {
    pub removed: bool,
}

fn serialize_redacted_url<S: serde::Serializer>(
    raw: &str,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut url = url::Url::parse(raw).map_err(serde::ser::Error::custom)?;
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    serializer.serialize_str(url.as_str())
}

pub fn normalize_sha256(value: Option<String>) -> Result<Option<String>, super::AppError> {
    value
        .map(|value| {
            let value = value.trim();
            if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(super::AppError::new(
                    ErrorCode::ChecksumMismatch,
                    "Expected SHA-256 must contain exactly 64 hexadecimal characters.",
                ));
            }
            Ok(value.to_ascii_lowercase())
        })
        .transpose()
}
