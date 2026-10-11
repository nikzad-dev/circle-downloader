use crate::domain::{AppError, ErrorCode};
use chrono::DateTime;
use reqwest::StatusCode;
use std::error::Error;
use std::io;

/// Never expose source-chain strings: they may contain signed URLs, credentials or local paths.
pub fn map_reqwest_err(error: reqwest::Error) -> AppError {
    if has_policy_source(&error) {
        super::network::policy_error()
    } else if error.is_timeout() {
        network_deadline("The server did not respond in time. Check your connection and retry.")
    } else if has_tls_source(&error) {
        AppError::new(
            ErrorCode::TlsError,
            "The secure connection could not be verified. Check your system clock or contact the site owner about its TLS configuration.",
        )
    } else if error.is_redirect() {
        AppError::new(
            ErrorCode::HttpRedirect,
            "The link has too many redirects or an invalid redirect. Use a direct file link.",
        )
    } else if error.is_body() || error.is_decode() {
        AppError::new(
            ErrorCode::NetworkInterrupted,
            "The connection ended before the download finished. Check your connection and retry.",
        )
    } else if error.is_connect() {
        AppError::new(
            ErrorCode::NetworkConnection,
            "Could not reach the server. Check your connection and the download host, then retry.",
        )
    } else if error.is_builder() {
        AppError::new(
            ErrorCode::InvalidUrl,
            "This download request is invalid. Check the link.",
        )
    } else {
        AppError::new(
            ErrorCode::NetworkInterrupted,
            "The download request failed before completion. Check the connection and retry.",
        )
    }
}

fn has_policy_source(mut source: &(dyn Error + 'static)) -> bool {
    loop {
        if source
            .downcast_ref::<super::network::AddressBlocked>()
            .is_some()
        {
            return true;
        }
        if let Some(error) = source.downcast_ref::<io::Error>()
            && let Some(inner) = error.get_ref()
            && has_policy_source(inner)
        {
            return true;
        }
        match source.source() {
            Some(next) => source = next,
            None => return false,
        }
    }
}

fn has_tls_source(mut source: &(dyn Error + 'static)) -> bool {
    loop {
        if source.downcast_ref::<rustls::Error>().is_some() {
            return true;
        }
        if let Some(error) = source.downcast_ref::<io::Error>()
            && let Some(inner) = error.get_ref()
            && has_tls_source(inner)
        {
            return true;
        }
        match source.source() {
            Some(next) => source = next,
            None => return false,
        }
    }
}

pub fn network_deadline(message: &'static str) -> AppError {
    AppError::new(ErrorCode::NetworkTimeout, message)
}

pub fn http_status_error_with_retry_after(
    status: StatusCode,
    retry_after: Option<&reqwest::header::HeaderValue>,
) -> AppError {
    let (code, action) = match status {
        StatusCode::UNAUTHORIZED => (
            ErrorCode::HttpUnauthorized,
            "This link requires sign-in. Obtain a direct download link.",
        ),
        StatusCode::PROXY_AUTHENTICATION_REQUIRED => (
            ErrorCode::HttpUnauthorized,
            "The proxy requires authorization. Check its scoped credential grant in Settings.",
        ),
        StatusCode::FORBIDDEN => (
            ErrorCode::HttpForbidden,
            "The server denied access. Check the link and its permissions.",
        ),
        StatusCode::NOT_FOUND | StatusCode::GONE => (
            ErrorCode::HttpNotFound,
            "This file is no longer available. Obtain a new link.",
        ),
        StatusCode::REQUEST_TIMEOUT => (
            ErrorCode::NetworkTimeout,
            "The server timed out. Check your connection and retry.",
        ),
        StatusCode::TOO_MANY_REQUESTS => (
            ErrorCode::HttpRateLimit,
            "The server is limiting requests. Wait before retrying.",
        ),
        StatusCode::RANGE_NOT_SATISFIABLE => (
            ErrorCode::RangeUnsupported,
            "The server cannot continue this partial file. Add a fresh download.",
        ),
        status if status.is_server_error() => (
            ErrorCode::HttpServerError,
            "The server failed to handle the request. Wait and retry.",
        ),
        status if status.is_redirection() => (
            ErrorCode::HttpRedirect,
            "The server did not provide a usable redirect. Use a direct file link.",
        ),
        _ => (
            ErrorCode::HttpClientError,
            "The server rejected the request. Check the link or obtain a new one.",
        ),
    };
    AppError::new(code, format!("HTTP {}: {action}", status.as_u16()))
        .with_retry_after(parse_retry_after(retry_after))
}

pub fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    let raw = value?.to_str().ok()?.trim();
    if let Ok(secs) = raw.parse::<u64>() {
        return Some(secs.min(3600));
    }
    DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|dt| {
            let now = chrono::Utc::now();
            let target = dt.with_timezone(&chrono::Utc);
            if target <= now {
                0
            } else {
                (target - now).num_seconds().max(0) as u64
            }
        })
        .map(|secs| secs.min(3600))
}

