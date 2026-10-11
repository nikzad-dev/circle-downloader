use super::errors::{
    http_status_error_with_retry_after, map_io_err, map_reqwest_err, network_deadline,
};
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
    pub verified_file: std::sync::Arc<super::files::VerifiedFile>,
}

#[derive(Debug, Clone, Default)]
pub struct ResumeMetadata {
    pub effective_url: Option<String>,
    pub offset: u64,
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResponseMetadata {
    pub effective_url: String,
    pub total_bytes: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// Inclusive byte range for a segmented transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }
}

#[derive(Debug, Clone)]
pub struct SegmentCapability {
    pub effective_url: String,
    pub total_bytes: u64,
    pub etag: String,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
    pub verifying: bool,
}

pub struct CancelToken {
    rx: watch::Receiver<bool>,
    pub limits: Vec<std::sync::Arc<super::rate_limit::RateLimiter>>,
    pub measured_bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl CancelToken {
    pub async fn throttle(&self, bytes: usize) -> Result<(), AppError> {
        for limit in &self.limits {
            if !limit.consume(bytes, self).await {
                return Err(canceled());
            }
        }
        Ok(())
    }
    pub fn inherit_limits(&mut self, parent: &Self) {
        self.limits = parent.limits.clone();
        self.measured_bytes = parent.measured_bytes.clone();
    }
    pub fn record_bytes(&self, bytes: usize) {
        self.measured_bytes
            .fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
    }
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
    (
        tx,
        CancelToken {
            rx,
            limits: vec![],
            measured_bytes: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        },
    )
}

fn canceled() -> AppError {
    AppError::new(ErrorCode::InvalidTransition, "Download canceled")
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
    if resume.offset > 0
        && resume
            .effective_url
            .as_ref()
            .is_some_and(|u| u != response.url().as_str())
    {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "The download redirected to a different source. Keep the partial and add a fresh download.",
        ));
    }
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
        // Suffix resume must extend through EOF.
        let total = validate_content_range(range, resume.offset, None, response.content_length())?;
        Some(total)
    } else {
        response.content_length()
    };
    Ok(ResponseMetadata {
        effective_url: response.url().to_string(),
        total_bytes,
        etag,
        last_modified,
    })
}

/// Validate Content-Range. `expected_end = None` means the span must reach EOF.
fn validate_content_range(
    value: &str,
    expected_start: u64,
    expected_end: Option<u64>,
    length: Option<u64>,
) -> Result<u64, AppError> {
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
    if start != expected_start || end < start || end >= total {
        return Err(invalid());
    }
    match expected_end {
        None => {
            if end.checked_add(1) != Some(total) {
                return Err(invalid());
            }
        }
        Some(expected) => {
            if end != expected {
                return Err(invalid());
            }
        }
    }
    let expected_len = end
        .checked_sub(start)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(invalid)?;
    if length.is_some_and(|v| v != expected_len) {
        return Err(invalid());
    }
    Ok(total)
}

fn response_etag_last_modified(response: &reqwest::Response) -> (Option<String>, Option<String>) {
    let headers = response.headers();
    let etag = headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let last_modified = headers
        .get(header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    (etag, last_modified)
}

fn reject_encoded(response: &reqwest::Response) -> Result<(), AppError> {
    let encoding = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok());
    if encoding.is_some_and(|value| !value.eq_ignore_ascii_case("identity")) {
        return Err(AppError::new(
            ErrorCode::RangeUnsupported,
            "Encoded file responses cannot be safely verified",
        ));
    }
    Ok(())
}

