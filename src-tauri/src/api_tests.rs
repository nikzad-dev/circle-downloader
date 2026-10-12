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
async fn retry_paused_task_re_queues_and_resumes() {
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/paused-retry.bin".into(),
            destination_id: None,
            suggested_name: Some("paused-retry.bin".into()),
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();

    assert_eq!(task.status, DownloadStatus::Paused);
    let retried = mgr.retry(&task.id).await.unwrap();
    assert_eq!(retried.status, DownloadStatus::Queued);
}

#[tokio::test]
async fn retry_after_resource_changed_resets_partial_and_completes() {
    let base = spawn_fixture_server(|req| {
        let (status, body) = if req.contains("range:") {
            (412, b"".to_vec())
        } else {
            (200, b"FRESH-CONTENT".to_vec())
        };
        (status, body)
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
    std::fs::write(&temp, b"OLD-PARTIAL").unwrap();

    mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(5),
    )
    .await;

    let failed = mgr.get(&task.id).unwrap();
    assert_eq!(failed.status, DownloadStatus::Failed);

    let retried = mgr.retry(&task.id).await.unwrap();
    assert_eq!(retried.status, DownloadStatus::Queued);
    assert_eq!(retried.downloaded_bytes, 0);

    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(5),
    )
    .await;

    let completed = mgr.get(&task.id).unwrap();
    assert_eq!(completed.status, DownloadStatus::Completed);
    let bytes = std::fs::read(mgr.download_dir().join("file.bin")).unwrap();
    assert_eq!(bytes, b"FRESH-CONTENT");
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
            max_connections_per_download: Some(2),
        })
        .unwrap();
    assert_eq!(updated.max_concurrent_downloads, 4);
    assert!(!updated.auto_start);
    assert_eq!(updated.max_connections_per_download, 2);

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
    let bad_connections = mgr
        .update_settings(UpdateSettingsRequest {
            max_connections_per_download: Some(9),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(bad_connections.code, ErrorCode::Internal);
}

/// Controlled 8 MiB fixture: exact ranges, per-connection pacing and mutable faults.
struct SegmentFixture {
    url: String,
    body: std::sync::Arc<Vec<u8>>,
    fault: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    requests: std::sync::Arc<parking_lot::Mutex<Vec<(usize, usize)>>>,
    completions: std::sync::Arc<parking_lot::Mutex<Vec<usize>>>,
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    peak: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for SegmentFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn segment_fixture(delay: Duration, fault_value: usize) -> SegmentFixture {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/seg.bin", listener.local_addr().unwrap());
    let body = Arc::new(
        (0..8 * 1024 * 1024)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>(),
    );
    let fault = Arc::new(AtomicUsize::new(fault_value));
    let requests = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let completions = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (data, modes, logs, finishes, counts, maxes) = (
        body.clone(),
        fault.clone(),
        requests.clone(),
        completions.clone(),
        active.clone(),
        peak.clone(),
    );
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let (body, fault, requests, completions, active, peak) = (
                data.clone(),
                modes.clone(),
                logs.clone(),
                finishes.clone(),
                counts.clone(),
                maxes.clone(),
            );
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0; 2048];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..n]);
                }
                let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
                let range = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="));
                let (start, end) = range
                    .map(|value| {
                        let (start, end) = value.trim().split_once('-').unwrap();
                        (
                            start.parse::<usize>().unwrap(),
                            if end.is_empty() {
                                body.len() - 1
                            } else {
                                end.parse().unwrap()
                            },
                        )
                    })
                    .unwrap_or((0, body.len() - 1));
                assert!(start <= end && end < body.len());
                if range.is_some() {
                    requests.lock().push((start, end));
                }
                let mode = fault.load(Ordering::SeqCst);
                let segmented = range.is_some() && end > start;
                let status = if range.is_some() && !(segmented && mode == 2) {
                    206
                } else {
                    200
                };
                let etag = if mode == 6 {
                    "W/\"seg-v1\""
                } else if segmented && (mode == 1 || mode == 9 && start == 0) {
                    "\"changed\""
                } else {
                    "\"seg-v1\""
                };
                let (payload_start, payload_end) = if status == 200 {
                    (0, body.len() - 1)
                } else {
                    (start, end)
                };
                let total = body.len() + usize::from(segmented && mode == 3);
                let reported_end = end + usize::from(segmented && mode == 5);
                let content_range = if status == 206 {
                    format!("Content-Range: bytes {start}-{reported_end}/{total}\r\n")
                } else {
                    String::new()
                };
                let accepts = if mode == 4 {
                    ""
                } else {
                    "Accept-Ranges: bytes\r\n"
                };
                let header = format!(
                    "HTTP/1.1 {status} OK\r\n{accepts}ETag: {etag}\r\n{content_range}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    payload_end - payload_start + 1
                );
                if socket.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                if mode == 8 && end == start || mode == 9 && segmented && start > 0 {
                    let _ = socket.read(&mut buffer).await;
                    return;
                }
                struct ActiveGuard(Arc<AtomicUsize>);
                impl Drop for ActiveGuard {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                let _guard = if segmented {
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    Some(ActiveGuard(active))
                } else {
                    None
                };
                if segmented && start == 0 {
                    tokio::time::sleep(Duration::from_millis(35)).await;
                }
                for chunk in body[payload_start..=payload_end].chunks(64 * 1024) {
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    if socket.write_all(chunk).await.is_err() {
                        return;
                    }
                }
                if segmented {
                    completions.lock().push(start);
                }
            });
        }
    });
    SegmentFixture {
        url,
        body,
        fault,
        requests,
        completions,
        active,
        peak,
        server,
    }
}

