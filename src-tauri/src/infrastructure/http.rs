use crate::domain::{AppError, ErrorCode};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode, header};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub bytes_written: u64,
    pub total_bytes: Option<u64>,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, Default)]
pub struct ResumeMetadata {
    pub offset: u64,
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResponseMetadata {
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
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

    pub async fn cancelled(&self) {
        let mut rx = self.rx.clone();
        let _ = rx.wait_for(|value| *value).await;
    }
}

pub fn cancel_pair() -> (watch::Sender<bool>, CancelToken) {
    let (tx, rx) = watch::channel(false);
    (tx, CancelToken { rx })
}

fn canceled() -> AppError {
    AppError::new(ErrorCode::InvalidTransition, "Download canceled")
}

fn deadline() -> AppError {
    AppError::new(ErrorCode::NetworkTimeout, "The server stopped responding")
}

fn strong_etag(value: &str) -> bool {
    value.len() >= 2
        && value.starts_with('"')
        && value.ends_with('"')
        && value.as_bytes()[1..value.len() - 1]
            .iter()
            .all(|byte| *byte == 0x21 || (0x23..=0x7e).contains(byte) || *byte >= 0x80)
}

/// Validate representation identity and range before opening or changing a partial file.
fn validate_metadata(
    response: &reqwest::Response,
    resume: &ResumeMetadata,
) -> Result<ResponseMetadata, AppError> {
    let headers = response.headers();
    let etag = headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let last_modified = headers
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok());
    if encoding.is_some_and(|value| !value.eq_ignore_ascii_case("identity")) {
        return Err(AppError::new(
            ErrorCode::RangeUnsupported,
            "Encoded file responses cannot be safely verified",
        ));
    }
    let total_bytes = if resume.offset > 0 {
        // Last-Modified alone may be a weak validator (multiple updates in one second).
        // Until a Date/Last-Modified strength policy is persisted, require a strong ETag.
        let valid_identity = resume
            .etag
            .as_deref()
            .filter(|v| strong_etag(v))
            .is_some_and(|expected| etag.as_deref() == Some(expected));
        if !valid_identity {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The partial file has no matching strong validator; add a fresh download",
            ));
        }
        let range = headers
            .get(header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::RangeUnsupported,
                    "Missing Content-Range on resumed response",
                )
            })?;
        let total = validate_content_range(range, resume.offset, response.content_length())?;
        Some(total)
    } else {
        response.content_length()
    };
    Ok(ResponseMetadata {
        total_bytes,
        etag,
        last_modified,
    })
}

fn validate_content_range(value: &str, offset: u64, length: Option<u64>) -> Result<u64, AppError> {
    let invalid = || {
        AppError::new(
            ErrorCode::RangeUnsupported,
            "The server returned an incompatible byte range",
        )
    };
    let range = value.strip_prefix("bytes ").ok_or_else(invalid)?;
    let (span, total) = range.split_once('/').ok_or_else(invalid)?;
    let (start, end) = span.split_once('-').ok_or_else(invalid)?;
    let start: u64 = start.parse().map_err(|_| invalid())?;
    let end: u64 = end.parse().map_err(|_| invalid())?;
    let total: u64 = total.parse().map_err(|_| invalid())?;
    if start != offset || end < start || end >= total || end.checked_add(1) != Some(total) {
        return Err(invalid());
    }
    let expected = end
        .checked_sub(start)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(invalid)?;
    if length.is_some_and(|v| v != expected) {
        return Err(invalid());
    }
    Ok(total)
}

