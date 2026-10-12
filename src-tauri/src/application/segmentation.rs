use crate::domain::{
    AppError, DownloadSegment, DownloadTask, ErrorCode, SegmentStatus, TransferMode,
};
use crate::infrastructure::db::Database;
use crate::infrastructure::errors::map_io_err;
use crate::infrastructure::files::{ManagedFiles, VerifiedFile};
use crate::infrastructure::http::{
    ByteRange, CancelToken, DownloadResult, RangeRequest, ResponseMetadata, ResumeMetadata,
    cancel_pair, download_managed, download_range_managed, hash_reader, probe_segment_capability,
};
use chrono::Utc;
use futures_util::{StreamExt, stream::FuturesUnordered};
use parking_lot::Mutex;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;

pub const SEGMENT_SIZE_THRESHOLD: u64 = 8 * 1024 * 1024;

pub fn plan_segments(task_id: &str, total: u64, connections: u32) -> Vec<DownloadSegment> {
    if total == 0 || total > i64::MAX as u64 {
        return Vec::new();
    }
    let count = u64::from(connections.clamp(1, 4)).min(total);
    let chunk = total / count;
    (0..count)
        .map(|index| DownloadSegment {
            task_id: task_id.into(),
            segment_index: index as u32,
            start_byte: index * chunk,
            end_byte: if index + 1 == count {
                total - 1
            } else {
                (index + 1) * chunk - 1
            },
            completed_bytes: 0,
            status: SegmentStatus::Pending,
            updated_at: Utc::now(),
        })
        .collect()
}

pub fn segment_file_path(temp_path: &str, index: u32) -> PathBuf {
    PathBuf::from(format!("{temp_path}.s{index}"))
}

pub fn should_attempt_segmentation(connections: u32, total: u64) -> bool {
    connections > 1 && total >= SEGMENT_SIZE_THRESHOLD && total <= i64::MAX as u64
}

pub struct SegmentProgress {
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
    pub verifying: bool,
}

pub struct SegmentationRequest<'a> {
    pub client: &'a Client,
    pub files: &'a ManagedFiles,
    pub db: &'a Database,
    pub task: &'a mut DownloadTask,
    pub temp_path: &'a Path,
    pub max_connections: u32,
    pub cancel: &'a CancelToken,
}

pub async fn download_with_optional_segmentation(
    request: SegmentationRequest<'_>,
    mut on_metadata: impl FnMut(&ResponseMetadata) -> Result<(), AppError>,
    mut on_progress: impl FnMut(SegmentProgress),
) -> Result<DownloadResult, AppError> {
    let SegmentationRequest {
        client,
        files,
        db,
        task,
        temp_path,
        max_connections,
        cancel,
    } = request;
    let connections = max_connections.clamp(1, 4);
    // A saved plan must be resumed even if the preference has since become one.
    if task.transfer_mode == TransferMode::Segmented {
        let segments = db.list_segments(&task.id)?;
        let etag = task.etag.as_deref().ok_or_else(changed)?;
        let total = task.total_bytes.ok_or_else(changed)?;
        if !coverage_contiguous(&segments, total) {
            return Err(changed());
        }
        on_metadata(&ResponseMetadata {
            total_bytes: Some(total),
            etag: Some(etag.into()),
            last_modified: task.last_modified.clone(),
            effective_url: db
                .download_source(&task.id)?
                .unwrap_or_else(|| task.url.clone()),
        })?;
        return run_segments(
            SegmentRun {
                client,
                files,
                db,
                task,
                temp_path,
                segments: &segments,
                etag,
                total,
                connections,
                cancel,
            },
            &mut on_progress,
        )
        .await;
    }
    let offset = files
        .length(Path::new(files.name(temp_path)?))?
        .unwrap_or(0);
    // Never turn a pre-existing single partial into a new segmented representation.
    if connections > 1
        && offset == 0
        && let Some(capability) = probe_segment_capability(client, &task.url, cancel).await?
        && should_attempt_segmentation(connections, capability.total_bytes)
    {
        let segments = plan_segments(&task.id, capability.total_bytes, connections);
        on_metadata(&ResponseMetadata {
            total_bytes: Some(capability.total_bytes),
            etag: Some(capability.etag.clone()),
            last_modified: capability.last_modified.clone(),
            effective_url: capability.effective_url.clone(),
        })?;
        db.replace_segments(&segments)?; // Plan and mode commit atomically before workers.
        task.transfer_mode = TransferMode::Segmented;
        task.etag = Some(capability.etag.clone());
        task.total_bytes = Some(capability.total_bytes);
        task.last_modified = capability.last_modified;
        return run_segments(
            SegmentRun {
                client,
                files,
                db,
                task,
                temp_path,
                segments: &segments,
                etag: &capability.etag,
                total: capability.total_bytes,
                connections,
                cancel,
            },
            &mut on_progress,
        )
        .await;
    }
    download_managed(
        (client, files),
        &task.url,
        temp_path,
        ResumeMetadata {
            effective_url: db
                .download_source(&task.id)?
                .or_else(|| (offset > 0).then(|| task.url.clone())),
            offset,
            etag: task.etag.clone(),
            last_modified: task.last_modified.clone(),
        },
        cancel,
        on_metadata,
        |update| {
            on_progress(SegmentProgress {
                transferred_bytes: update.transferred_bytes,
                total_bytes: update.total_bytes,
                bytes_per_second: update.bytes_per_second,
                verifying: update.verifying,
            })
        },
    )
    .await
}

