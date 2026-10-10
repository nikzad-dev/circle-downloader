use crate::build_state;
use crate::domain::{CreateDownloadRequest, DownloadStatus, ErrorCode};
use crate::infrastructure::db::Database;
use crate::infrastructure::ownership::DataDirectoryLease;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct OwnerProcess(Child);
impl Drop for OwnerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_owner(path: &Path) -> OwnerProcess {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ownership_tests::child_process_owner",
            "--ignored",
            "--nocapture",
        ])
        .env("CIRCLE_LEASE_TEST_DIR", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut owner = OwnerProcess(child);
    let started = Instant::now();
    while !path.join("ready").exists() {
        assert!(
            owner.0.try_wait().unwrap().is_none(),
            "owner exited before acquiring lease"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "owner startup timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    owner
}

#[test]
#[ignore = "subprocess fixture, invoked explicitly by parent tests"]
fn child_process_owner() {
    let path =
        std::env::var_os("CIRCLE_LEASE_TEST_DIR").expect("parent must provide test directory");
    let path = std::path::PathBuf::from(path);
    let state = build_state(path.clone()).unwrap();
    let manager = state.manager.clone();
    drop(state); // The lease must follow the manager, including its clones/workers.
    let mut task = manager
        .create(CreateDownloadRequest {
            url: "https://example.com/lease-fixture.bin".into(),
            destination_id: None,
            suggested_name: None,
            start_now: false,
            request_id: None,
            expected_sha256: None,
        })
        .unwrap();
    task.status = DownloadStatus::Downloading;
    Database::open(path.join("circle-downloader.db"))
        .unwrap()
        .upsert_download(&task)
        .unwrap();
    std::fs::write(path.join("ready"), b"ready").unwrap();
    let mut byte = [0];
    std::io::stdin().read_exact(&mut byte).unwrap();
    drop(manager);
}

#[test]
fn second_process_cannot_open_or_reconcile_owned_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut owner = start_owner(dir.path());
    let error = build_state(dir.path().to_path_buf()).err().unwrap();
    assert_eq!(error.code, ErrorCode::DataDirectoryInUse);
    assert!(!error.retryable);
    let task = Database::open(dir.path().join("circle-downloader.db"))
        .unwrap()
        .list_downloads()
        .unwrap()
        .remove(0);
    assert_eq!(
        task.status,
        DownloadStatus::Downloading,
        "blocked startup must not reconcile a live owner's task"
    );
    let independent = tempfile::tempdir().unwrap();
    let _independent_state = build_state(independent.path().to_path_buf()).unwrap();
    owner.0.stdin.as_mut().unwrap().write_all(b"q").unwrap();
    assert!(owner.0.wait().unwrap().success());
    assert!(dir.path().join(".circle-downloader.lock").exists());
    let state = build_state(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        state.manager.get(&task.id).unwrap().status,
        DownloadStatus::Failed
    );
}

#[test]
fn killed_owner_releases_lease_without_deleting_lock_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut owner = start_owner(dir.path());
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    assert!(dir.path().join(".circle-downloader.lock").exists());
    let _state = build_state(dir.path().to_path_buf()).unwrap();
}

#[test]
fn failed_initialization_releases_the_lease() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("downloads"), b"not a directory").unwrap();
    assert!(build_state(dir.path().to_path_buf()).is_err());
    let _lease = DataDirectoryLease::acquire(dir.path()).unwrap();
}

#[cfg(unix)]
#[test]
fn directory_aliases_share_the_same_lease() {
    let dir = tempfile::tempdir().unwrap();
    let aliases = tempfile::tempdir().unwrap();
    let alias = aliases.path().join("alias");
    std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
    let _state = build_state(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        build_state(alias).err().unwrap().code,
        ErrorCode::DataDirectoryInUse
    );
}
