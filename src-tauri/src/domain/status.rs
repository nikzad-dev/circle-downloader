use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    Connecting,
    Downloading,
    Paused,
    Verifying,
    Completed,
    Failed,
    Canceled,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Connecting => "connecting",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "connecting" => Some(Self::Connecting),
            "downloading" => Some(Self::Downloading),
            "paused" => Some(Self::Paused),
            "verifying" => Some(Self::Verifying),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "canceled" => Some(Self::Canceled),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Canceled)
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Connecting | Self::Downloading | Self::Verifying
        )
    }
}