fn changed() -> AppError {
    AppError::new(
        ErrorCode::ResourceChanged,
        "The segment checkpoint is inconsistent. Keep the partials and add a fresh download.",
    )
}
fn canceled() -> AppError {
    AppError::new(ErrorCode::InvalidTransition, "Download canceled")
}

struct SegmentRun<'a> {
    client: &'a Client,
    files: &'a ManagedFiles,
    db: &'a Database,
    task: &'a DownloadTask,
    temp_path: &'a Path,
    segments: &'a [DownloadSegment],
    etag: &'a str,
    total: u64,
    connections: u32,
    cancel: &'a CancelToken,
}

async fn run_segments(
    run: SegmentRun<'_>,
    on_progress: &mut impl FnMut(SegmentProgress),
) -> Result<DownloadResult, AppError> {
    if !coverage_contiguous(run.segments, run.total) {
        return Err(changed());
    }
    let progress = Mutex::new(
        run.segments
            .iter()
            .map(|s| s.completed_bytes)
            .collect::<Vec<_>>(),
    );
    let started = Instant::now();
    let initial: u64 = progress.lock().iter().sum();
    let slots = Semaphore::new(run.connections.clamp(1, 4) as usize);
    let (stop, mut child_cancel) = cancel_pair();
    child_cancel.inherit_limits(run.cancel);
    let mut jobs = FuturesUnordered::new();
    for segment in run.segments {
        let run = &run;
        let progress = &progress;
        let slots = &slots;
        let cancel = &child_cancel;
        jobs.push(async move {
            let _permit = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(canceled()),
                result = slots.acquire() => result.map_err(|_| canceled())?,
            };
            run_one_segment(run, segment.clone(), cancel, progress).await
        });
    }
    let mut first_error = None;
    let mut stopping = false;
    let mut ticks = tokio::time::interval(Duration::from_millis(100));
    while !jobs.is_empty() {
        tokio::select! {
            biased;
            _ = run.cancel.cancelled(), if !stopping => {
                stopping = true; let _ = stop.send(true); first_error = Some(canceled());
            }
            result = jobs.next() => {
                if let Some(Err(error)) = result && first_error.is_none() {
                    first_error = Some(error); stopping = true; let _ = stop.send(true);
                }
            }
            _ = ticks.tick() => {
                let transferred: u64 = progress.lock().iter().sum();
                on_progress(SegmentProgress { transferred_bytes: transferred, total_bytes: Some(run.total), bytes_per_second: ((transferred.saturating_sub(initial)) as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64, verifying: false });
            }
        }
    }
    // All child futures have flushed and checkpointed before the coordinator returns.
    if let Some(error) = first_error {
        return Err(error);
    }
    if run.cancel.is_cancelled() {
        return Err(canceled());
    }
    let latest = run.db.list_segments(&run.task.id)?;
    if !segments_complete(&latest, run.total) {
        return Err(changed());
    }
    on_progress(SegmentProgress {
        transferred_bytes: run.total,
        total_bytes: Some(run.total),
        bytes_per_second: 0,
        verifying: true,
    });
    assemble_verified(run.files, run.temp_path, &latest, run.total, run.cancel).await
}