pub async fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    resume: ResumeMetadata,
    cancel: &CancelToken,
    mut on_metadata: impl FnMut(&ResponseMetadata) -> Result<(), AppError>,
    mut on_progress: impl FnMut(ProgressUpdate),
) -> Result<DownloadResult, AppError> {
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    let mut request = client.get(url).header(header::ACCEPT_ENCODING, "identity");
    if resume.offset > 0 {
        let validator = resume
            .etag
            .as_deref()
            .filter(|v| strong_etag(v))
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::ResourceChanged,
                    "This partial file cannot be resumed safely; add a fresh download",
                )
            })?;
        request = request
            .header(header::RANGE, format!("bytes={}-", resume.offset))
            .header(header::IF_RANGE, validator);
    }
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(canceled()),
        result = tokio::time::timeout(Duration::from_secs(30), request.send()) => result.map_err(|_| deadline())?.map_err(map_reqwest_err)?,
    };
    match response.status() {
        StatusCode::UNAUTHORIZED => {
            return Err(AppError::new(ErrorCode::HttpUnauthorized, "HTTP 401"));
        }
        StatusCode::FORBIDDEN => return Err(AppError::new(ErrorCode::HttpForbidden, "HTTP 403")),
        StatusCode::TOO_MANY_REQUESTS => {
            return Err(AppError::new(ErrorCode::HttpRateLimit, "HTTP 429"));
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            return Err(AppError::new(ErrorCode::RangeUnsupported, "HTTP 416"));
        }
        StatusCode::OK if resume.offset > 0 => {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The server ignored Range; add a fresh download",
            ));
        }
        StatusCode::PARTIAL_CONTENT if resume.offset == 0 => {
            return Err(AppError::new(
                ErrorCode::RangeUnsupported,
                "Unexpected partial response for a new download",
            ));
        }
        StatusCode::OK | StatusCode::PARTIAL_CONTENT => {}
        status => {
            return Err(AppError::new(
                ErrorCode::Internal,
                format!("Server returned HTTP {}", status.as_u16()),
            ));
        }
    }
    let metadata = validate_metadata(&response, &resume)?;
    on_metadata(&metadata)?;
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    let mut file = if resume.offset > 0 {
        let file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(dest)
            .await
            .map_err(map_io_err)?;
        if file.metadata().await.map_err(map_io_err)?.len() != resume.offset {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The partial file changed while opening it",
            ));
        }
        file
    } else {
        File::create(dest).await.map_err(map_io_err)?
    };
    let mut stream = response.bytes_stream();
    let mut written = resume.offset;
    let mut hasher = Sha256::new();
    let started = Instant::now();
    let mut last_tick = started;
    let transfer = async {
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = tokio::time::timeout(Duration::from_secs(30), stream.next()) => result.map_err(|_| deadline())?,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(map_reqwest_err)?;
        let next_written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| AppError::new(ErrorCode::ChecksumMismatch, "File size overflow"))?;
        if metadata
            .total_bytes
            .is_some_and(|total| next_written > total)
        {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "Response exceeded the declared file size",
            ));
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = file.write_all(&chunk) => result.map_err(map_io_err)?,
        }
        hasher.update(&chunk);
        written = next_written;
        if last_tick.elapsed().as_millis() >= 100 {
            on_progress(ProgressUpdate {
                transferred_bytes: written,
                total_bytes: metadata.total_bytes,
                bytes_per_second: ((written - resume.offset) as f64
                    / started.elapsed().as_secs_f64().max(0.001))
                    as u64,
            });
            last_tick = Instant::now();
        }
    }
    if metadata.total_bytes.is_some_and(|total| written != total) {
        return Err(AppError::new(
            ErrorCode::ChecksumMismatch,
            "The response ended before all expected bytes arrived",
        ));
    }
    Ok::<(), AppError>(())
    }.await;
    // Drain pending Tokio filesystem work even on cancellation or network failure.
    file.flush().await.map_err(map_io_err)?;
    transfer?;
    file.sync_all().await.map_err(map_io_err)?;
    drop(file);
    let sha256_hex = if resume.offset == 0 {
        hex::encode(hasher.finalize())
    } else {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = hash_file(dest) => result?,
        }
    };
    on_progress(ProgressUpdate {
        transferred_bytes: written,
        total_bytes: metadata.total_bytes,
        bytes_per_second: 0,
    });
    Ok(DownloadResult {
        bytes_written: written,
        total_bytes: metadata.total_bytes,
        sha256_hex,
    })
}

pub async fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = File::open(path).await.map_err(map_io_err)?;
    let mut buffer = [0; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let size = file.read(&mut buffer).await.map_err(map_io_err)?;
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn map_reqwest_err(err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        deadline()
    } else {
        AppError::new(
            ErrorCode::Internal,
            "The connection failed or the response was interrupted",
        )
    }
}