/// Probe whether an origin supports integrity-safe multi-connection ranges.
pub async fn probe_segment_capability(
    client: &Client,
    url: &str,
    cancel: &CancelToken,
) -> Result<Option<SegmentCapability>, AppError> {
    crate::domain::url_policy::validate_download_url(url)?;
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    let timeouts = TransferTimeouts::default();
    let request = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .header(header::RANGE, "bytes=0-0");
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(canceled()),
        result = tokio::time::timeout(timeouts.headers, request.send()) => result.map_err(|_| network_deadline("The server did not send response headers in time. Check your connection and retry."))?.map_err(map_reqwest_err)?,
    };
    if response.status() != StatusCode::PARTIAL_CONTENT {
        // Drain/drop body; origin is not segment-compatible.
        return Ok(None);
    }
    if reject_encoded(&response).is_err()
        || !response
            .headers()
            .get(header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("bytes"))
    {
        return Ok(None);
    }
    let (etag, last_modified) = response_etag_last_modified(&response);
    let Some(etag) = etag.filter(|v| strong_etag(v)) else {
        return Ok(None);
    };
    let range = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok());
    let Some(range) = range else {
        return Ok(None);
    };
    let Ok(total) = validate_content_range(range, 0, Some(0), response.content_length()) else {
        return Ok(None);
    };
    if total < 2 {
        return Ok(None);
    }
    let effective_url = response.url().to_string();
    // Consume the one-byte body so the connection can close cleanly.
    if !consume_probe(response, cancel, timeouts.body).await? {
        return Ok(None);
    }
    // Mid-file probe confirms arbitrary ranges, not only prefix.
    let mid = total / 2;
    let mid_request = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .header(header::RANGE, format!("bytes={mid}-{mid}"))
        .header(header::IF_RANGE, &etag);
    let mid_response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(canceled()),
        result = tokio::time::timeout(timeouts.headers, mid_request.send()) => result.map_err(|_| network_deadline("The server did not send response headers in time. Check your connection and retry."))?.map_err(map_reqwest_err)?,
    };
    if mid_response.url().as_str() != effective_url
        || mid_response.status() != StatusCode::PARTIAL_CONTENT
    {
        return Ok(None);
    }
    if reject_encoded(&mid_response).is_err() {
        return Ok(None);
    }
    let (mid_etag, _) = response_etag_last_modified(&mid_response);
    if mid_etag.as_deref() != Some(etag.as_str()) {
        return Ok(None);
    }
    let mid_range = mid_response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok());
    let Some(mid_range) = mid_range else {
        return Ok(None);
    };
    if validate_content_range(mid_range, mid, Some(mid), mid_response.content_length()).ok()
        != Some(total)
    {
        return Ok(None);
    }
    if !consume_probe(mid_response, cancel, timeouts.body).await? {
        return Ok(None);
    }
    Ok(Some(SegmentCapability {
        effective_url,
        total_bytes: total,
        etag,
        last_modified,
    }))
}

async fn consume_probe(
    response: reqwest::Response,
    cancel: &CancelToken,
    timeout: Duration,
) -> Result<bool, AppError> {
    let mut stream = response.bytes_stream();
    let mut received = 0usize;
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = tokio::time::timeout(timeout, stream.next()) => result.map_err(|_| network_deadline("The server stopped sending probe data. Check your connection and retry."))?,
        };
        match next {
            Some(Ok(chunk)) => {
                received = received.saturating_add(chunk.len());
                if received > 1 {
                    return Ok(false);
                }
            }
            Some(Err(_)) => return Ok(false),
            None => return Ok(received == 1),
        }
    }
}

/// Download an exact inclusive byte range into a dedicated segment file.
pub struct RangeRequest<'a> {
    pub effective_url: Option<&'a str>,
    pub url: &'a str,
    pub dest: &'a Path,
    pub range: ByteRange,
    pub resume: u64,
    pub etag: &'a str,
    pub total: u64,
}