async fn run_one_segment(
    run: &SegmentRun<'_>,
    mut segment: DownloadSegment,
    cancel: &CancelToken,
    progress: &Mutex<Vec<u64>>,
) -> Result<(), AppError> {
    let path = segment_file_path(&run.temp_path.to_string_lossy(), segment.segment_index);
    let name = PathBuf::from(run.files.name(&path)?);
    let actual = reconcile_segment_file(run.files, &path, &segment)?;
    segment.completed_bytes = actual;
    run.db.update_segment_checkpoint(
        &segment.task_id,
        segment.segment_index,
        actual,
        SegmentStatus::Active,
    )?;
    progress.lock()[segment.segment_index as usize] = actual;
    let effective_url = run.db.download_source(&run.task.id)?;
    let result = download_range_managed(
        (run.client, run.files),
        RangeRequest {
            effective_url: effective_url.as_deref(),
            url: &run.task.url,
            dest: &path,
            range: ByteRange {
                start: segment.start_byte,
                end: segment.end_byte,
            },
            resume: actual,
            etag: run.etag,
            total: run.total,
        },
        cancel,
        |completed| {
            run.db.update_segment_checkpoint(
                &segment.task_id,
                segment.segment_index,
                completed,
                SegmentStatus::Active,
            )?;
            progress.lock()[segment.segment_index as usize] = completed;
            Ok(())
        },
    )
    .await;
    match result {
        Ok(written) => run.db.update_segment_checkpoint(
            &segment.task_id,
            segment.segment_index,
            written,
            SegmentStatus::Complete,
        ),
        Err(error) => {
            let completed = run
                .files
                .length(&name)?
                .unwrap_or(0)
                .min(segment.expected_len());
            run.db.update_segment_checkpoint(
                &segment.task_id,
                segment.segment_index,
                completed,
                SegmentStatus::Failed,
            )?;
            progress.lock()[segment.segment_index as usize] = completed;
            Err(error)
        }
    }
}

async fn assemble_verified(
    files: &ManagedFiles,
    temp: &Path,
    segments: &[DownloadSegment],
    total: u64,
    cancel: &CancelToken,
) -> Result<DownloadResult, AppError> {
    if !segments_complete(segments, total) {
        return Err(changed());
    }
    // Check every extent and source handle before changing the canonical partial.
    let mut inputs = Vec::new();
    for segment in segments {
        let path = segment_file_path(&temp.to_string_lossy(), segment.segment_index);
        let file = files.open(Path::new(files.name(&path)?), false, false)?;
        let metadata = file.metadata().map_err(map_io_err)?;
        if metadata.len() != segment.expected_len() {
            return Err(changed());
        }
        inputs.push((tokio::fs::File::from_std(file), metadata));
    }
    if cancel.is_cancelled() {
        return Err(canceled());
    }
    let name = Path::new(files.name(temp)?);
    let output = match files.open(name, true, true) {
        Err(error) if error.code == ErrorCode::AlreadyExists => files.open(name, true, false)?,
        result => result?,
    };
    let mut output = tokio::fs::File::from_std(output);
    output.set_len(0).await.map_err(map_io_err)?;
    let mut buffer = [0; 64 * 1024];
    let copy = async {
        for (input, before) in &mut inputs {
            loop {
                let n = tokio::select! { biased; _ = cancel.cancelled() => return Err(canceled()), n = input.read(&mut buffer) => n.map_err(map_io_err)? };
                if n == 0 { break; }
                tokio::select! { biased; _ = cancel.cancelled() => return Err(canceled()), result = output.write_all(&buffer[..n]) => result.map_err(map_io_err)? };
                tokio::task::yield_now().await;
            }
            let after = input.metadata().await.map_err(map_io_err)?;
            if after.len() != before.len() || after.modified().map_err(map_io_err)? != before.modified().map_err(map_io_err)? { return Err(changed()); }
        }
        Ok(())
    }.await;
    output.flush().await.map_err(map_io_err)?;
    output.sync_all().await.map_err(map_io_err)?;
    files.sync()?;
    copy?;
    let metadata = output.metadata().await.map_err(map_io_err)?;
    if metadata.len() != total {
        return Err(changed());
    }
    output
        .seek(std::io::SeekFrom::Start(0))
        .await
        .map_err(map_io_err)?;
    let sha256_hex = hash_reader(&mut output, cancel).await?;
    let verified_file = Arc::new(VerifiedFile {
        file: output.into_std().await,
        metadata,
    });
    verified_file.unchanged()?;
    Ok(DownloadResult {
        bytes_written: total,
        total_bytes: Some(total),
        sha256_hex,
        verified_file,
    })
}

