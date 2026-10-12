use super::error::ErrorCode;
use super::status::DownloadStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransferMode {
    #[default]
    Single,
    Segmented,
}

impl TransferMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Segmented => "segmented",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "segmented" => Self::Segmented,
            _ => Self::Single,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentStatus {
    Pending,
    Active,
    Complete,
    Failed,
}

impl SegmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSegment {
    pub task_id: String,
    pub segment_index: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub completed_bytes: u64,
    pub status: SegmentStatus,
    pub updated_at: DateTime<Utc>,
}

impl DownloadSegment {
    pub fn expected_len(&self) -> u64 {
        self.end_byte
            .saturating_sub(self.start_byte)
            .saturating_add(1)
    }

    pub fn remaining(&self) -> u64 {
        self.expected_len().saturating_sub(self.completed_bytes)
    }
}

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
    pub transfer_mode: TransferMode,
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
    pub retry_count: u32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub queue_id: String,
    #[serde(skip)]
    pub queue_position: Option<i64>,
    #[serde(skip)]
    pub host: String,
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
    /// Opt-in multi-connection fan-out per download (1 = single-connection).
    pub max_connections_per_download: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_concurrent_downloads: 3,
            default_destination_id: "default".to_string(),
            auto_start: true,
            max_connections_per_download: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub max_concurrent_downloads: Option<u32>,
    pub default_destination_id: Option<String>,
    pub auto_start: Option<bool>,
    pub max_connections_per_download: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub id: String,
    pub name: String,
    pub task_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    pub label: String,
    /// Redacted display path; never includes secrets.
    pub display_path: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantDestinationRequest {
    pub path: String,
    pub label: Option<String>,
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
