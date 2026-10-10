use crate::domain::{
    AppError, DownloadStatus, DownloadTask, ErrorCode, ProgressEvent, Queue, RemoveDownloadResult,
    Settings, TaskError,
};
use serde_json::json;

/// Shared with the frontend Zod test: native serialization is the source of truth.
#[test]
fn serialized_ipc_matches_the_shared_frontend_contract() {
    let timestamp = "2026-10-10T00:00:00Z".parse().unwrap();
    let task = DownloadTask {
        id: "contract-task".into(),
        url: "https://example.com/contract.bin".into(),
        file_name: "contract.bin".into(),
        destination_id: "default".into(),
        status: DownloadStatus::Failed,
        downloaded_bytes: 4096,
        total_bytes: None,
        speed_bytes_per_second: 0,
        revision: 7,
        error: Some(TaskError {
            code: ErrorCode::DiskFull,
            message: "Fixture storage error".into(),
            retryable: true,
        }),
        created_at: timestamp,
        updated_at: timestamp,
        temp_path: Some("private/temp/path".into()),
        final_path: Some("private/final/path".into()),
        etag: Some("private-validator".into()),
        last_modified: None,
        request_id: Some("private-request-id".into()),
        retry_count: 3,
        expected_sha256: None,
        actual_sha256: None,
        checksum_result: None,
        finalization: None,
        next_retry_at: None,
        queue_id: "default".into(),
        queue_position: None,
        host: "example.com".into(),
    };
    let payload = json!({
        "task": task,
        "progress": ProgressEvent {
            task_id: "contract-task".into(), revision: 7,
            transferred_bytes: 4096, total_bytes: Some(8192), bytes_per_second: 2048,
            status: DownloadStatus::Downloading, timestamp,
        },
        "settings": Settings::default(),
        "queue": Queue { id: "default".into(), name: "Default".into(), task_count: 2 },
        "remove": RemoveDownloadResult { removed: true },
        "errorCodes": ErrorCode::ALL,
        "error": AppError::new(ErrorCode::DiskFull, "Fixture storage error"),
    });
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/ipc-contract.json")).unwrap();
    assert_eq!(
        payload, expected,
        "Update the fixture and Zod contract together; do not expose private native fields"
    );
}