/// File length may be ahead of a committed checkpoint after a crash. Discard
/// that unconfirmed tail instead of treating length alone as verified progress.
pub(crate) fn reconcile_segment_file(
    files: &ManagedFiles,
    path: &Path,
    segment: &DownloadSegment,
) -> Result<u64, AppError> {
    let name = Path::new(files.name(path)?);
    let length = files.length(name)?.unwrap_or(0);
    if length > segment.expected_len() {
        return Err(changed());
    }
    let confirmed = length.min(segment.completed_bytes);
    if length > confirmed {
        let file = files.open(name, true, false)?;
        file.set_len(confirmed).map_err(map_io_err)?;
        file.sync_all().map_err(map_io_err)?;
    }
    Ok(confirmed)
}

pub(crate) fn coverage_contiguous(segments: &[DownloadSegment], total: u64) -> bool {
    if segments.is_empty() || segments.len() > 4 || total == 0 || total > i64::MAX as u64 {
        return false;
    }
    let mut next = 0;
    for (index, segment) in segments.iter().enumerate() {
        if segment.segment_index as usize != index
            || segment.start_byte != next
            || segment.end_byte < segment.start_byte
            || segment.completed_bytes > segment.expected_len()
            || segment.task_id != segments[0].task_id
        {
            return false;
        }
        let Some(end) = segment.end_byte.checked_add(1) else {
            return false;
        };
        next = end;
    }
    next == total
}