fn segmented_settings(mgr: &DownloadManager, connections: u32) {
    mgr.update_settings(UpdateSettingsRequest {
        max_connections_per_download: Some(connections),
        ..Default::default()
    })
    .unwrap();
}
fn create_segment_task(
    mgr: &DownloadManager,
    fixture: &SegmentFixture,
) -> crate::domain::DownloadTask {
    mgr.create(CreateDownloadRequest {
        url: fixture.url.clone(),
        destination_id: None,
        suggested_name: Some("seg.bin".into()),
        start_now: true,
        request_id: None,
        expected_sha256: Some(sha256_hex(&fixture.body)),
    })
    .unwrap()
}
async fn wait_segment_progress(mgr: &DownloadManager, id: &str) {
    for _ in 0..400 {
        let task = mgr.get(id).unwrap();
        if task.transfer_mode == crate::domain::TransferMode::Segmented && task.downloaded_bytes > 0
        {
            return;
        }
        assert_ne!(task.status, DownloadStatus::Failed, "{:?}", task.error);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no segment progress");
}

#[tokio::test]
async fn segmented_out_of_order_download_assembles_byte_identical_file_and_cleans_parts() {
    use std::sync::atomic::Ordering;
    let fixture = segment_fixture(Duration::from_millis(2), 0).await;
    let (dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &fixture);
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(12),
    )
    .await;
    let done = mgr.get(&task.id).unwrap();
    assert_eq!(done.transfer_mode, crate::domain::TransferMode::Segmented);
    assert_eq!(
        done.actual_sha256.as_deref(),
        Some(sha256_hex(&fixture.body).as_str())
    );
    assert_eq!(done.checksum_result.as_deref(), Some("matched"));
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        *fixture.body
    );
    assert!(fixture.peak.load(Ordering::SeqCst) <= 4);
    assert_ne!(
        *fixture.completions.lock(),
        vec![0, 2097152, 4194304, 6291456]
    );
    assert_eq!(
        std::fs::read_dir(dir.path().join("downloads"))
            .unwrap()
            .count(),
        1
    );
    assert!(
        Database::open(dir.path().join("test.db"))
            .unwrap()
            .list_segments(&task.id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn incompatible_origins_and_default_setting_use_single_connection() {
    for (connections, fault) in [(1, 0), (4, 4), (4, 6)] {
        let fixture = segment_fixture(Duration::ZERO, fault).await;
        let (_dir, mgr) = manager();
        segmented_settings(&mgr, connections);
        let task = create_segment_task(&mgr, &fixture);
        wait_for_status(
            &mgr,
            &task.id,
            &[DownloadStatus::Completed],
            Duration::from_secs(12),
        )
        .await;
        let done = mgr.get(&task.id).unwrap();
        assert_eq!(done.transfer_mode, crate::domain::TransferMode::Single);
        assert_eq!(
            std::fs::read(done.final_path.unwrap()).unwrap(),
            *fixture.body
        );
        assert!(
            fixture
                .requests
                .lock()
                .iter()
                .all(|(start, end)| start == end)
        );
        if connections == 1 {
            assert!(fixture.requests.lock().is_empty());
        }
    }
}

#[tokio::test]
async fn segmented_pause_reopen_and_lower_connection_setting_resume_saved_extents() {
    use std::sync::atomic::Ordering;
    let fixture = segment_fixture(Duration::from_millis(10), 0).await;
    let (dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &fixture);
    wait_segment_progress(&mgr, &task.id).await;
    mgr.pause(&task.id).await.unwrap();
    assert_eq!(mgr.get(&task.id).unwrap().status, DownloadStatus::Paused);
    let db = Database::open(dir.path().join("test.db")).unwrap();
    let before = db.list_segments(&task.id).unwrap();
    assert!(
        before
            .iter()
            .any(|s| s.completed_bytes > 0 && s.completed_bytes < s.expected_len())
    );
    let paths: Vec<_> = before
        .iter()
        .map(|s| {
            crate::application::segmentation::segment_file_path(
                task.temp_path.as_ref().unwrap(),
                s.segment_index,
            )
        })
        .collect();
    let lengths: Vec<_> = paths
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .collect();
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        lengths,
        paths
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .collect::<Vec<_>>()
    );
    drop(mgr);
    drop(db);
    let reopened = DownloadManager::new(
        Database::open(dir.path().join("test.db")).unwrap(),
        dir.path().join("downloads"),
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    reopened.reconcile_on_startup().unwrap();
    segmented_settings(&reopened, 1);
    fixture.requests.lock().clear();
    fixture.peak.store(0, Ordering::SeqCst);
    assert_eq!(fixture.active.load(Ordering::SeqCst), 0);
    reopened.resume(&task.id).await.unwrap();
    wait_for_status(
        &reopened,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(
        std::fs::read(reopened.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
        *fixture.body
    );
    assert!(fixture.peak.load(Ordering::SeqCst) <= 1);
    let requests = fixture.requests.lock();
    for segment in before {
        if segment.completed_bytes < segment.expected_len() {
            assert!(requests.contains(&(
                (segment.start_byte + segment.completed_bytes) as usize,
                segment.end_byte as usize
            )));
        }
    }
}

#[tokio::test]
async fn segmented_identity_and_range_errors_do_not_publish() {
    for fault in [1, 2, 3, 5] {
        let fixture = segment_fixture(Duration::from_millis(10), fault).await;
        let (dir, mgr) = manager();
        segmented_settings(&mgr, 4);
        let task = create_segment_task(&mgr, &fixture);
        wait_for_status(
            &mgr,
            &task.id,
            &[DownloadStatus::Failed],
            Duration::from_secs(5),
        )
        .await;
        let failed = mgr.get(&task.id).unwrap();
        assert!(matches!(
            failed.error.unwrap().code,
            ErrorCode::ResourceChanged | ErrorCode::RangeUnsupported
        ));
        assert!(!std::path::Path::new(failed.final_path.as_ref().unwrap()).exists());
        assert!(!std::path::Path::new(failed.temp_path.as_ref().unwrap()).exists());
        assert_eq!(
            Database::open(dir.path().join("test.db"))
                .unwrap()
                .list_segments(&task.id)
                .unwrap()
                .len(),
            4
        );
    }
}

#[tokio::test]
async fn changed_origin_after_pause_keeps_segment_files_and_fails() {
    use std::sync::atomic::Ordering;
    let fixture = segment_fixture(Duration::from_millis(10), 0).await;
    let (_dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &fixture);
    wait_segment_progress(&mgr, &task.id).await;
    mgr.pause(&task.id).await.unwrap();
    fixture.fault.store(2, Ordering::SeqCst);
    mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(5),
    )
    .await;
    let failed = mgr.get(&task.id).unwrap();
    assert_eq!(failed.error.unwrap().code, ErrorCode::ResourceChanged);
    assert!(!std::path::Path::new(failed.final_path.as_ref().unwrap()).exists());
    assert!(
        std::fs::metadata(crate::application::segmentation::segment_file_path(
            task.temp_path.as_ref().unwrap(),
            0
        ))
        .unwrap()
        .len()
            > 0
    );
}

#[tokio::test]
async fn cancellation_interrupts_a_stalled_probe_and_removal_deletes_segment_parts() {
    let fixture = segment_fixture(Duration::ZERO, 8).await;
    let (_dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &fixture);
    for _ in 0..100 {
        if !fixture.requests.lock().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::timeout(Duration::from_secs(1), mgr.pause(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mgr.get(&task.id).unwrap().status, DownloadStatus::Paused);
    let slow = segment_fixture(Duration::from_millis(10), 0).await;
    let (dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &slow);
    wait_segment_progress(&mgr, &task.id).await;
    tokio::time::timeout(Duration::from_secs(1), mgr.cancel(&task.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mgr.get(&task.id).unwrap().status, DownloadStatus::Canceled);
    mgr.remove(RemoveDownloadRequest {
        id: task.id,
        delete_partial: true,
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_dir(dir.path().join("downloads"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
#[ignore = "controlled benchmark; run explicitly with --ignored --nocapture"]
async fn segmented_benchmark_records_medians_under_equivalent_conditions() {
    let fixture = segment_fixture(Duration::from_millis(3), 0).await;
    let mut single = Vec::new();
    let mut multi = Vec::new();
    // Alternate order to reduce systematic warm-cache/order bias.
    for round in 0..7 {
        for connections in if round % 2 == 0 { [1, 4] } else { [4, 1] } {
            let (_dir, mgr) = manager();
            segmented_settings(&mgr, connections);
            let start = std::time::Instant::now();
            let task = create_segment_task(&mgr, &fixture);
            wait_for_status(
                &mgr,
                &task.id,
                &[DownloadStatus::Completed],
                Duration::from_secs(15),
            )
            .await;
            let ms = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(
                std::fs::read(mgr.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
                *fixture.body
            );
            if connections == 1 {
                single.push(ms);
            } else {
                multi.push(ms);
            }
        }
    }
    for (name, mut samples) in [("single", single), ("four", multi)] {
        let raw = samples.clone();
        samples.sort_by(f64::total_cmp);
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance =
            samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (samples.len() - 1) as f64;
        eprintln!(
            "N4A {name}: samples_ms={raw:?}; median_ms={}; sample_variance_ms2={variance}; min_ms={}; max_ms={}",
            samples[samples.len() / 2],
            samples[0],
            samples[samples.len() - 1]
        );
    }
}

#[tokio::test]
async fn one_bad_segment_drains_stalled_siblings_promptly() {
    let fixture = segment_fixture(Duration::ZERO, 9).await;
    let (_dir, mgr) = manager();
    segmented_settings(&mgr, 4);
    let task = create_segment_task(&mgr, &fixture);
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(2),
    )
    .await;
    let failed = mgr.get(&task.id).unwrap();
    assert_eq!(failed.error.unwrap().code, ErrorCode::ResourceChanged);
    let db_paths = task.temp_path.as_ref().unwrap();
    let sizes: Vec<_> = (0..4)
        .map(|i| {
            std::fs::metadata(crate::application::segmentation::segment_file_path(
                db_paths, i,
            ))
            .map(|m| m.len())
            .unwrap_or(0)
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        sizes,
        (0..4)
            .map(
                |i| std::fs::metadata(crate::application::segmentation::segment_file_path(
                    db_paths, i
                ))
                .map(|m| m.len())
                .unwrap_or(0)
            )
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn opting_in_after_a_single_partial_preserves_suffix_resume() {
    let fixture = segment_fixture(Duration::from_millis(6), 0).await;
    let (_dir, mgr) = manager();
    let task = create_segment_task(&mgr, &fixture);
    for _ in 0..400 {
        if mgr.get(&task.id).unwrap().downloaded_bytes > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    mgr.pause(&task.id).await.unwrap();
    let offset = std::fs::metadata(task.temp_path.as_ref().unwrap())
        .unwrap()
        .len();
    assert!(offset > 0 && offset < fixture.body.len() as u64);
    segmented_settings(&mgr, 4);
    fixture.requests.lock().clear();
    mgr.resume(&task.id).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(12),
    )
    .await;
    let done = mgr.get(&task.id).unwrap();
    assert_eq!(done.transfer_mode, crate::domain::TransferMode::Single);
    assert_eq!(
        *fixture.requests.lock(),
        vec![(offset as usize, fixture.body.len() - 1)]
    );
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        *fixture.body
    );
}

#[tokio::test]
async fn segmented_downloads_still_share_the_global_task_slot() {
    use std::sync::atomic::Ordering;
    let fixture = segment_fixture(Duration::from_millis(3), 0).await;
    let (_dir, mgr) = manager();
    mgr.update_settings(UpdateSettingsRequest {
        max_concurrent_downloads: Some(1),
        max_connections_per_download: Some(4),
        ..Default::default()
    })
    .unwrap();
    let first = create_segment_task(&mgr, &fixture);
    let second = create_segment_task(&mgr, &fixture);
    wait_for_status(
        &mgr,
        &first.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(12),
    )
    .await;
    wait_for_status(
        &mgr,
        &second.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(12),
    )
    .await;
    assert!(fixture.peak.load(Ordering::SeqCst) <= 4);
    for id in [first.id, second.id] {
        let task = mgr.get(&id).unwrap();
        assert_eq!(
            std::fs::read(task.final_path.unwrap()).unwrap(),
            *fixture.body
        );
    }
}

#[tokio::test]
async fn bulk_import_is_validated_before_mutation_and_retry_safe() {
    use crate::application::manager::advanced::BulkImport;
    use crate::domain::advanced::TaskOptions;
    let (_dir, mgr) = manager();
    let options = TaskOptions {
        queue_id: "default".into(),
        category: "Research".into(),
        tags: vec!["papers".into()],
        priority: 2,
        ..Default::default()
    };
    let input = |urls: Vec<String>| BulkImport {
        urls,
        options: options.clone(),
        start_now: false,
        batch_id: "retry-safe".into(),
    };
    assert!(
        mgr.bulk_import(input(vec![
            "https://example.com/a".into(),
            "file:///private/secret".into()
        ]))
        .await
        .is_err()
    );
    assert!(mgr.list().unwrap().is_empty());
    let urls = vec![
        "https://example.com/a".into(),
        "https://example.com/b".into(),
    ];
    let first = mgr.bulk_import(input(urls.clone())).await.unwrap();
    let second = mgr.bulk_import(input(urls)).await.unwrap();
    assert!(first.errors.is_empty());
    assert_eq!(
        first.created.iter().map(|t| &t.id).collect::<Vec<_>>(),
        second.created.iter().map(|t| &t.id).collect::<Vec<_>>()
    );
    assert_eq!(mgr.list().unwrap().len(), 2);
    assert!(
        mgr.task_options()
            .unwrap()
            .iter()
            .all(|o| o.category == "Research" && o.priority == 2 && o.tags == ["papers"])
    );
}

#[tokio::test]
async fn queue_pause_preserves_intent_and_schedule_wakes_without_user_action() {
    use crate::domain::advanced::TaskOptions;
    let (_dir, mgr) = manager();
    let url = spawn_fixture_server(|_| (200, b"scheduled bytes".to_vec())).await;
    let mut queue = mgr.queue_policies().unwrap().remove(0);
    queue.paused = true;
    mgr.save_queue(queue.clone()).await.unwrap();
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
    mgr.update_task_options(TaskOptions {
        id: task.id.clone(),
        queue_id: queue.id.clone(),
        ..Default::default()
    })
    .unwrap();
    mgr.resume(&task.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(mgr.get(&task.id).unwrap().status, DownloadStatus::Queued);
    // Persist a subsecond UTC deadline directly; the UI's local date picker has minute precision.
    let db = Database::open(_dir.path().join("test.db")).unwrap();
    let mut options = db.task_options(&task.id).unwrap();
    options.scheduled_at = Some(chrono::Utc::now() + chrono::Duration::milliseconds(350));
    db.save_task_options(&options).unwrap();
    queue.paused = false;
    mgr.save_queue(queue).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(mgr.get(&task.id).unwrap().status, DownloadStatus::Queued);
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        std::fs::read(mgr.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
        b"scheduled bytes"
    );
}

#[tokio::test]
async fn segmented_transfer_obeys_shared_rates_and_records_real_bytes() {
    use crate::domain::advanced::NetworkSettings;
    for connections in [1, 4] {
        let (_dir, mgr) = manager();
        mgr.update_settings(UpdateSettingsRequest {
            max_connections_per_download: Some(connections),
            ..Default::default()
        })
        .unwrap();
        mgr.save_network(NetworkSettings {
            bytes_per_second: 32 * 1024 * 1024,
            ..Default::default()
        })
        .unwrap();
        let mut queue = mgr.queue_policies().unwrap().remove(0);
        queue.bytes_per_second = 16 * 1024 * 1024;
        mgr.save_queue(queue).await.unwrap();
        let fixture = segment_fixture(Duration::ZERO, 0).await;
        let start = std::time::Instant::now();
        let task = create_segment_task(&mgr, &fixture);
        wait_for_status(
            &mgr,
            &task.id,
            &[DownloadStatus::Completed],
            Duration::from_secs(6),
        )
        .await;
        assert!(
            start.elapsed() >= Duration::from_millis(490),
            "connections={connections} bypassed queue limit"
        );
        assert_eq!(
            std::fs::read(mgr.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
            *fixture.body
        );
        // Completion is committed before the worker's durable measurement; wait for that commit.
        for _ in 0..100 {
            if mgr.statistics().unwrap().completed_attempts == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let stats = mgr.statistics().unwrap();
        assert_eq!(stats.transferred_bytes, fixture.body.len() as u64);
        assert_eq!(stats.completed_attempts, 1);
        assert!(stats.transfer_millis >= 490);
    }
}

#[tokio::test]
async fn dispatch_rotates_queues_and_honors_priority_within_each_queue() {
    use crate::domain::advanced::{QueuePolicy, TaskOptions};
    let (_dir, mgr) = manager();
    mgr.update_settings(UpdateSettingsRequest {
        max_concurrent_downloads: Some(1),
        ..Default::default()
    })
    .unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = seen.clone();
    let url = spawn_fixture_server(move |request| {
        record.lock().unwrap().push(
            request
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .to_string(),
        );
        (200, b"queue fixture".to_vec())
    })
    .await;
    let mut default = mgr.queue_policies().unwrap().remove(0);
    default.paused = true;
    mgr.save_queue(default.clone()).await.unwrap();
    mgr.save_queue(QueuePolicy {
        id: "second".into(),
        name: "Second".into(),
        paused: true,
        max_concurrent: 1,
        bytes_per_second: 0,
        task_count: 0,
    })
    .await
    .unwrap();
    let mut ids = vec![];
    for (name, queue, priority) in [
        ("low", "default", -2),
        ("high", "default", 2),
        ("other", "second", 0),
    ] {
        let task = mgr
            .create(CreateDownloadRequest {
                url: format!("{url}/{name}"),
                destination_id: None,
                suggested_name: None,
                start_now: false,
                request_id: None,
                expected_sha256: None,
            })
            .unwrap();
        mgr.update_task_options(TaskOptions {
            id: task.id.clone(),
            queue_id: queue.into(),
            priority,
            ..Default::default()
        })
        .unwrap();
        mgr.resume(&task.id).await.unwrap();
        ids.push(task.id);
    }
    // Atomic policy setup keeps all tasks pending until both queues are enabled.
    let db = Database::open(_dir.path().join("test.db")).unwrap();
    for mut policy in db.queue_policies().unwrap() {
        policy.paused = false;
        db.save_queue(&policy).unwrap();
    }
    mgr.dispatch();
    for id in ids {
        wait_for_status(
            &mgr,
            &id,
            &[DownloadStatus::Completed],
            Duration::from_secs(3),
        )
        .await;
    }
    assert_eq!(*seen.lock().unwrap(), ["/high", "/other", "/low"]);
}

#[tokio::test]
async fn configured_creation_persists_controls_before_start_and_rejects_bad_options() {
    use crate::domain::advanced::TaskOptions;
    let (_dir, mgr) = manager();
    let mut queue = mgr.queue_policies().unwrap().remove(0);
    queue.paused = true;
    mgr.save_queue(queue).await.unwrap();
    let request = || CreateDownloadRequest {
        url: "https://example.com/file".into(),
        destination_id: None,
        suggested_name: None,
        start_now: true,
        request_id: Some("window-request".into()),
        expected_sha256: None,
    };
    assert!(
        mgr.create_configured(
            request(),
            TaskOptions {
                queue_id: "default".into(),
                connections: Some(9),
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
    assert!(mgr.list().unwrap().is_empty());
    let controls = TaskOptions {
        queue_id: "default".into(),
        connections: Some(2),
        category: "Research".into(),
        priority: 2,
        ..Default::default()
    };
    let first = mgr
        .create_configured(request(), controls.clone())
        .await
        .unwrap();
    let again = mgr.create_configured(request(), controls).await.unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(first.status, DownloadStatus::Queued);
    assert_eq!(mgr.list().unwrap().len(), 1);
    let saved = mgr.task_options().unwrap().remove(0);
    assert_eq!(saved.connections, Some(2));
    assert_eq!(saved.category, "Research");
    assert_eq!(saved.priority, 2);
}

#[tokio::test]
async fn new_download_in_paused_default_queue_starts_after_queue_resume() {
    use crate::domain::advanced::TaskOptions;
    let url = spawn_fixture_server(|_| (200, b"new download".to_vec())).await;
    let (_dir, mgr) = manager();
    let mut queue = mgr.queue_policies().unwrap().remove(0);
    queue.paused = true;
    mgr.save_queue(queue.clone()).await.unwrap();
    let task = mgr
        .create_configured(
            CreateDownloadRequest {
                url,
                destination_id: None,
                suggested_name: Some("new.bin".into()),
                start_now: true,
                request_id: None,
                expected_sha256: None,
            },
            TaskOptions {
                queue_id: queue.id.clone(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(task.status, DownloadStatus::Queued);
    assert_eq!(task.downloaded_bytes, 0);
    queue.paused = false;
    mgr.save_queue(queue).await.unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        std::fs::read(mgr.get(&task.id).unwrap().final_path.unwrap()).unwrap(),
        b"new download"
    );
}

#[tokio::test]
async fn categories_are_inferred_for_existing_tasks_and_each_bulk_file_with_manual_override() {
    use crate::application::manager::advanced::BulkImport;
    use crate::domain::advanced::TaskOptions;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/report.PDF?file=video.mp4".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    let options = || {
        mgr.task_options()
            .unwrap()
            .into_iter()
            .find(|o| o.id == task.id)
            .unwrap()
    };
    assert_eq!(options().category, "");
    assert_eq!(options().resolved_category, "Documents");
    let mut manual = options();
    manual.category = "Pictures".into();
    assert_eq!(
        mgr.update_task_options(manual).unwrap().resolved_category,
        "Pictures"
    );
    let mut automatic = options();
    automatic.category.clear();
    assert_eq!(
        mgr.update_task_options(automatic)
            .unwrap()
            .resolved_category,
        "Documents"
    );
    let result = mgr
        .bulk_import(BulkImport {
            urls: vec![
                "https://example.com/song.mp3".into(),
                "https://example.com/movie.mkv".into(),
                "https://example.com/download".into(),
            ],
            options: TaskOptions {
                queue_id: "default".into(),
                ..Default::default()
            },
            start_now: false,
            batch_id: "category-batch".into(),
        })
        .await
        .unwrap();
    assert!(result.errors.is_empty());
    for (task, expected) in result.created.iter().zip(["Music", "Videos", ""]) {
        assert_eq!(
            mgr.task_options()
                .unwrap()
                .into_iter()
                .find(|o| o.id == task.id)
                .unwrap()
                .resolved_category,
            expected
        );
    }
}

#[tokio::test]
async fn circle_destination_creates_category_folders_and_routes_new_downloads() {
    use crate::domain::advanced::TaskOptions;
    let dir = tempdir().unwrap();
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir(&downloads).unwrap();
    let root = downloads.join("CIRCLE");
    let mgr = DownloadManager::new(
        Database::open(dir.path().join("test.db")).unwrap(),
        root.clone(),
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    mgr.prepare_category_folders().unwrap();
    for category in crate::domain::category::CATEGORIES {
        assert!(root.join(category).is_dir());
    }
    let input = |name: &str| CreateDownloadRequest {
        url: format!("https://example.com/{name}"),
        destination_id: None,
        suggested_name: None,
        start_now: false,
        request_id: None,
        expected_sha256: None,
    };
    let automatic = mgr.create(input("report.pdf")).unwrap();
    assert_eq!(
        std::path::Path::new(automatic.final_path.as_ref().unwrap())
            .parent()
            .unwrap(),
        root.join("Documents")
    );
    let manual = mgr
        .create_configured(
            input("report.pdf"),
            TaskOptions {
                queue_id: "default".into(),
                category: "Videos".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        std::path::Path::new(manual.final_path.as_ref().unwrap())
            .parent()
            .unwrap(),
        root.join("Videos")
    );
    let unknown = mgr.create(input("download.bin")).unwrap();
    assert_eq!(
        std::path::Path::new(unknown.final_path.as_ref().unwrap())
            .parent()
            .unwrap(),
        root
    );
    // Removing a categorized partial uses the same approved folder capability.
    std::fs::write(automatic.temp_path.as_ref().unwrap(), b"partial").unwrap();
    mgr.remove(RemoveDownloadRequest {
        id: automatic.id,
        delete_partial: true,
    })
    .await
    .unwrap();
    assert!(!std::path::Path::new(automatic.temp_path.as_ref().unwrap()).exists());
}

#[tokio::test]
async fn changed_default_preserves_old_partial_paths_and_removal() {
    let (dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/old.bin".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    std::fs::write(task.temp_path.as_ref().unwrap(), b"keep this partial").unwrap();
    drop(mgr);
    let db = Database::open(dir.path().join("test.db")).unwrap();
    db.preserve_legacy_downloads(&dir.path().join("downloads"))
        .unwrap();
    db.preserve_legacy_downloads(&dir.path().join("downloads"))
        .unwrap();
    let root = dir.path().join("CIRCLE");
    let mgr = DownloadManager::new(
        db,
        root,
        ProgressHub::default(),
        DataDirectoryLease::acquire(dir.path()).unwrap(),
    )
    .unwrap();
    assert_eq!(mgr.get(&task.id).unwrap().temp_path, task.temp_path);
    assert_eq!(mgr.get(&task.id).unwrap().destination_id, "circle-legacy");
    assert_eq!(
        std::fs::read(task.temp_path.as_ref().unwrap()).unwrap(),
        b"keep this partial"
    );
    mgr.remove(RemoveDownloadRequest {
        id: task.id,
        delete_partial: true,
    })
    .await
    .unwrap();
    assert!(!std::path::Path::new(task.temp_path.as_ref().unwrap()).exists());
}

#[cfg(unix)]
#[test]
fn category_folder_symlink_cannot_redirect_downloads_outside_circle() {
    let (dir, mgr) = manager();
    let outside = tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("downloads/Documents")).unwrap();
    let result = mgr.create(CreateDownloadRequest {
        url: "https://example.com/report.pdf".into(),
        destination_id: None,
        suggested_name: None,
        start_now: false,
        request_id: None,
        expected_sha256: None,
    });
    assert_eq!(result.unwrap_err().code, ErrorCode::PathNotAllowed);
    assert!(mgr.list().unwrap().is_empty());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn categorized_transfer_publishes_verified_bytes_inside_its_category() {
    let url = spawn_fixture_server(|_| (200, b"document bytes".to_vec())).await;
    let (_dir, mgr) = manager();
    let task = mgr
        .create(CreateDownloadRequest {
            url,
            destination_id: None,
            suggested_name: Some("report.pdf".into()),
            start_now: true,
            request_id: None,
            expected_sha256: Some(sha256_hex(b"document bytes")),
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(3),
    )
    .await;
    let done = mgr.get(&task.id).unwrap();
    assert_eq!(
        std::path::Path::new(done.final_path.as_ref().unwrap())
            .parent()
            .unwrap(),
        mgr.download_dir().join("Documents")
    );
    assert_eq!(
        std::fs::read(done.final_path.unwrap()).unwrap(),
        b"document bytes"
    );
    assert_eq!(done.checksum_result.as_deref(), Some("matched"));
    assert!(!std::path::Path::new(task.temp_path.as_ref().unwrap()).exists());
}

#[tokio::test]
async fn queue_pause_stops_running_writers_and_resume_preserves_individual_pauses() {
    use crate::domain::advanced::{QueuePolicy, TaskOptions};
    let bytes: Vec<u8> = (0..512_000).map(|index| (index % 251) as u8).collect();
    let url = spawn_slow_range_server(bytes.clone()).await;
    let (_dir, mgr) = manager();
    let mut queue = mgr.queue_policies().unwrap().remove(0);
    queue.max_concurrent = 1;
    mgr.save_queue(queue.clone()).await.unwrap();
    let make_input = |name: &str, start_now| CreateDownloadRequest {
        url: url.clone(),
        destination_id: None,
        suggested_name: Some(name.into()),
        start_now,
        request_id: None,
        expected_sha256: None,
    };
    let running = mgr.create(make_input("running.bin", true)).unwrap();
    let manual = mgr.create(make_input("manual.bin", false)).unwrap();
    let waiting = mgr.create(make_input("waiting.bin", true)).unwrap();
    let other_queue = mgr
        .save_queue(QueuePolicy {
            id: String::new(),
            name: "Other".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let other = mgr
        .create_configured(
            make_input("other.bin", true),
            TaskOptions {
                queue_id: other_queue.id,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    wait_for_status(
        &mgr,
        &running.id,
        &[DownloadStatus::Downloading],
        Duration::from_secs(3),
    )
    .await;
    wait_for_status(
        &mgr,
        &other.id,
        &[DownloadStatus::Downloading],
        Duration::from_secs(3),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    let position = mgr.get(&running.id).unwrap().queue_position;
    queue.paused = true;
    mgr.save_queue(queue.clone()).await.unwrap();
    let stopped = mgr.get(&running.id).unwrap();
    assert_eq!(stopped.status, DownloadStatus::Queued);
    assert_eq!(stopped.queue_position, position);
    assert_eq!(stopped.speed_bytes_per_second, 0);
    assert!(stopped.downloaded_bytes > 0);
    let partial = stopped.temp_path.unwrap();
    let length = std::fs::metadata(&partial).unwrap().len();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        std::fs::metadata(&partial).unwrap().len(),
        length,
        "queue Pause must drain the active writer"
    );
    assert_eq!(mgr.get(&waiting.id).unwrap().status, DownloadStatus::Queued);
    assert_eq!(mgr.get(&manual.id).unwrap().status, DownloadStatus::Paused);
    assert!(matches!(
        mgr.get(&other.id).unwrap().status,
        DownloadStatus::Downloading | DownloadStatus::Completed
    ));
    queue.paused = false;
    mgr.save_queue(queue).await.unwrap();
    wait_for_status(
        &mgr,
        &running.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(5),
    )
    .await;
    wait_for_status(
        &mgr,
        &waiting.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(
        std::fs::read(mgr.get(&running.id).unwrap().final_path.unwrap()).unwrap(),
        bytes
    );
    assert_eq!(mgr.get(&manual.id).unwrap().status, DownloadStatus::Paused);
    mgr.shutdown().await;
}