pub async fn download_range_managed(
    context: (&Client, &super::files::ManagedFiles),
    request: RangeRequest<'_>,
    cancel: &CancelToken,
    mut on_progress: impl FnMut(u64) -> Result<(), AppError>,
) -> Result<u64, AppError> {
    let RangeRequest {
        effective_url,
        url,
        dest,
        range,
        resume: resume_within_segment,
        etag,
        total: expected_total,
    } = request;
    let (client, files) = context;
    crate::domain::url_policy::validate_download_url(url)?;
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    if !strong_etag(etag) {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "This partial file cannot be resumed safely; add a fresh download",
        ));
    }
    if range.end < range.start {
        return Err(AppError::new(
            ErrorCode::RangeUnsupported,
            "The server returned an incompatible byte range",
        ));
    }
    let expected_len = range.len();
    if resume_within_segment > expected_len {
        return Err(AppError::new(
            ErrorCode::LengthMismatch,
            "Segment progress exceeds the planned range",
        ));
    }
    if resume_within_segment == expected_len {
        on_progress(resume_within_segment)?;
        return Ok(resume_within_segment);
    }
    let request_start = range
        .start
        .checked_add(resume_within_segment)
        .ok_or_else(|| AppError::new(ErrorCode::LengthMismatch, "File size overflow"))?;
    let timeouts = TransferTimeouts::default();
    let request = client
        .get(url)
        .header(header::ACCEPT_ENCODING, "identity")
        .header(
            header::RANGE,
            format!("bytes={request_start}-{}", range.end),
        )
        .header(header::IF_RANGE, etag);
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(canceled()),
        result = tokio::time::timeout(timeouts.headers, request.send()) => result.map_err(|_| network_deadline("The server did not send response headers in time. Check your connection and retry."))?.map_err(map_reqwest_err)?,
    };
    if effective_url.is_some_and(|u| u != response.url().as_str()) {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "The range response came from a different source. Keep the partial and add a fresh download.",
        ));
    }
    match response.status() {
        StatusCode::OK => {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The server ignored Range; add a fresh download",
            ));
        }
        StatusCode::PARTIAL_CONTENT => {}
        status => {
            return Err(http_status_error_with_retry_after(
                status,
                response.headers().get(header::RETRY_AFTER),
            ));
        }
    }
    reject_encoded(&response)?;
    let (response_etag, _) = response_etag_last_modified(&response);
    if response_etag.as_deref() != Some(etag) {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "The partial file has no matching strong validator; add a fresh download",
        ));
    }
    let content_range = response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::RangeUnsupported,
                "Missing Content-Range on resumed response",
            )
        })?;
    let total = validate_content_range(
        content_range,
        request_start,
        Some(range.end),
        response.content_length(),
    )?;
    if total != expected_total || total <= range.end {
        return Err(AppError::new(
            ErrorCode::ResourceChanged,
            "The server returned an incompatible byte range",
        ));
    }
    let name = files.name(dest)?;
    let mut file = File::from_std(files.partial(Path::new(name), resume_within_segment)?);
    let mut stream = response.bytes_stream();
    let mut written = resume_within_segment;
    let started = Instant::now();
    let mut last_tick = started;
    let transfer = async {
        loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(canceled()),
                result = tokio::time::timeout(timeouts.body, stream.next()) => result.map_err(|_| network_deadline("The server stopped sending file data. Check your connection and retry."))?,
            };
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(map_reqwest_err)?;
            cancel.record_bytes(chunk.len());
            cancel.throttle(chunk.len()).await?;
            let next_written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| AppError::new(ErrorCode::LengthMismatch, "File size overflow"))?;
            if next_written > expected_len {
                return Err(AppError::new(
                    ErrorCode::LengthMismatch,
                    "Response exceeded the declared file size",
                ));
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(canceled()),
                result = file.write_all(&chunk) => result.map_err(map_io_err)?,
            }
            written = next_written;
            if last_tick.elapsed().as_millis() >= 100 {
                file.flush().await.map_err(map_io_err)?;
                file.sync_all().await.map_err(map_io_err)?;
                on_progress(written)?;
                last_tick = Instant::now();
            }
        }
        if written != expected_len {
            return Err(AppError::new(
                ErrorCode::LengthMismatch,
                "The response ended before all expected bytes arrived",
            ));
        }
        Ok::<(), AppError>(())
    }
    .await;
    file.flush().await.map_err(map_io_err)?;
    file.sync_all().await.map_err(map_io_err)?;
    on_progress(written)?;
    transfer?;
    Ok(written)
}

#[cfg(test)]
pub async fn download_to_file(
    client: &Client,
    url: &str,
    dest: &Path,
    resume: ResumeMetadata,
    cancel: &CancelToken,
    on_metadata: impl FnMut(&ResponseMetadata) -> Result<(), AppError>,
    on_progress: impl FnMut(ProgressUpdate),
) -> Result<DownloadResult, AppError> {
    transfer(
        TransferRequest {
            client,
            url,
            dest,
            files: None,
            resume,
            cancel,
            timeouts: TransferTimeouts::default(),
        },
        on_metadata,
        on_progress,
    )
    .await
}