fn segments_complete(segments: &[DownloadSegment], total: u64) -> bool {
    coverage_contiguous(segments, total)
        && segments.iter().all(|segment| {
            segment.status == SegmentStatus::Complete
                && segment.completed_bytes == segment.expected_len()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_even_splits_with_remainder_on_last() {
        let segments = plan_segments("t", 10, 3);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].start_byte, 0);
        assert_eq!(segments[0].end_byte, 2);
        assert_eq!(segments[1].start_byte, 3);
        assert_eq!(segments[1].end_byte, 5);
        assert_eq!(segments[2].start_byte, 6);
        assert_eq!(segments[2].end_byte, 9);
        assert!(coverage_contiguous(&segments, 10));
    }

    #[test]
    fn rejects_holes_in_coverage() {
        let mut segments = plan_segments("t", 10, 2);
        segments[1].start_byte = 8;
        assert!(!coverage_contiguous(&segments, 10));
    }
    #[test]
    fn empty_and_invalid_plans_are_rejected() {
        assert!(plan_segments("t", 0, 4).is_empty());
        assert!(!should_attempt_segmentation(1, SEGMENT_SIZE_THRESHOLD));
        assert!(!should_attempt_segmentation(4, SEGMENT_SIZE_THRESHOLD - 1));
        let mut plan = plan_segments("t", 10, 2);
        plan[1].segment_index = 0;
        assert!(!coverage_contiguous(&plan, 10));
        assert!(!coverage_contiguous(&plan_segments("t", 10, 2), 11));
    }

    #[tokio::test]
    async fn assembly_refuses_holes_lengths_and_cancellation_before_publication() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let files = ManagedFiles::new(&dir.path().join("downloads")).unwrap();
        let temp = dir.path().join("downloads/test.part");
        let mut segments = plan_segments("t", 10, 2);
        for (segment, bytes) in segments.iter_mut().zip([b"abcde", b"fghij"]) {
            let path = segment_file_path(&temp.to_string_lossy(), segment.segment_index);
            files
                .partial(Path::new(files.name(&path).unwrap()), 0)
                .unwrap()
                .write_all(bytes)
                .unwrap();
            segment.completed_bytes = 5;
            segment.status = SegmentStatus::Complete;
        }
        let (_tx, cancel) = cancel_pair();
        let mut invalid = segments.clone();
        invalid[1].start_byte = 8;
        assert!(
            assemble_verified(&files, &temp, &invalid, 10, &cancel)
                .await
                .is_err()
        );
        assert!(!temp.exists());
        let second = segment_file_path(&temp.to_string_lossy(), 1);
        let handle = files
            .open(Path::new(files.name(&second).unwrap()), true, false)
            .unwrap();
        handle.set_len(4).unwrap();
        assert!(
            assemble_verified(&files, &temp, &segments, 10, &cancel)
                .await
                .is_err()
        );
        assert!(!temp.exists());
        handle.set_len(5).unwrap();
        drop(handle);
        let (tx, canceled_token) = cancel_pair();
        tx.send(true).unwrap();
        assert_eq!(
            assemble_verified(&files, &temp, &segments, 10, &canceled_token)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InvalidTransition
        );
        assert!(!temp.exists());
    }

    #[tokio::test]
    async fn interrupted_assembly_is_rebuilt_and_hash_matches_ordered_bytes() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("downloads");
        let files = ManagedFiles::new(&root).unwrap();
        let temp = root.join("test.part");
        let mut segments = plan_segments("t", 10, 2);
        for (segment, bytes) in segments.iter_mut().rev().zip([b"fghij", b"abcde"]) {
            let path = segment_file_path(&temp.to_string_lossy(), segment.segment_index);
            files
                .partial(Path::new(files.name(&path).unwrap()), 0)
                .unwrap()
                .write_all(bytes)
                .unwrap();
            segment.completed_bytes = 5;
            segment.status = SegmentStatus::Complete;
        }
        files
            .partial(Path::new("test.part"), 0)
            .unwrap()
            .write_all(b"interrupted assembly")
            .unwrap();
        let (_tx, cancel) = cancel_pair();
        let result = assemble_verified(&files, &temp, &segments, 10, &cancel)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&temp).unwrap(), b"abcdefghij");
        assert_eq!(
            result.sha256_hex,
            "72399361da6a7754fec986dca5b7cbaf1c810a28ded4abaf56b2106d06cb78b0"
        );
        assert_eq!(result.bytes_written, 10);
    }

    #[test]
    fn uncommitted_segment_tail_is_discarded_but_oversized_files_are_preserved() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("downloads");
        let files = ManagedFiles::new(&root).unwrap();
        let path = root.join("test.part.s0");
        let mut segment = plan_segments("t", 10, 1).remove(0);
        segment.completed_bytes = 5;
        files
            .partial(Path::new("test.part.s0"), 0)
            .unwrap()
            .write_all(b"abcdefghij")
            .unwrap();
        assert_eq!(reconcile_segment_file(&files, &path, &segment).unwrap(), 5);
        assert_eq!(std::fs::read(&path).unwrap(), b"abcde");
        files
            .open(Path::new("test.part.s0"), true, false)
            .unwrap()
            .set_len(11)
            .unwrap();
        assert!(reconcile_segment_file(&files, &path, &segment).is_err());
        assert_eq!(std::fs::metadata(path).unwrap().len(), 11);
    }
}