pub(crate) fn map_io_err(err: std::io::Error) -> AppError {
    if matches!(err.raw_os_error(), Some(28 | 112))
        || err.to_string().to_lowercase().contains("no space")
    {
        AppError::new(
            ErrorCode::DiskFull,
            "There is not enough disk space to save this file",
        )
    } else {
        AppError::new(
            ErrorCode::PathNotAllowed,
            "The file could not be read or written; check folder permissions",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incompatible_content_ranges() {
        for range in [
            "bytes 0-9/10",
            "bytes 5-9/*",
            "bytes 5-10/10",
            "bytes 5-8/10",
            "bytes 9-5/10",
            "garbage",
            "bytes 5-18446744073709551615/0",
        ] {
            assert!(validate_content_range(range, 5, None).is_err(), "{range}");
        }
        assert_eq!(
            validate_content_range("bytes 5-9/10", 5, Some(5)).unwrap(),
            10
        );
        assert!(validate_content_range("bytes 5-9/10", 5, Some(4)).is_err());
    }

    #[tokio::test]
    async fn hashes_multiple_buffers_and_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.bin");
        let body: Vec<u8> = (0..200_000).map(|index| (index % 251) as u8).collect();
        tokio::fs::write(&path, &body).await.unwrap();
        assert_eq!(
            hash_file(&path).await.unwrap(),
            hex::encode(Sha256::digest(&body))
        );
        tokio::fs::write(&path, []).await.unwrap();
        assert_eq!(
            hash_file(&path).await.unwrap(),
            hex::encode(Sha256::digest([]))
        );
    }

    async fn serve(response: Vec<u8>, stall: bool) -> (String, tokio::task::JoinHandle<String>) {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/fixture.bin", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|v| v == b"\r\n\r\n") {
                let size = socket.read(&mut buffer).await.unwrap();
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..size]);
            }
            let _ = socket.write_all(&response).await;
            if stall {
                std::future::pending::<()>().await;
            }
            String::from_utf8(request).unwrap()
        });
        (url, server)
    }

    #[tokio::test]
    async fn resumes_only_the_matching_range_and_hashes_the_whole_file() {
        let (url, server) = serve(b"HTTP/1.1 206 Partial Content\r\nETag: \"same\"\r\nContent-Range: bytes 5-10/11\r\nContent-Length: 6\r\nConnection: close\r\n\r\n world".to_vec(), false).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.part");
        tokio::fs::write(&path, b"hello").await.unwrap();
        let (_tx, cancel) = cancel_pair();
        let mut accepted_metadata = false;
        let result = download_to_file(
            &Client::new(),
            &url,
            &path,
            ResumeMetadata {
                offset: 5,
                etag: Some("\"same\"".into()),
            },
            &cancel,
            |metadata| {
                assert_eq!(metadata.total_bytes, Some(11));
                accepted_metadata = true;
                Ok(())
            },
            |_| {},
        )
        .await
        .unwrap();
        assert!(accepted_metadata);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"hello world");
        assert_eq!(
            result.sha256_hex,
            hex::encode(Sha256::digest(b"hello world"))
        );
        assert_eq!(result.bytes_written, 11);
        let request = server.await.unwrap().to_lowercase();
        assert!(request.contains("range: bytes=5-"));
        assert!(request.contains("if-range: \"same\""));
        assert!(request.contains("accept-encoding: identity"));
    }

    #[tokio::test]
    async fn invalid_resume_responses_never_modify_the_partial_file() {
        for headers in [
            "ETag: \"same\"\r\nContent-Range: bytes 0-5/6\r\n",
            "ETag: \"same\"\r\n",
            "ETag: \"different\"\r\nContent-Range: bytes 5-10/11\r\n",
            "Content-Range: bytes 5-10/11\r\n",
            "ETag: \"same\"\r\nContent-Range: bytes 5-9/11\r\n",
            "ETag: \"same\"\r\nContent-Range: bytes 5-10/11\r\nContent-Encoding: gzip\r\n",
        ] {
            let response = format!(
                "HTTP/1.1 206 Partial Content\r\n{headers}Content-Length: 6\r\nConnection: close\r\n\r\n world"
            );
            let (url, server) = serve(response.into_bytes(), false).await;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            tokio::fs::write(&path, b"hello").await.unwrap();
            let (_tx, cancel) = cancel_pair();
            let result = download_to_file(
                &Client::new(),
                &url,
                &path,
                ResumeMetadata {
                    offset: 5,
                    etag: Some("\"same\"".into()),
                },
                &cancel,
                |_| panic!("Invalid metadata must not be accepted"),
                |_| {},
            )
            .await;
            assert!(result.is_err(), "{headers}");
            assert_eq!(tokio::fs::read(&path).await.unwrap(), b"hello");
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn weak_or_absent_validators_refuse_resume_before_network_access() {
        for etag in [None, Some("W/\"weak\"".into())] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            tokio::fs::write(&path, b"safe").await.unwrap();
            let (_tx, cancel) = cancel_pair();
            let error = download_to_file(
                &Client::new(),
                "http://127.0.0.1:1/fixture",
                &path,
                ResumeMetadata { offset: 4, etag },
                &cancel,
                |_| Ok(()),
                |_| {},
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::ResourceChanged);
            assert_eq!(tokio::fs::read(&path).await.unwrap(), b"safe");
        }
    }

    #[tokio::test]
    async fn accepts_zero_and_unknown_totals_but_rejects_truncated_bodies() {
        for (response, expected) in [
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                Some(Vec::new()),
            ),
            (
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nunknown".to_vec(),
                Some(b"unknown".to_vec()),
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nshort".to_vec(),
                None,
            ),
        ] {
            let (url, server) = serve(response, false).await;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            let (_tx, cancel) = cancel_pair();
            let result = download_to_file(
                &Client::new(),
                &url,
                &path,
                ResumeMetadata::default(),
                &cancel,
                |_| Ok(()),
                |_| {},
            )
            .await;
            if let Some(expected) = expected {
                assert_eq!(
                    result.unwrap().sha256_hex,
                    hex::encode(Sha256::digest(&expected))
                );
                assert_eq!(tokio::fs::read(&path).await.unwrap(), expected);
            } else {
                assert!(result.is_err());
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_interrupts_waiting_for_headers_and_stalled_body() {
        for response in [
            Vec::new(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n".to_vec(),
        ] {
            let (url, server) = serve(response, true).await;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            let (tx, cancel) = cancel_pair();
            let cancel_task = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                tx.send(true).unwrap();
            });
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                download_to_file(
                    &Client::new(),
                    &url,
                    &path,
                    ResumeMetadata::default(),
                    &cancel,
                    |_| Ok(()),
                    |_| {},
                ),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(result.code, ErrorCode::InvalidTransition);
            cancel_task.await.unwrap();
            server.abort();
        }
    }
}