pub async fn download_managed(
    context: (&Client, &super::files::ManagedFiles),
    url: &str,
    dest: &Path,
    resume: ResumeMetadata,
    cancel: &CancelToken,
    on_metadata: impl FnMut(&ResponseMetadata) -> Result<(), AppError>,
    on_progress: impl FnMut(ProgressUpdate),
) -> Result<DownloadResult, AppError> {
    transfer(
        TransferRequest {
            client: context.0,
            files: Some(context.1),
            url,
            dest,
            resume,
            cancel,
            timeouts: TransferTimeouts::default(),
        },
        on_metadata,
        on_progress,
    )
    .await
}

#[derive(Clone, Copy)]
struct TransferTimeouts {
    headers: Duration,
    body: Duration,
}
impl Default for TransferTimeouts {
    fn default() -> Self {
        Self {
            headers: Duration::from_secs(30),
            body: Duration::from_secs(30),
        }
    }
}
pub(crate) struct TransferRequest<'a> {
    client: &'a Client,
    url: &'a str,
    dest: &'a Path,
    files: Option<&'a super::files::ManagedFiles>,
    resume: ResumeMetadata,
    cancel: &'a CancelToken,
    timeouts: TransferTimeouts,
}
async fn transfer(
    request: TransferRequest<'_>,
    mut on_metadata: impl FnMut(&ResponseMetadata) -> Result<(), AppError>,
    mut on_progress: impl FnMut(ProgressUpdate),
) -> Result<DownloadResult, AppError> {
    let TransferRequest {
        client,
        url,
        dest,
        files,
        resume,
        cancel,
        timeouts,
    } = request;
    crate::domain::url_policy::validate_download_url(url)?;
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
        result = tokio::time::timeout(timeouts.headers, request.send()) => result.map_err(|_| network_deadline("The server did not send response headers in time. Check your connection and retry."))?.map_err(map_reqwest_err)?,
    };
    match response.status() {
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
            return Err(http_status_error_with_retry_after(
                status,
                response.headers().get(header::RETRY_AFTER),
            ));
        }
    }

    let metadata = validate_metadata(&response, &resume)?;
    on_metadata(&metadata)?;
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    let owned_files;
    let files = match files {
        Some(files) => files,
        None => {
            owned_files = super::files::ManagedFiles::new(dest.parent().ok_or_else(|| {
                AppError::new(ErrorCode::PathNotAllowed, "Missing download directory")
            })?)?;
            &owned_files
        }
    };
    let mut file = File::from_std(files.partial(Path::new(files.name(dest)?), resume.offset)?);
    let mut stream = response.bytes_stream();
    let mut written = resume.offset;
    let started = Instant::now();
    let mut last_tick = started;
    let transfer = async {
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = tokio::time::timeout(timeouts.body, stream.next()) => result.map_err(|_| network_deadline("The server stopped sending file data. Check your connection and retry."))?,
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(map_reqwest_err)?;
            cancel.record_bytes(chunk.len());
            cancel.throttle(chunk.len()).await?;
        let next_written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| AppError::new(ErrorCode::LengthMismatch, "File size overflow"))?;
        if metadata
            .total_bytes
            .is_some_and(|total| next_written > total)
        {
            return Err(AppError::new(
                ErrorCode::LengthMismatch,
                "Response exceeded the declared file size",
            ));
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = file.write_all(&chunk) => result.map_err(map_io_err)?,
        }
        written = next_written;
        if last_tick.elapsed().as_millis() >= 100 {
            on_progress(ProgressUpdate {
                verifying: false,
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
            ErrorCode::LengthMismatch,
            "The response ended before all expected bytes arrived",
        ));
    }
    Ok::<(), AppError>(())
    }.await;
    // Drain pending Tokio filesystem work even on cancellation or network failure.
    file.flush().await.map_err(map_io_err)?;
    file.sync_all().await.map_err(map_io_err)?;
    transfer?;
    use tokio::io::AsyncSeekExt;
    file.seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(map_io_err)?;
    on_progress(ProgressUpdate {
        verifying: true,
        transferred_bytes: written,
        total_bytes: metadata.total_bytes,
        bytes_per_second: 0,
    });
    let metadata_before_hash = file.metadata().await.map_err(map_io_err)?;
    let sha256_hex = hash_reader(&mut file, cancel).await?;
    let verified_file = std::sync::Arc::new(super::files::VerifiedFile {
        file: file.into_std().await,
        metadata: metadata_before_hash,
    });
    verified_file.unchanged()?;
    on_progress(ProgressUpdate {
        verifying: true,
        transferred_bytes: written,
        total_bytes: metadata.total_bytes,
        bytes_per_second: 0,
    });
    Ok(DownloadResult {
        bytes_written: written,
        total_bytes: metadata.total_bytes,
        sha256_hex,
        verified_file,
    })
}

