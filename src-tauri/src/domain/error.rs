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
    LengthMismatch,
    UrlNotAllowed,
    TaskNotFound,
    InvalidTransition,
    DataDirectoryInUse,
    NetworkConnection,
    NetworkInterrupted,
    TlsError,
    HttpNotFound,
    HttpClientError,
    HttpServerError,
    HttpRedirect,
    StoragePermission,
    StorageNotFound,
    StorageError,
    Internal,
}

impl ErrorCode {
    pub const ALL: &[Self] = &[
        Self::InvalidUrl,
        Self::UnsupportedScheme,
        Self::PathNotAllowed,
        Self::AlreadyExists,
        Self::DiskFull,
        Self::NetworkTimeout,
        Self::HttpUnauthorized,
        Self::HttpForbidden,
        Self::HttpRateLimit,
        Self::ResourceChanged,
        Self::RangeUnsupported,
        Self::ChecksumMismatch,
        Self::LengthMismatch,
        Self::UrlNotAllowed,
        Self::TaskNotFound,
        Self::InvalidTransition,
        Self::DataDirectoryInUse,
        Self::NetworkConnection,
        Self::NetworkInterrupted,
        Self::TlsError,
        Self::HttpNotFound,
        Self::HttpClientError,
        Self::HttpServerError,
        Self::HttpRedirect,
        Self::StoragePermission,
        Self::StorageNotFound,
        Self::StorageError,
        Self::Internal,
    ];

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
            Self::LengthMismatch => "LENGTH_MISMATCH",
            Self::UrlNotAllowed => "URL_NOT_ALLOWED",
            Self::TaskNotFound => "TASK_NOT_FOUND",
            Self::InvalidTransition => "INVALID_TRANSITION",
            Self::DataDirectoryInUse => "DATA_DIRECTORY_IN_USE",
            Self::NetworkConnection => "NETWORK_CONNECTION",
            Self::NetworkInterrupted => "NETWORK_INTERRUPTED",
            Self::TlsError => "TLS_ERROR",
            Self::HttpNotFound => "HTTP_NOT_FOUND",
            Self::HttpClientError => "HTTP_CLIENT_ERROR",
            Self::HttpServerError => "HTTP_SERVER_ERROR",
            Self::HttpRedirect => "HTTP_REDIRECT",
            Self::StoragePermission => "STORAGE_PERMISSION",
            Self::StorageNotFound => "STORAGE_NOT_FOUND",
            Self::StorageError => "STORAGE_ERROR",
            Self::Internal => "INTERNAL",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::NetworkTimeout
                | Self::NetworkConnection
                | Self::NetworkInterrupted
                | Self::HttpServerError
                | Self::StorageError
                | Self::HttpRateLimit
                | Self::DiskFull
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
    /// Seconds from Retry-After; never serialized to IPC.
    #[serde(skip)]
    pub retry_after_secs: Option<u64>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            retryable: code.retryable(),
            code,
            message: message.into(),
            retry_after_secs: None,
        }
    }

    pub fn with_retry_after(mut self, secs: Option<u64>) -> Self {
        self.retry_after_secs = secs;
        self
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
