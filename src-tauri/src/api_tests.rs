use crate::application::DownloadManager;
use crate::domain::{
    CreateDownloadRequest, DownloadStatus, ErrorCode, RemoveDownloadRequest, UpdateSettingsRequest,
};
use crate::infrastructure::db::Database;
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
    let mgr = DownloadManager::new(db, downloads, ProgressHub::default()).unwrap();
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
        })
        .unwrap();
    let b = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: Some("idem-1".into()),
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
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;
    let canceled = mgr.cancel(&task.id).unwrap();
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
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &task.id,
        &[DownloadStatus::Completed],
        Duration::from_secs(10),
    )
    .await;
    let err = mgr.cancel(&task.id).unwrap_err();
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
        })
        .unwrap();

    let paused = mgr.pause(&task.id).unwrap();
    assert_eq!(paused.status, DownloadStatus::Paused);

    let _ = mgr.remove(RemoveDownloadRequest {
        id: task.id,
        delete_partial: true,
    });

    let bad = mgr
        .create(CreateDownloadRequest {
            url: "http://127.0.0.1:1/nope.bin".into(),
            destination_id: None,
            suggested_name: Some("nope.bin".into()),
            start_now: true,
            request_id: None,
        })
        .unwrap();
    wait_for_status(
        &mgr,
        &bad.id,
        &[DownloadStatus::Failed],
        Duration::from_secs(10),
    )
    .await;

    let illegal = mgr.pause(&bad.id).unwrap_err();
    assert_eq!(illegal.code, ErrorCode::InvalidTransition);

    let retried = mgr.retry(&bad.id).unwrap();
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
        })
        .unwrap();

    let temp = task.temp_path.clone().unwrap();
    std::fs::write(&temp, b"PART").unwrap();
    let _ = mgr.pause(&task.id).unwrap();
    let _ = mgr.resume(&task.id).unwrap();
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
    let mgr = DownloadManager::new(db, downloads.clone(), ProgressHub::default()).unwrap();
    let task = mgr
        .create(CreateDownloadRequest {
            url: "https://example.com/a.bin".into(),
            destination_id: None,
            suggested_name: Some("a.bin".into()),
            start_now: false,
            request_id: None,
        })
        .unwrap();
    let mut active = mgr.get(&task.id).unwrap();
    active.status = DownloadStatus::Downloading;
    Database::open(&db_path)
        .unwrap()
        .upsert_download(&active)
        .unwrap();

    let mgr2 = DownloadManager::new(
        Database::open(&db_path).unwrap(),
        downloads,
        ProgressHub::default(),
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