#[cfg(test)]
pub async fn hash_file(path: &Path, cancel: &CancelToken) -> Result<String, AppError> {
    let file = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(canceled()),
        result = File::open(path) => result.map_err(map_io_err)?,
    };
    hash_reader(file, cancel).await
}

pub(crate) async fn hash_reader(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    cancel: &CancelToken,
) -> Result<String, AppError> {
    let mut buffer = [0; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let size = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(canceled()),
            result = reader.read(&mut buffer) => result.map_err(map_io_err)?,
        };
        if size == 0 {
            break;
        }
        hasher.update(&buffer[..size]);
        // Always yield even if the reader is immediately ready, so cancellation cannot starve.
        tokio::task::yield_now().await;
    }
    Ok(hex::encode(hasher.finalize()))
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
            assert!(
                validate_content_range(range, 5, None, None).is_err(),
                "{range}"
            );
        }
        assert_eq!(
            validate_content_range("bytes 5-9/10", 5, None, Some(5)).unwrap(),
            10
        );
        assert!(validate_content_range("bytes 5-9/10", 5, None, Some(4)).is_err());
        assert_eq!(
            validate_content_range("bytes 2-4/10", 2, Some(4), Some(3)).unwrap(),
            10
        );
        assert!(validate_content_range("bytes 2-4/10", 2, Some(5), Some(3)).is_err());
    }

    #[tokio::test]
    async fn hashes_multiple_buffers_and_empty_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.bin");
        let (_tx, cancel) = cancel_pair();
        let body: Vec<u8> = (0..200_000).map(|index| (index % 251) as u8).collect();
        tokio::fs::write(&path, &body).await.unwrap();
        assert_eq!(
            hash_file(&path, &cancel).await.unwrap(),
            hex::encode(Sha256::digest(&body))
        );
        tokio::fs::write(&path, []).await.unwrap();
        assert_eq!(
            hash_file(&path, &cancel).await.unwrap(),
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
                effective_url: None,
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
    async fn changed_source_with_identical_etag_preserves_partial() {
        let (url, server) = serve(b"HTTP/1.1 206 Partial Content\r\nETag: \"same\"\r\nContent-Range: bytes 5-10/11\r\nContent-Length: 6\r\nConnection: close\r\n\r\n world".to_vec(), false).await;
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
                effective_url: Some("https://previous.example/file".into()),
            },
            &cancel,
            |_| panic!("changed source must not be accepted"),
            |_| {},
        )
        .await;
        assert_eq!(result.unwrap_err().code, ErrorCode::ResourceChanged);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"hello");
        server.await.unwrap();
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
                    effective_url: None,
                    offset: 5,
                    etag: Some("\"same\"".into()),
                },
                &cancel,
                |_| panic!("Invalid metadata must not be accepted"),
                |_| {},
            )
            .await;
            assert!(!result.unwrap_err().retryable, "{headers}");
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
                ResumeMetadata {
                    offset: 4,
                    etag,
                    effective_url: None,
                },
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
                let error = result.unwrap_err();
                assert_eq!(error.code, ErrorCode::NetworkInterrupted);
                assert!(error.retryable);
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
    #[tokio::test]
    async fn http_failures_have_stable_actionable_codes_and_retry_policy() {
        for (status, expected, retryable) in [
            (400, ErrorCode::HttpClientError, false),
            (401, ErrorCode::HttpUnauthorized, false),
            (403, ErrorCode::HttpForbidden, false),
            (404, ErrorCode::HttpNotFound, false),
            (408, ErrorCode::NetworkTimeout, true),
            (410, ErrorCode::HttpNotFound, false),
            (416, ErrorCode::RangeUnsupported, false),
            (422, ErrorCode::HttpClientError, false),
            (429, ErrorCode::HttpRateLimit, true),
            (500, ErrorCode::HttpServerError, true),
            (503, ErrorCode::HttpServerError, true),
            (302, ErrorCode::HttpRedirect, false),
        ] {
            let response = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let (url, server) = serve(response.into_bytes(), false).await;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            let (_tx, cancel) = cancel_pair();
            let error = download_to_file(
                &Client::new(),
                &format!("{url}?token=private"),
                &path,
                ResumeMetadata::default(),
                &cancel,
                |_| panic!("Failed response cannot be accepted"),
                |_| {},
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, expected, "HTTP {status}");
            assert_eq!(error.retryable, retryable, "HTTP {status}");
            assert!(error.message.contains(&format!("HTTP {status}")));
            assert!(!error.message.contains("private"));
            assert!(!path.exists());
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn refused_connections_are_retryable_without_exposing_the_url() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/private?signature=secret",
            listener.local_addr().unwrap()
        );
        drop(listener);
        let error = map_reqwest_err(Client::new().get(url).send().await.unwrap_err());
        assert_eq!(error.code, ErrorCode::NetworkConnection);
        assert!(error.retryable);
        assert!(!error.message.contains("secret"));
        assert!(!error.message.contains("private"));
    }

    #[tokio::test]
    async fn header_and_body_deadlines_use_the_real_transfer_path_and_keep_partials() {
        assert_eq!(TransferTimeouts::default().headers, Duration::from_secs(30));
        assert_eq!(TransferTimeouts::default().body, Duration::from_secs(30));
        // Small injected limits exercise the same path without spending 60 seconds per test.
        for body_stall in [false, true] {
            let response = if body_stall {
                b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nETag: \"fixture\"\r\n\r\nabc".to_vec()
            } else {
                Vec::new()
            };
            let (url, server) = serve(response, true).await;
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.part");
            let (_tx, cancel) = cancel_pair();
            let client = Client::new();
            let mut metadata_accepted = false;
            let error = tokio::time::timeout(
                Duration::from_secs(2),
                transfer(
                    TransferRequest {
                        client: &client,
                        url: &url,
                        dest: &path,
                        files: None,
                        resume: ResumeMetadata::default(),
                        cancel: &cancel,
                        timeouts: TransferTimeouts {
                            headers: Duration::from_millis(200),
                            body: Duration::from_millis(200),
                        },
                    },
                    |_| {
                        metadata_accepted = true;
                        Ok(())
                    },
                    |_| {},
                ),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert_eq!(error.code, ErrorCode::NetworkTimeout);
            assert!(error.retryable);
            assert_eq!(metadata_accepted, body_stall);
            if body_stall {
                assert!(error.message.contains("file data"));
                assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
            } else {
                assert!(error.message.contains("response headers"));
                assert!(!path.exists());
            }
            server.abort();
        }
    }

    #[tokio::test]
    async fn verification_cancels_after_reading_data_while_waiting_for_more() {
        let (reader, mut writer) = tokio::io::duplex(1024);
        let (tx, cancel) = cancel_pair();
        let hashing = tokio::spawn(async move { hash_reader(reader, &cancel).await });
        // More than the duplex capacity proves the verifier has started consuming data.
        writer.write_all(&vec![3; 64 * 1024]).await.unwrap();
        tx.send(true).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(1), hashing)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidTransition);
        // Writer is still alive: EOF did not terminate the hash.
        drop(writer);
    }

    #[tokio::test]
    async fn verification_cancellation_is_not_starved_by_an_always_ready_reader() {
        let (tx, cancel) = cancel_pair();
        let hashing = tokio::spawn(async move { hash_reader(tokio::io::repeat(1), &cancel).await });
        tokio::task::yield_now().await;
        tx.send(true).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), hashing)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .code,
            ErrorCode::InvalidTransition
        );
    }

    #[tokio::test]
    async fn verification_of_an_actual_file_respects_preexisting_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.bin");
        tokio::fs::write(&path, b"fixture").await.unwrap();
        let (tx, cancel) = cancel_pair();
        tx.send(true).unwrap();
        assert_eq!(
            hash_file(&path, &cancel).await.unwrap_err().code,
            ErrorCode::InvalidTransition
        );
        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"fixture");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn untrusted_tls_certificate_is_classified_and_never_bypassed() {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        use std::process::Command;
        use std::sync::Arc;
        // Generate an ephemeral fixture key at runtime; no private key is committed.
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec![
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=localhost",
                "-keyout",
                "key.pem",
                "-out",
                "cert.pem",
            ],
            vec![
                "x509", "-in", "cert.pem", "-outform", "DER", "-out", "cert.der",
            ],
            vec![
                "pkcs8", "-topk8", "-nocrypt", "-in", "key.pem", "-outform", "DER", "-out",
                "key.der",
            ],
        ] {
            assert!(
                Command::new("openssl")
                    .args(args)
                    .current_dir(dir.path())
                    .output()
                    .expect("TLS integration fixture requires openssl on Unix")
                    .status
                    .success()
            );
        }
        let certificate = CertificateDer::from(std::fs::read(dir.path().join("cert.der")).unwrap());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            std::fs::read(dir.path().join("key.der")).unwrap(),
        ));
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let _ = tokio_rustls::TlsAcceptor::from(Arc::new(config))
                .accept(socket)
                .await;
        });
        let error = crate::infrastructure::network::configured_client(
            &crate::domain::advanced::NetworkSettings::default(),
            None,
            None,
            None,
            true,
        )
        .unwrap()
        .get(format!("https://{address}/file?signature=private"))
        .send()
        .await
        .unwrap_err();
        let classified = map_reqwest_err(error);
        assert_eq!(classified.code, ErrorCode::TlsError);
        assert!(!classified.retryable);
        assert!(!classified.message.contains("private"));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn redirect_loops_have_a_permanent_redacted_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/loop?token=private",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let _ = socket.read(&mut buffer).await;
                socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: /loop?token=private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            }
        });
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::limited(1))
            .build()
            .unwrap();
        let error = map_reqwest_err(client.get(url).send().await.unwrap_err());
        assert_eq!(error.code, ErrorCode::HttpRedirect);
        assert!(!error.retryable);
        assert!(!error.message.contains("private"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn body_deadline_measures_inactivity_rather_than_total_download_time() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/fixture", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 4096];
            let _ = socket.read(&mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            for _ in 0..6 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                socket.write_all(b"x").await.unwrap();
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.part");
        let client = Client::new();
        let (_tx, cancel) = cancel_pair();
        let result = transfer(
            TransferRequest {
                client: &client,
                url: &url,
                dest: &path,
                files: None,
                resume: ResumeMetadata::default(),
                cancel: &cancel,
                timeouts: TransferTimeouts {
                    headers: Duration::from_secs(2),
                    body: Duration::from_millis(200),
                },
            },
            |_| Ok(()),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.bytes_written, 6);
        assert_eq!(result.sha256_hex, hex::encode(Sha256::digest(b"xxxxxx")));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn probe_body_has_a_deadline_and_rejects_more_than_one_byte() {
        let (url, server) = serve(
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 1\r\n\r\n".to_vec(),
            true,
        )
        .await;
        let response = Client::new().get(&url).send().await.unwrap();
        let (_tx, cancel) = cancel_pair();
        let error = consume_probe(response, &cancel, Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::NetworkTimeout);
        server.abort();
        let (url, server) = serve(
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\nConnection: close\r\n\r\nab"
                .to_vec(),
            false,
        )
        .await;
        let response = Client::new().get(&url).send().await.unwrap();
        assert!(
            !consume_probe(response, &cancel, Duration::from_secs(1))
                .await
                .unwrap()
        );
        server.await.unwrap();
    }
}
