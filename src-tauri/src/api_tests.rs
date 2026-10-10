use crate::application::DownloadManager;
use crate::domain::{
    CreateDownloadRequest, DownloadStatus, ErrorCode, RemoveDownloadRequest, UpdateSettingsRequest,
};
use crate::infrastructure::db::Database;
use crate::infrastructure::ownership::DataDirectoryLease;
use crate::ipc::progress::ProgressHub;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn manager() -> (tempfile::TempDir, DownloadManager) {
    let dir = tempdir().unwrap();
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let downloads = dir.path().join("downloads");
    let mgr = DownloadManager::new(
        db,
        downloads,
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    (dir, mgr)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

async fn wait_for_status(
    mgr: &DownloadManager,
    id: &str,
    wanted: &[DownloadStatus],
    timeout: Duration,
) -> DownloadStatus {
    let start = std::time::Instant::now();
    loop {
        let task = mgr.get(id).unwrap();
        if wanted.contains(&task.status) {
            return task.status;
        }
        if start.elapsed() > timeout {
            panic!(
                "timeout waiting for {:?}, last status={} error={:?}",
                wanted,
                task.status.as_str(),
                task.error
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Minimal HTTP/1.1 fixture server for integration tests.
async fn spawn_fixture_server(
    handler: impl Fn(String) -> (u16, Vec<u8>) + Send + Sync + 'static,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handler = std::sync::Arc::new(handler);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let handler = handler.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let (status, body) = handler(req);
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(&body).await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn create_rejects_remote_http() {
    let (_dir, mgr) = manager();
    let err = mgr
        .create(CreateDownloadRequest {
            url: "http://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedScheme);
}

#[tokio::test]
async fn create_rejects_invalid_url() {
    let (_dir, mgr) = manager();
    let err = mgr
        .create(CreateDownloadRequest {
            url: "not-a-url".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidUrl);
}

#[tokio::test]
async fn get_missing_task_not_found() {
    let (_dir, mgr) = manager();
    let err = mgr.get("missing").unwrap_err();
    assert_eq!(err.code, ErrorCode::TaskNotFound);
}

#[tokio::test]
async fn list_reflects_created_tasks() {
    let (_dir, mgr) = manager();
    mgr.create(CreateDownloadRequest {
        url: "https://example.com/a.bin".into(),
        destination_id: None,
        suggested_name: Some("a.bin".into()),
        start_now: false,
        request_id: None,
        expected_sha256: None,
    })
    .unwrap();
    assert_eq!(mgr.list().unwrap().len(), 1);
}

#[tokio::test]
async fn create_idempotent_with_request_id() {
    let (_dir, mgr) = manager();
    let a = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: Some("idem-1".into()),
            expected_sha256: None,
        })
        .unwrap();
    let b = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: Some("idem-1".into()),
            expected_sha256: None,
        })
        .unwrap();
    assert_eq!(a.id, b.id);
    assert_eq!(mgr.list().unwrap().len(), 1);
}

#[tokio::test]
async fn downloads_fixture_byte_identical() {
    let body = b"hello-circle-downloader-fixture-bytes".to_vec();
    let expected = sha256_hex(&body);
    let base = spawn_fixture_server(move |_req| (200, body.clone())).await;

    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/file.bin"),
            destination_id: None,
            suggested_name: Some("file.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();

    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(10),
    )
    .await;

    let done = mgr.get(&task.id).unwrap();
    assert_eq!(done.status, DownloadStatus::Completed);
    let path = mgr.download_dir().join("file.bin");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(sha256_hex(&bytes), expected);
}

#[tokio::test]
async fn cancel_mid_download_is_not_completed() {
    let base = spawn_fixture_server(|_req| (200, vec![7u8; 2_000_000])).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/big.bin"),
            destination_id: None,
            suggested_name: Some("big.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;
    let canceled = mgr.cancel(&task.id).await.unwrap();
    assert_eq!(canceled.status, DownloadStatus::Canceled);
}

#[tokio::test]
async fn cancel_completed_is_invalid_transition() {
    let base = spawn_fixture_server(|_req| (200, b"x".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/x.bin"),
            destination_id: None,
            suggested_name: Some("x.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(10),
    )
    .await;
    let err = mgr.cancel(&task.id).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidTransition);
}

#[tokio::test]
async fn pause_resume_retry_remove() {
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();

    assert_eq!(task.status, DownloadStatus::Paused);

    let _ = mgr
        .remove(RemoveDownloadRequest {
            id: task.id,
            delete_partial: true,
        })
        .await;

    let bad = mgr
        .create(CreateDownloadRequest {
            url: "http://127.0.0.1:1/nope.bin".into(),
            destination_id: None,
            suggested_name: Some("nope.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &bad.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(10),
    )
    .await;

    let illegal = mgr.pause(&bad.id).await.unwrap_err();
    assert_eq!(illegal.code, ErrorCode::InvalidTransition);

    let retried = mgr.retry(&bad.id).await.unwrap();
    assert!(matches!(
        retried.status,
        DownloadStatus::Queued
            | DownloadStatus::Connecting
            | DownloadStatus::Downloading
            | DownloadStatus::Failed
    ));

    wait_for_status(
        &mgr,
        &bad.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(10),
    )
    .await;

    let removed = mgr
        .remove(RemoveDownloadRequest {
            id: bad.id.clone(),
            delete_partial: true,
        })
        .await
        .unwrap();
    assert!(removed.removed);
    assert_eq!(mgr.get(&bad.id).unwrap_err().code, ErrorCode::TaskNotFound);
}

#[tokio::test]
async fn range_200_does_not_corrupt_partial() {
    let base = spawn_fixture_server(|req| {
        // Always ignore Range and return 200 full body.
        let _ = req;
        (200, b"FULL-FILE-CONTENT".to_vec())
    })
    .await;

    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/file.bin"),
            destination_id: None,
            suggested_name: Some("file.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();

    let temp = task.temp_path.clone().unwrap();
    std::fs::write(&temp, b"PART").unwrap();
    assert_eq!(task.status, DownloadStatus::Paused);
    let _ = mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed, DownloadStatus::Completed],
        Duration::from_secs(10),
    )
    .await;

    let done = mgr.get(&task.id).unwrap();
    if done.status == DownloadStatus::Completed {
        let bytes = std::fs::read(mgr.download_dir().join("file.bin")).unwrap();
        assert_eq!(bytes, b"FULL-FILE-CONTENT");
        assert!(!String::from_utf8_lossy(&bytes).contains("PARTFULL"));
    } else {
        assert_eq!(done.status, DownloadStatus::Failed);
        assert!(matches!(
            done.error.as_ref().map(|e| e.code),
            Some(ErrorCode::ResourceChanged | ErrorCode::RangeUnsupported)
        ));
    }
}

#[tokio::test]
async fn reconcile_marks_active_as_failed() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = Database::open(&db_path).unwrap();
    let downloads = dir.path().join("downloads");
    let mgr = DownloadManager::new(
        db,
        downloads.clone(),
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let mut active = mgr.get(&task.id).unwrap();
    active.status = DownloadStatus::Downloading;
    Database::open(&db_path)
        .unwrap()
        .upsert_download(&active)
        .unwrap();

    drop(mgr);
    let mgr2 = DownloadManager::new(
        Database::open(&db_path).unwrap(),
        downloads,
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    mgr2.reconcile_on_startup().unwrap();
    let reconciled = mgr2.get(&task.id).unwrap();
    assert_eq!(reconciled.status, DownloadStatus::Failed);
    assert!(reconciled.error.as_ref().unwrap().retryable);
}

#[tokio::test]
async fn settings_and_queues_apis() {
    let (_dir, mgr) = manager();
    let settings = mgr.get_settings().unwrap();
    assert_eq!(settings.max_concurrent_downloads, 3);

    let updated = mgr
        .update_settings(UpdateSettingsRequest {
            max_concurrent_downloads: Some(4),
            default_destination_id: Some("default".into()),
            auto_start: Some(false),
        })
        .unwrap();
    assert_eq!(updated.max_concurrent_downloads, 4);
    assert!(!updated.auto_start);

    let queues = mgr.list_queues().unwrap();
    assert_eq!(queues[0].id, "default");
}

#[test]
fn health_check_unit() {
    assert_eq!(crate::commands::system::health_check(), "ok");
}

#[tokio::test]
async fn deferred_download_is_paused_and_resumable() {
    let base = spawn_fixture_server(|_| (200, b"RESUMED".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/later.bin"),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    assert_eq!(task.status, DownloadStatus::Paused);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(mgr.get(&task.id).unwrap().downloaded_bytes, 0);
    mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(5),
    )
    .await;
}

#[tokio::test]
async fn auto_start_off_saves_paused_even_when_requested() {
    let (_dir, mgr) = manager();
    mgr.update_settings(UpdateSettingsRequest {
        auto_start: Some(false),
        ..Default::default()
    })
    .unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/later.bin".into(),
            destination_id: None,
            suggested_name: None,
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    assert_eq!(task.status, DownloadStatus::Paused);
}

#[test]
fn native_sync_command_can_start_worker_without_tokio_context() {
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "http://127.0.0.1:1/native.bin".into(),
            destination_id: None,
            suggested_name: None,
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if mgr.get(&task.id).unwrap().status == DownloadStatus::Failed {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "worker never ran");
        std::thread::sleep(Duration::from_millis(10));
    }
}

async fn spawn_slow_range_server(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = std::sync::Arc::new(body);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.ends_with(b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..n]);
                }
                let request = String::from_utf8_lossy(&request).to_lowercase();
                let offset = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .and_then(|value| value.trim_end_matches('-').parse::<usize>().ok())
                    .unwrap_or(0);
                if offset >= body.len() {
                    return;
                }
                let range = if offset > 0 {
                    format!(
                        "Content-Range: bytes {offset}-{}/{}\r\n",
                        body.len() - 1,
                        body.len()
                    )
                } else {
                    String::new()
                };
                let status = if offset > 0 { 206 } else { 200 };
                let headers = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nETag: \"fixture-v1\"\r\n{range}Connection: close\r\n\r\n",
                    body.len() - offset
                );
                if socket.write_all(headers.as_bytes()).await.is_err() {
                    return;
                }
                for chunk in body[offset..].chunks(4096) {
                    if socket.write_all(chunk).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            });
        }
    });
    format!("http://{address}/file.bin")
}

#[tokio::test]
async fn rapid_pause_resume_drains_each_writer_and_preserves_identity() {
    let bytes: Vec<u8> = (0..256_000).map(|index| (index % 251) as u8).collect();
    let url = spawn_slow_range_server(bytes.clone()).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url,
            destination_id: None,
            suggested_name: None,
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    for _ in 0..5 {
        wait_for_status(
            &mgr,
            &task.id,
            &[DownloadStatus::Downloading],
            Duration::from_secs(3),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(12)).await;
        let paused = mgr.pause(&task.id).await.unwrap();
        assert_eq!(paused.status, DownloadStatus::Paused);
        assert_eq!(paused.etag.as_deref(), Some("\"fixture-v1\""));
        let path = paused.temp_path.as_ref().unwrap();
        let length = std::fs::metadata(path).unwrap().len();
        assert_eq!(paused.downloaded_bytes, length);
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert_eq!(
            std::fs::metadata(path).unwrap().len(),
            length,
            "paused writer must be drained"
        );
        mgr.resume(&task.id).await.unwrap();
    }
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(5),
    )
    .await;
    let completed = mgr.get(&task.id).unwrap();
    assert_eq!(std::fs::read(completed.final_path.unwrap()).unwrap(), bytes);
}

#[tokio::test]
async fn remove_during_transfer_cannot_recreate_task_or_partial() {
    let url = spawn_slow_range_server(vec![7; 512_000]).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url,
            destination_id: None,
            suggested_name: None,
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Downloading],
        Duration::from_secs(3),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    mgr.remove(RemoveDownloadRequest {
        id: task.id.clone(),
        delete_partial: true,
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(mgr.get(&task.id).unwrap_err().code, ErrorCode::TaskNotFound);
    assert!(!std::path::Path::new(task.temp_path.as_ref().unwrap()).exists());
    assert!(!std::path::Path::new(task.final_path.as_ref().unwrap()).exists());
}

#[tokio::test]
async fn finalization_never_replaces_a_file_that_appears_during_transfer() {
    let url = spawn_fixture_server(|_| (200, b"download".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url,
            destination_id: None,
            suggested_name: Some("external.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let final_path = task.final_path.as_ref().unwrap();
    std::fs::write(final_path, b"external owner").unwrap();
    mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        mgr.get(&task.id).unwrap().error.unwrap().code,
        ErrorCode::AlreadyExists
    );
    assert_eq!(std::fs::read(final_path).unwrap(), b"external owner");
    assert_eq!(std::fs::read(task.temp_path.unwrap()).unwrap(), b"download");
}

#[test]
fn simultaneous_creates_reserve_names_and_request_ids() {
    let (_dir, mgr) = manager();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let mgr = mgr.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                mgr.create(CreateDownloadRequest {
                    url: "https://example.com/file.bin".into(),
                    destination_id: None,
                    suggested_name: Some("file.bin".into()),
                    start_now: false,
                    request_id: Some(format!("request-{}", index / 2)),
                    expected_sha256: None,
                })
                .unwrap()
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let tasks = mgr.list().unwrap();
    assert_eq!(tasks.len(), 4);
    let names: std::collections::HashSet<_> = tasks
        .iter()
        .map(|task| task.final_path.clone().unwrap())
        .collect();
    assert_eq!(names.len(), 4);
    for task in tasks {
        assert_eq!(
            std::path::Path::new(&task.final_path.unwrap())
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            task.file_name
        );
    }
}

#[tokio::test]
async fn invalid_or_duplicate_start_actions_do_not_cancel_the_valid_writer() {
    let bytes = vec![9; 256_000];
    let url = spawn_slow_range_server(bytes.clone()).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url,
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let (first, second) = tokio::join!(mgr.resume(&task.id), mgr.resume(&task.id));
    assert_ne!(first.is_ok(), second.is_ok());
    let error = if let Err(error) = first {
        error
    } else {
        second.unwrap_err()
    };
    assert_eq!(error.code, ErrorCode::InvalidTransition);
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Downloading],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        mgr.resume(&task.id).await.unwrap_err().code,
        ErrorCode::InvalidTransition
    );
    assert_eq!(
        mgr.retry(&task.id).await.unwrap_err().code,
        ErrorCode::InvalidTransition
    );
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        std::fs::read(mgr.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
        bytes
    );
}

#[tokio::test]
async fn supplied_sha256_is_compared_and_retained_without_publishing_mismatches() {
    let bytes = b"checksum-content".to_vec();
    let actual = sha256_hex(&bytes);
    let base = spawn_fixture_server(move |_| (200, bytes.clone())).await;
    for expected in [None, Some(actual.to_uppercase()), Some("0".repeat(64))] {
        let (_dir, mgr) = manager();
        let task = mgr
            .create(CreateDownloadRequest {
                url: format!("{base}/checksum.bin?signature=private"),
                destination_id: None,
                suggested_name: None,
                start_now: true,
                request_id: None,
                expected_sha256: expected.clone(),
            })
            .unwrap();
        wait_for_status(
            &mgr,
            &task.id,
            &[DownloadStatus::Completed, DownloadStatus::Failed],
            Duration::from_secs(5),
        )
        .await;
        let done = mgr.get(&task.id).unwrap();
        assert_eq!(done.actual_sha256.as_deref(), Some(actual.as_str()));
        assert!(!serde_json::to_string(&done).unwrap().contains("private"));
        assert!(
            done.url.contains("signature=private"),
            "signed URL stays private in SQLite for resumption"
        );
        match expected {
            Some(value) if value == "0".repeat(64) => {
                assert_eq!(done.status, DownloadStatus::Failed);
                assert_eq!(done.checksum_result.as_deref(), Some("mismatch"));
                assert_eq!(done.error.unwrap().code, ErrorCode::ChecksumMismatch);
                assert!(!std::path::Path::new(done.final_path.as_ref().unwrap()).exists());
                assert!(std::path::Path::new(done.temp_path.as_ref().unwrap()).exists());
            }
            value => {
                assert_eq!(done.status, DownloadStatus::Completed);
                assert_eq!(
                    done.checksum_result.as_deref(),
                    Some(if value.is_some() {
                        "matched"
                    } else {
                        "computed"
                    })
                );
                assert_eq!(done.expected_sha256, value.map(|s| s.to_lowercase()));
                assert!(done.finalization.is_none());
            }
        }
    }
}

#[test]
fn rejects_invalid_checksum_and_unapproved_destination_before_creating_tasks() {
    let (_dir, mgr) = manager();
    for (checksum, destination) in [
        (Some("bad".into()), None),
        (None, Some("unapproved".into())),
    ] {
        assert!(
            mgr.create(CreateDownloadRequest {
                url: "https://example.com/file".into(),
                destination_id: destination,
                suggested_name: None,
                start_now: false,
                request_id: None,
                expected_sha256: checksum
            })
            .is_err()
        );
    }
    assert!(mgr.list().unwrap().is_empty());
}

#[test]
fn durable_finalization_recovers_every_publication_boundary() {
    for (checkpoint, partial, final_file) in [
        ("ready", true, false),
        ("ready", true, true),
        ("ready", false, true),
        ("cleanup", true, true),
        ("cleanup", false, true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::build_state(dir.path().into()).unwrap();
        let mut task = state
            .manager
            .create(CreateDownloadRequest {
                url: "https://example.com/recovery.bin".into(),
                destination_id: None,
                suggested_name: None,
                start_now: false,
                request_id: None,
                expected_sha256: Some(sha256_hex(b"durable")),
            })
            .unwrap();
        let temp = task.temp_path.clone().unwrap();
        let final_path = task.final_path.clone().unwrap();
        if partial {
            std::fs::write(&temp, b"durable").unwrap();
        }
        if final_file {
            std::fs::write(&final_path, b"durable").unwrap();
        }
        task.status = if checkpoint == "cleanup" {
            DownloadStatus::Completed
        } else {
            DownloadStatus::Verifying
        };
        task.actual_sha256 = Some(sha256_hex(b"durable"));
        task.checksum_result = Some("matched".into());
        task.finalization = Some(checkpoint.into());
        task.downloaded_bytes = 7;
        task.total_bytes = Some(7);
        Database::open(dir.path().join("circle-downloader.db"))
            .unwrap()
            .upsert_download(&task)
            .unwrap();
        drop(state);
        let recovered = crate::build_state(dir.path().into()).unwrap();
        let done = recovered.manager.get(&task.id).unwrap();
        assert_eq!(
            done.status,
            DownloadStatus::Completed,
            "{checkpoint} {partial} {final_file}"
        );
        assert_eq!(std::fs::read(&final_path).unwrap(), b"durable");
        assert!(!std::path::Path::new(&temp).exists());
        assert!(done.temp_path.is_none());
        assert!(done.finalization.is_none());
        assert_eq!(done.actual_sha256, task.actual_sha256);
        drop(recovered);
        assert_eq!(
            crate::build_state(dir.path().into())
                .unwrap()
                .manager
                .get(&task.id)
                .unwrap()
                .status,
            DownloadStatus::Completed,
            "recovery is idempotent"
        );
    }
}

#[test]
fn recovery_preserves_conflicts_and_detects_missing_or_changed_files() {
    for scenario in [
        "conflict",
        "changed-partial",
        "missing-both",
        "missing-completed",
        "changed-completed",
        "orphan-final",
        "missing-partial",
        "paused-length",
        "unknown-checkpoint",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::build_state(dir.path().into()).unwrap();
        let mut task = state
            .manager
            .create(CreateDownloadRequest {
                url: "https://example.com/recovery.bin".into(),
                destination_id: None,
                suggested_name: None,
                start_now: false,
                request_id: None,
                expected_sha256: None,
            })
            .unwrap();
        let temp = task.temp_path.clone().unwrap();
        let final_path = task.final_path.clone().unwrap();
        task.actual_sha256 = Some(sha256_hex(b"durable"));
        task.checksum_result = Some("computed".into());
        task.downloaded_bytes = 7;
        match scenario {
            "conflict" => {
                task.finalization = Some("ready".into());
                std::fs::write(&temp, b"durable").unwrap();
                std::fs::write(&final_path, b"external").unwrap();
            }
            "changed-partial" => {
                task.finalization = Some("ready".into());
                std::fs::write(&temp, b"changed").unwrap();
            }
            "missing-both" => task.finalization = Some("ready".into()),
            "unknown-checkpoint" => task.finalization = Some("future".into()),
            "missing-completed" => {
                task.status = DownloadStatus::Completed;
                task.temp_path = None;
            }
            "changed-completed" => {
                task.status = DownloadStatus::Completed;
                task.temp_path = None;
                std::fs::write(&final_path, b"changed").unwrap();
            }
            "orphan-final" => {
                std::fs::write(&final_path, b"external").unwrap();
            }
            "paused-length" => {
                std::fs::write(&temp, b"part").unwrap();
                task.total_bytes = Some(100);
            }
            _ => {}
        }
        Database::open(dir.path().join("circle-downloader.db"))
            .unwrap()
            .upsert_download(&task)
            .unwrap();
        drop(state);
        let recovered = crate::build_state(dir.path().into()).unwrap();
        let done = recovered.manager.get(&task.id).unwrap();
        if scenario == "paused-length" {
            assert_eq!(done.status, DownloadStatus::Paused);
            assert_eq!(done.downloaded_bytes, 4);
        } else {
            assert_eq!(done.status, DownloadStatus::Failed, "{scenario}");
            assert!(done.error.is_some());
        }
        if scenario == "conflict" {
            assert_eq!(std::fs::read(&temp).unwrap(), b"durable");
            assert_eq!(std::fs::read(&final_path).unwrap(), b"external");
        }
        if scenario == "orphan-final" {
            assert_eq!(std::fs::read(&final_path).unwrap(), b"external");
        }
        if scenario == "missing-partial" {
            assert_eq!(done.downloaded_bytes, 0);
            assert!(done.etag.is_none());
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn managed_download_rejects_partial_symlinks_without_touching_target() {
    let base = spawn_fixture_server(|_| (200, b"payload".to_vec())).await;
    let (dir, mgr) = manager();
    let victim = dir.path().join("victim");
    std::fs::write(&victim, b"private").unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/unsafe.bin"),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    std::os::unix::fs::symlink(&victim, task.temp_path.unwrap()).unwrap();
    assert_eq!(
        mgr.resume(&task.id).await.unwrap_err().code,
        ErrorCode::PathNotAllowed
    );
    assert_eq!(std::fs::read(&victim).unwrap(), b"private");
}

#[test]
fn canceled_publication_checkpoint_does_not_publish_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::build_state(dir.path().into()).unwrap();
    let mut task = state
        .manager
        .create(CreateDownloadRequest {
            url: "https://example.com/canceled".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    std::fs::write(task.temp_path.as_ref().unwrap(), b"ready").unwrap();
    task.status = DownloadStatus::Canceled;
    task.finalization = Some("ready".into());
    task.actual_sha256 = Some(sha256_hex(b"ready"));
    task.checksum_result = Some("computed".into());
    Database::open(dir.path().join("circle-downloader.db"))
        .unwrap()
        .upsert_download(&task)
        .unwrap();
    drop(state);
    let recovered = crate::build_state(dir.path().into()).unwrap();
    let done = recovered.manager.get(&task.id).unwrap();
    assert_eq!(done.status, DownloadStatus::Canceled);
    assert!(done.finalization.is_none());
    assert!(!std::path::Path::new(done.final_path.as_ref().unwrap()).exists());
}

#[test]
fn ipc_serialization_redacts_legacy_credentials_and_signed_queries() {
    let (_dir, mgr) = manager();
    let mut task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/file?signature=private".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    task.url = "https://legacy-user:private-password@example.com/private-signed-path/file?signature=private-query#private-fragment".into();
    let serialized = serde_json::to_value(task).unwrap();
    assert_eq!(serialized["url"], "https://example.com/");
    assert!(!serialized.to_string().contains("private"));
    assert_eq!(serialized["retryCount"], 0);
    assert!(serialized["nextRetryAt"].is_null());
}

async fn spawn_holding_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = socket.read(&mut buf).await;
                // Hold the connection so workers remain active for dispatch tests.
                tokio::time::sleep(Duration::from_secs(30)).await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn global_concurrency_caps_active_workers() {
    let base = spawn_holding_server().await;
    let (_dir, mgr) = manager();
    mgr.update_settings(UpdateSettingsRequest {
        max_concurrent_downloads: Some(2),
        ..Default::default()
    })
    .unwrap();
    for i in 0..4 {
        mgr.create(CreateDownloadRequest {
            url: format!("{base}/file-{i}.bin"),
            destination_id: None,
            suggested_name: Some(format!("file-{i}.bin")),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let tasks = mgr.list().unwrap();
    let active = tasks
        .iter()
        .filter(|t| {
            matches!(
                t.status,
                DownloadStatus::Connecting | DownloadStatus::Downloading
            )
        })
        .count();
    let queued = tasks
        .iter()
        .filter(|t| t.status == DownloadStatus::Queued && t.next_retry_at.is_none())
        .count();
    assert_eq!(active, 2);
    assert!(queued >= 2);
    mgr.shutdown().await;
}

#[tokio::test]
async fn per_host_fairness_limits_same_host_slots() {
    let a = spawn_holding_server().await;
    // Distinct host strings (127.0.0.1 vs localhost) exercise per-host fairness.
    let b = a.replacen("127.0.0.1", "localhost", 1);
    let (_dir, mgr) = manager();
    mgr.update_settings(UpdateSettingsRequest {
        max_concurrent_downloads: Some(4),
        ..Default::default()
    })
    .unwrap();
    for i in 0..3 {
        mgr.create(CreateDownloadRequest {
            url: format!("{a}/a-{i}.bin"),
            destination_id: None,
            suggested_name: Some(format!("a-{i}.bin")),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    }
    mgr.create(CreateDownloadRequest {
        url: format!("{b}/b.bin"),
        destination_id: None,
        suggested_name: Some("b.bin".into()),
        start_now: true,
        request_id: None,
        expected_sha256: None,
    })
    .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let tasks = mgr.list().unwrap();
    let host_a = tasks
        .iter()
        .filter(|t| t.host == "127.0.0.1")
        .filter(|t| {
            matches!(
                t.status,
                DownloadStatus::Connecting | DownloadStatus::Downloading
            )
        })
        .count();
    let host_b_active = tasks.iter().filter(|t| t.host == "localhost").any(|t| {
        matches!(
            t.status,
            DownloadStatus::Connecting | DownloadStatus::Downloading
        )
    });
    assert!(host_a <= 2, "same-host active={host_a}");
    assert!(host_b_active, "other host should get a slot");
    mgr.shutdown().await;
}

#[tokio::test]
async fn paused_intent_survives_restart_without_autostart() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let downloads = dir.path().join("downloads");
    let mgr = DownloadManager::new(
        Database::open(&db_path).unwrap(),
        downloads.clone(),
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/paused.bin".into(),
            destination_id: None,
            suggested_name: Some("paused.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    assert_eq!(task.status, DownloadStatus::Paused);
    drop(mgr);
    let mgr2 = DownloadManager::new(
        Database::open(&db_path).unwrap(),
        downloads,
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    mgr2.reconcile_on_startup().unwrap();
    let recovered = mgr2.get(&task.id).unwrap();
    assert_eq!(recovered.status, DownloadStatus::Paused);
    assert!(recovered.queue_position.is_none());
}

#[tokio::test]
async fn permanent_failures_are_not_auto_retried() {
    let base = spawn_fixture_server(|_| (404, b"missing".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/gone.bin"),
            destination_id: None,
            suggested_name: Some("gone.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(3),
    )
    .await;
    let done = mgr.get(&task.id).unwrap();
    assert_eq!(done.error.as_ref().unwrap().code, ErrorCode::HttpNotFound);
    assert!(!done.error.as_ref().unwrap().retryable);
    assert_eq!(done.retry_count, 0);
    assert!(done.next_retry_at.is_none());
}

#[tokio::test]
async fn retryable_failures_schedule_until_budget_then_fail() {
    let base = spawn_fixture_server(|_| (503, b"busy".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/flaky.bin"),
            destination_id: None,
            suggested_name: Some("flaky.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(5),
    )
    .await;
    let done = mgr.get(&task.id).unwrap();
    assert_eq!(
        done.error.as_ref().unwrap().code,
        ErrorCode::HttpServerError
    );
    assert_eq!(done.retry_count, crate::application::retry::RETRY_BUDGET);
    assert!(done.next_retry_at.is_none());
}

#[tokio::test]
async fn shutdown_cancels_active_workers() {
    let base = spawn_holding_server().await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: format!("{base}/held.bin"),
            destination_id: None,
            suggested_name: Some("held.bin".into()),
            start_now: true,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Connecting, DownloadStatus::Downloading],
        Duration::from_secs(2),
    )
    .await;
    mgr.shutdown().await;
    // After drain, no workers remain registered; further dispatch is refused.
    mgr.dispatch();
    let after = mgr.get(&task.id).unwrap();
    assert!(matches!(
        after.status,
        DownloadStatus::Paused
            | DownloadStatus::Canceled
            | DownloadStatus::Failed
            | DownloadStatus::Connecting
            | DownloadStatus::Downloading
            | DownloadStatus::Queued
    ));
}

#[tokio::test]
async fn honor_retry_after_when_present() {
    use crate::infrastructure::errors::parse_retry_after;
    use reqwest::header::HeaderValue;
    assert_eq!(
        parse_retry_after(Some(&HeaderValue::from_static("42"))),
        Some(42)
    );
    assert_eq!(
        parse_retry_after(Some(&HeaderValue::from_static("99999"))),
        Some(3600)
    );
}

#[tokio::test]
async fn destination_grants_resolve_and_reject_unknown_ids() {
    use crate::domain::GrantDestinationRequest;
    let dir = tempdir().unwrap();
    let granted = dir.path().join("granted");
    std::fs::create_dir_all(&granted).unwrap();
    let (_data, mgr) = manager();
    let destinations = mgr.list_destinations().unwrap();
    assert!(
        destinations
            .iter()
            .any(|item| item.id == "default" && item.is_default)
    );
    let granted_item = mgr
        .grant_destination(GrantDestinationRequest {
            path: granted.to_string_lossy().into_owned(),
            label: Some("Granted".into()),
        })
        .unwrap();
    assert_eq!(granted_item.label, "Granted");
    assert!(!granted_item.is_default);
    mgr.update_settings(UpdateSettingsRequest {
        default_destination_id: Some(granted_item.id.clone()),
        ..Default::default()
    })
    .unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/into-granted.bin".into(),
            destination_id: Some(granted_item.id.clone()),
            suggested_name: Some("into-granted.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let granted_canonical = std::fs::canonicalize(&granted).unwrap();
    assert!(
        task.final_path
            .as_deref()
            .is_some_and(|path| path.starts_with(granted_canonical.to_string_lossy().as_ref()))
    );
    let err = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/bad.bin".into(),
            destination_id: Some("missing-destination".into()),
            suggested_name: Some("bad.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PathNotAllowed);
    mgr.remove_destination(&granted_item.id).unwrap_err(); // still default
    mgr.update_settings(UpdateSettingsRequest {
        default_destination_id: Some("default".into()),
        ..Default::default()
    })
    .unwrap();
    // Active paused task still uses the folder.
    let in_use = mgr.remove_destination(&granted_item.id).unwrap_err();
    assert_eq!(in_use.code, ErrorCode::PathNotAllowed);
}

#[tokio::test]
async fn open_and_reveal_validate_paths_under_destination() {
    let (data, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/open-me.bin".into(),
            destination_id: None,
            suggested_name: Some("open-me.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let final_path = task
        .final_path
        .clone()
        .expect("create reserves a final path");
    let missing = mgr.open_download(&task.id).unwrap_err();
    assert_eq!(missing.code, ErrorCode::StorageNotFound);

    std::fs::write(&final_path, b"ok").unwrap();
    mgr.open_download(&task.id).unwrap();
    mgr.reveal_download(&task.id).unwrap();

    let db = Database::open(data.path().join("test.db")).unwrap();
    let mut poisoned = db.get_download(&task.id).unwrap();
    poisoned.final_path = Some(data.path().join("outside.bin").to_string_lossy().into());
    db.upsert_download(&poisoned).unwrap();
    let outside = mgr.open_download(&task.id).unwrap_err();
    assert_eq!(outside.code, ErrorCode::PathNotAllowed);
}

#[tokio::test]
async fn settings_reject_unknown_destination_and_out_of_range_concurrency() {
    let (_data, mgr) = manager();
    let bad_dest = mgr
        .update_settings(UpdateSettingsRequest {
            default_destination_id: Some("missing".into()),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(bad_dest.code, ErrorCode::PathNotAllowed);
    let bad_concurrency = mgr
        .update_settings(UpdateSettingsRequest {
            max_concurrent_downloads: Some(99),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(bad_concurrency.code, ErrorCode::Internal);
    let ok = mgr
        .update_settings(UpdateSettingsRequest {
            max_concurrent_downloads: Some(8),
            auto_start: Some(false),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(ok.max_concurrent_downloads, 8);
    assert!(!ok.auto_start);
}
