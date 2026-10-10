use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidUrl,
    UnsupportedScheme,
    PathNotAllowed,
    AlreadyExists,
    DiskFull,
    NetworkTimeout,
    HttpUnauthorized,
    HttpForbidden,
    HttpRateLimit,
    ResourceChanged,
    RangeUnsupported,
    ChecksumMismatch,
    TaskNotFound,
    InvalidTransition,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidUrl => "INVALID_URL",
            Self::UnsupportedScheme => "UNSUPPORTED_SCHEME",
            Self::PathNotAllowed => "PATH_NOT_ALLOWED",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::DiskFull => "DISK_FULL",
            Self::NetworkTimeout => "NETWORK_TIMEOUT",
            Self::HttpUnauthorized => "HTTP_UNAUTHORIZED",
            Self::HttpForbidden => "HTTP_FORBIDDEN",
            Self::HttpRateLimit => "HTTP_RATE_LIMIT",
            Self::ResourceChanged => "RESOURCE_CHANGED",
            Self::RangeUnsupported => "RANGE_UNSUPPORTED",
            Self::ChecksumMismatch => "CHECKSUM_MISMATCH",
            Self::TaskNotFound => "TASK_NOT_FOUND",
            Self::InvalidTransition => "INVALID_TRANSITION",
            Self::Internal => "INTERNAL",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::NetworkTimeout
                | Self::HttpRateLimit
                | Self::DiskFull
                | Self::ResourceChanged
                | Self::Internal
        )
    }
}

#[derive(Debug, Error, Clone, Serialize, Deserialize)]
#[error("{message}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            retryable: code.retryable(),
            code,
            message: message.into(),
        }
    }

    pub fn to_ipc_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"code":"INTERNAL","message":"serialization failed","retryable":false}"#.to_string()
        })
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.to_ipc_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_codes() {
        let err = AppError::new(ErrorCode::TaskNotFound, "missing");
        let json = err.to_ipc_string();
        assert!(json.contains("TASK_NOT_FOUND"));
        assert!(json.contains("\"retryable\":false"));
    }
}
