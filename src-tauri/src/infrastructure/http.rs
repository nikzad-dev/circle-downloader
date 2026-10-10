use crate::domain::{AppError, ErrorCode};
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Instant;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub bytes_written: u64,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sha256_hex: String,
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
}

pub struct CancelToken {
    rx: watch::Receiver<bool>,
}

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }
}

pub fn cancel_pair() -> (watch::Sender<bool>, CancelToken) {
    let (tx, rx) = watch::channel(false);
    (tx, CancelToken { rx })
}

pub async fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    resume_from: u64,
    expected_etag: Option<&str>,
    cancel: &CancelToken,
    mut on_progress: impl FnMut(ProgressUpdate),
) -> Result<DownloadResult, AppError> {
    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header(header::RANGE, format!("bytes={resume_from}-"));
        if let Some(etag) = expected_etag {
            request = request.header(header::IF_RANGE, etag);
        }
    }

    let response = request.send().await.map_err(map_reqwest_err)?;
    let status = response.status();

    match status {
        StatusCode::UNAUTHORIZED => {
            return Err(AppError::new(ErrorCode::HttpUnauthorized, "HTTP 401"));
        }
        StatusCode::FORBIDDEN => {
            return Err(AppError::new(ErrorCode::HttpForbidden, "HTTP 403"));
        }
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(AppError::new(ErrorCode::HttpRateLimit, "HTTP 429"));
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            return Err(AppError::new(
                ErrorCode::RangeUnsupported,
                "HTTP 416 Range Not Satisfiable",
            ));
        }
        _ => {}
    }

    // Resume requested but server returned full body — never append.
    if resume_from > 0 && status == StatusCode::OK {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "Server ignored Range and returned HTTP 200; refusing to append",
        ));
    }

    if resume_from > 0 && status != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::new(
            ErrorCode::RangeUnsupported,
            format!("Expected HTTP 206 for resume, got {status}"),
        ));
    }

    if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::new(
            ErrorCode::Internal,
            format!("Unexpected HTTP status: {status}"),
        ));
    }

    let etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    if resume_from > 0 {
        if let (Some(expected), Some(actual)) = (expected_etag, etag.as_deref()) {
            if expected != actual {
                return Err(AppError::new(
                    ErrorCode::ResourceChanged,
                    "ETag changed since partial download",
                ));
            }
        }
    }

    let content_length = response.content_length();
    let total_bytes = match (resume_from, content_length) {
        (0, Some(len)) => Some(len),
        (from, Some(len)) => Some(from + len),
        (_, None) => None,
    };

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(map_io_err)?;
    }

    let mut file = if resume_from > 0 {
        // Caller must have truncated/reset if ResourceChanged; append only after 206.
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await
            .map_err(map_io_err)?
    } else {
        File::create(dest).await.map_err(map_io_err)?
    };

    let mut stream = response.bytes_stream();
    let mut written = resume_from;
    let mut hasher = Sha256::new();
    if resume_from > 0 {
        // For resume tests we only hash new bytes; full-file verify is caller's job when needed.
    }
    let started = Instant::now();
    let mut last_tick = started;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Download canceled",
            ));
        }
        let chunk = chunk.map_err(map_reqwest_err)?;
        file.write_all(&chunk).await.map_err(map_io_err)?;
        hasher.update(&chunk);
        written += chunk.len() as u64;

        let now = Instant::now();
        if now.duration_since(last_tick).as_millis() >= 100 {
            let elapsed = started.elapsed().as_secs_f64().max(0.001);
            let speed = ((written.saturating_sub(resume_from)) as f64 / elapsed) as u64;
            on_progress(ProgressUpdate {
                transferred_bytes: written,
                total_bytes,
                bytes_per_second: speed,
            });
            last_tick = now;
        }
    }

    file.flush().await.map_err(map_io_err)?;

    let sha256_hex = if resume_from == 0 {
        hex::encode(hasher.finalize())
    } else {
        // Re-hash full file for correctness after resume.
        hash_file(dest).await?
    };

    on_progress(ProgressUpdate {
        transferred_bytes: written,
        total_bytes,
        bytes_per_second: 0,
    });

    Ok(DownloadResult {
        bytes_written: written,
        total_bytes,
        etag,
        last_modified,
        sha256_hex,
    })
}

pub async fn hash_file(path: &Path) -> Result<String, AppError> {
    let bytes = tokio::fs::read(path).await.map_err(map_io_err)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn map_reqwest_err(err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        AppError::new(ErrorCode::NetworkTimeout, err.to_string())
    } else {
        AppError::new(ErrorCode::Internal, format!("HTTP error: {err}"))
    }
}

fn map_io_err(err: std::io::Error) -> AppError {
    if err.raw_os_error() == Some(28) || err.to_string().to_lowercase().contains("no space") {
        AppError::new(ErrorCode::DiskFull, err.to_string())
    } else {
        AppError::new(ErrorCode::PathNotAllowed, format!("IO error: {err}"))
    }
}