pub fn map_io_err(error: io::Error) -> AppError {
    let (code, message) = match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => (
            ErrorCode::DiskFull,
            "Free some disk space, then retry the download.",
        ),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => (
            ErrorCode::StoragePermission,
            "Circle cannot write to this folder. Check its permissions or read-only status.",
        ),
        io::ErrorKind::NotFound => (
            ErrorCode::StorageNotFound,
            "The folder or partial file is missing. Restore it or add a fresh download.",
        ),
        io::ErrorKind::AlreadyExists => (
            ErrorCode::AlreadyExists,
            "The destination is already in use. Choose another filename.",
        ),
        _ if is_disk_full_os_error(&error) => (
            ErrorCode::DiskFull,
            "Free some disk space, then retry the download.",
        ),
        _ => (
            ErrorCode::StorageError,
            "The storage operation failed. Check the drive is connected and working, then retry.",
        ),
    };
    AppError::new(code, message)
}

fn is_disk_full_os_error(error: &io::Error) -> bool {
    #[cfg(windows)]
    let disk_full = 112;
    #[cfg(unix)]
    let disk_full = 28;
    #[cfg(not(any(windows, unix)))]
    return false;
    #[cfg(any(windows, unix))]
    {
        error.raw_os_error() == Some(disk_full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_errors_are_actionable_and_redacted() {
        for (kind, code, retryable) in [
            (
                io::ErrorKind::PermissionDenied,
                ErrorCode::StoragePermission,
                false,
            ),
            (
                io::ErrorKind::ReadOnlyFilesystem,
                ErrorCode::StoragePermission,
                false,
            ),
            (io::ErrorKind::NotFound, ErrorCode::StorageNotFound, false),
            (io::ErrorKind::StorageFull, ErrorCode::DiskFull, true),
            (io::ErrorKind::QuotaExceeded, ErrorCode::DiskFull, true),
            (
                io::ErrorKind::AlreadyExists,
                ErrorCode::AlreadyExists,
                false,
            ),
            (io::ErrorKind::Other, ErrorCode::StorageError, true),
        ] {
            let error = map_io_err(io::Error::new(kind, "secret/path?token=private"));
            assert_eq!(error.code, code);
            assert_eq!(error.retryable, retryable);
            assert!(!error.message.contains("secret"));
        }
        #[cfg(windows)]
        assert_eq!(
            map_io_err(io::Error::from_raw_os_error(112)).code,
            ErrorCode::DiskFull
        );
        #[cfg(unix)]
        {
            assert_eq!(
                map_io_err(io::Error::from_raw_os_error(28)).code,
                ErrorCode::DiskFull
            );
            assert_ne!(
                map_io_err(io::Error::from_raw_os_error(112)).code,
                ErrorCode::DiskFull
            );
        }
    }

    #[test]
    fn detects_typed_tls_errors_through_io_wrappers() {
        let error = io::Error::other(rustls::Error::InvalidCertificate(
            rustls::CertificateError::Expired,
        ));
        assert!(has_tls_source(&error));
        assert!(!has_tls_source(&io::Error::other(
            "certificate words are not a typed TLS error"
        )));
    }
}
