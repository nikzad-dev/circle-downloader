use crate::domain::{
    AppError, DownloadSegment, DownloadStatus, DownloadTask, ErrorCode, Queue, SegmentStatus,
    Settings, TaskError, TransferMode, UpdateSettingsRequest,
};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SCHEMA_VERSION: i64 = 5;
const BUSY_TIMEOUT_MS: i32 = 5000;

type MigrationStep = fn(&rusqlite::Transaction<'_>) -> Result<(), AppError>;

const MIGRATIONS: &[(i64, MigrationStep)] = &[
    (2, migrate_v2_checksums),
    (3, migrate_v3_queue),
    (4, migrate_v4_destinations),
    (5, migrate_v5_segmentation),
];

const DOWNLOAD_SELECT: &str =
    "SELECT id, url, file_name, destination_id, temp_path, final_path, status,
                        downloaded_bytes, total_bytes, speed_bytes_per_second, revision,
                        error_code, error_message, error_retryable, etag, last_modified,
                        request_id, retry_count, created_at, updated_at, expected_sha256,
                        actual_sha256, checksum_result, finalization, queue_id, queue_position,
                        host, next_retry_at, transfer_mode
                 FROM downloads";

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::infrastructure::errors::map_io_err)?;
        }
        // macOS /var and legitimate user data roots may alias their canonical parent;
        // SQLite NOFOLLOW then rejects substitutions of the database file itself.
        let path = match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => std::fs::canonicalize(parent)
                .map_err(crate::infrastructure::errors::map_io_err)?
                .join(name),
            _ => path,
        };
        let conn = Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(map_db_err)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            path,
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory().map_err(map_db_err)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<(), AppError> {
        {
            let conn = self.conn.lock();
            conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64))
                .map_err(map_db_err)?;
            conn.pragma_update(None, "journal_mode", "WAL")
                .map_err(map_db_err)?;
            conn.pragma_update(None, "synchronous", "FULL")
                .map_err(map_db_err)?;
            conn.execute_batch(BOOTSTRAP_SQL).map_err(map_db_err)?;
        }

        let version = self.schema_version()?;
        if version > SCHEMA_VERSION {
            return Err(AppError::new(
                ErrorCode::StorageError,
                format!(
                    "This database is schema version {version}, newer than this app supports ({SCHEMA_VERSION}). Use a newer Circle Downloader or restore circle-downloader.db.bak."
                ),
            ));
        }

        let pending: Vec<(i64, MigrationStep)> = MIGRATIONS
            .iter()
            .copied()
            .filter(|(v, _)| version < *v)
            .collect();
        if pending.is_empty() {
            return Ok(());
        }

        if !self.is_memory() {
            backup_database_file(&self.path)?;
        }

        for (target, step) in pending {
            let result = {
                let mut conn = self.conn.lock();
                let tx = conn.transaction().map_err(map_db_err)?;
                match step(&tx) {
                    Ok(()) => {
                        tx.execute(
                            "INSERT INTO schema_migrations(version) VALUES(?1)",
                            params![target],
                        )
                        .map_err(map_db_err)?;
                        tx.commit().map_err(map_db_err)?;
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            };
            if let Err(error) = result {
                if !self.is_memory() {
                    let _ = restore_database_file(&self.path);
                }
                return Err(AppError::new(
                    ErrorCode::StorageError,
                    format!(
                        "Database upgrade to version {target} failed. Restore circle-downloader.db.bak if the app will not open. ({})",
                        error.message
                    ),
                ));
            }
        }
        Ok(())
    }

    fn is_memory(&self) -> bool {
        self.path == Path::new(":memory:")
    }

    fn schema_version(&self) -> Result<i64, AppError> {
        let conn = self.conn.lock();
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .map_err(map_db_err)
    }

    pub fn next_queue_position(&self, queue_id: &str) -> Result<i64, AppError> {
        let conn = self.conn.lock();
        let max: Option<i64> = conn
            .query_row(
                "SELECT MAX(queue_position) FROM downloads WHERE queue_id = ?1",
                params![queue_id],
                |row| row.get(0),
            )
            .map_err(map_db_err)?;
        Ok(max.unwrap_or(0).saturating_add(1))
    }

    pub fn list_runnable_queued(&self, now: DateTime<Utc>) -> Result<Vec<DownloadTask>, AppError> {
        let conn = self.conn.lock();
        let sql = format!(
            "{DOWNLOAD_SELECT}
                 WHERE status = 'queued'
                   AND (next_retry_at IS NULL OR next_retry_at <= ?1)
                 ORDER BY queue_position ASC NULLS LAST, created_at ASC"
        );
        let mut stmt = conn.prepare(&sql).map_err(map_db_err)?;
        let rows = stmt
            .query_map(params![now.to_rfc3339()], map_task_row)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    pub fn earliest_retry_at(&self) -> Result<Option<DateTime<Utc>>, AppError> {
        let conn = self.conn.lock();
        let value: Option<String> = conn
            .query_row(
                "SELECT MIN(next_retry_at) FROM downloads
                 WHERE status = 'queued' AND next_retry_at IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_err)?
            .flatten();
        value.map(|v| parse_dt(&v).map_err(map_db_err)).transpose()
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadTask>, AppError> {
        let conn = self.conn.lock();
        let sql = format!("{DOWNLOAD_SELECT} ORDER BY created_at ASC");
        let mut stmt = conn.prepare(&sql).map_err(map_db_err)?;
        let rows = stmt
            .query_map([], map_task_row)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    pub fn get_download(&self, id: &str) -> Result<DownloadTask, AppError> {
        let conn = self.conn.lock();
        let sql = format!("{DOWNLOAD_SELECT} WHERE id = ?1");
        conn.query_row(&sql, params![id], map_task_row)
            .optional()
            .map_err(map_db_err)?
            .ok_or_else(|| {
                AppError::new(ErrorCode::TaskNotFound, format!("Download not found: {id}"))
            })
    }

    pub fn find_by_request_id(&self, request_id: &str) -> Result<Option<DownloadTask>, AppError> {
        let conn = self.conn.lock();
        let sql = format!("{DOWNLOAD_SELECT} WHERE request_id = ?1");
        conn.query_row(&sql, params![request_id], map_task_row)
            .optional()
            .map_err(map_db_err)
    }

    pub fn upsert_download(&self, task: &DownloadTask) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let (error_code, error_message, error_retryable) = match &task.error {
            Some(err) => (
                Some(err.code.as_str().to_string()),
                Some(err.message.clone()),
                Some(i64::from(err.retryable)),
            ),
            None => (None, None, None),
        };
        conn.execute(
            "INSERT INTO downloads (
                id, url, file_name, destination_id, temp_path, final_path, status,
                downloaded_bytes, total_bytes, speed_bytes_per_second, revision,
                error_code, error_message, error_retryable, etag, last_modified,
                request_id, retry_count, created_at, updated_at, expected_sha256,
                actual_sha256, checksum_result, finalization, queue_id, queue_position,
                host, next_retry_at, transfer_mode
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29)
             ON CONFLICT(id) DO UPDATE SET
                url=excluded.url,
                file_name=excluded.file_name,
                destination_id=excluded.destination_id,
                temp_path=excluded.temp_path,
                final_path=excluded.final_path,
                status=excluded.status,
                downloaded_bytes=excluded.downloaded_bytes,
                total_bytes=excluded.total_bytes,
                speed_bytes_per_second=excluded.speed_bytes_per_second,
                revision=excluded.revision,
                error_code=excluded.error_code,
                error_message=excluded.error_message,
                error_retryable=excluded.error_retryable,
                etag=excluded.etag,
                last_modified=excluded.last_modified,
                request_id=excluded.request_id,
                retry_count=excluded.retry_count,
                updated_at=excluded.updated_at,
                expected_sha256=excluded.expected_sha256,
                actual_sha256=excluded.actual_sha256,
                checksum_result=excluded.checksum_result,
                finalization=excluded.finalization,
                queue_id=excluded.queue_id,
                queue_position=excluded.queue_position,
                host=excluded.host,
                next_retry_at=excluded.next_retry_at,
                transfer_mode=excluded.transfer_mode",
            params![
                task.id,
                task.url,
                task.file_name,
                task.destination_id,
                task.temp_path,
                task.final_path,
                task.status.as_str(),
                task.downloaded_bytes as i64,
                task.total_bytes.map(|v| v as i64),
                task.speed_bytes_per_second as i64,
                task.revision as i64,
                error_code,
                error_message,
                error_retryable,
                task.etag,
                task.last_modified,
                task.request_id,
                task.retry_count as i64,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
                task.expected_sha256,
                task.actual_sha256,
                task.checksum_result,
                task.finalization,
                task.queue_id,
                task.queue_position,
                task.host,
                task.next_retry_at.map(|dt| dt.to_rfc3339()),
                task.transfer_mode.as_str(),
            ],
        )
        .map_err(map_db_err)?;
        Ok(())
    }

    pub fn remove_download(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM download_segments WHERE task_id = ?1",
            params![id],
        )
        .map_err(map_db_err)?;
        let changed = conn
            .execute("DELETE FROM downloads WHERE id = ?1", params![id])
            .map_err(map_db_err)?;
        if changed == 0 {
            return Err(AppError::new(
                ErrorCode::TaskNotFound,
                format!("Download not found: {id}"),
            ));
        }
        Ok(())
    }

    pub fn replace_segments(&self, segments: &[DownloadSegment]) -> Result<(), AppError> {
        let first = segments
            .first()
            .ok_or_else(|| AppError::new(ErrorCode::Internal, "Empty segment plan"))?;
        if segments.len() > 4
            || segments.iter().any(|s| {
                s.task_id != first.task_id
                    || s.end_byte < s.start_byte
                    || s.end_byte > i64::MAX as u64
                    || s.completed_bytes > s.expected_len()
            })
        {
            return Err(AppError::new(ErrorCode::Internal, "Invalid segment plan"));
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(map_db_err)?;
        if let Some(first) = segments.first() {
            tx.execute(
                "DELETE FROM download_segments WHERE task_id = ?1",
                params![first.task_id],
            )
            .map_err(map_db_err)?;
        }
        for segment in segments {
            tx.execute(
                "INSERT INTO download_segments (
                    task_id, segment_index, start_byte, end_byte, completed_bytes, status, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    segment.task_id,
                    segment.segment_index as i64,
                    segment.start_byte as i64,
                    segment.end_byte as i64,
                    segment.completed_bytes as i64,
                    segment.status.as_str(),
                    segment.updated_at.to_rfc3339(),
                ],
            )
            .map_err(map_db_err)?;
        }
        let changed = tx
            .execute(
                "UPDATE downloads SET transfer_mode='segmented' WHERE id=?1",
                params![first.task_id],
            )
            .map_err(map_db_err)?;
        if changed != 1 {
            return Err(AppError::new(ErrorCode::TaskNotFound, "Download not found"));
        }
        tx.commit().map_err(map_db_err)
    }

    pub fn clear_segments(&self, task_id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM download_segments WHERE task_id = ?1",
            params![task_id],
        )
        .map_err(map_db_err)?;
        Ok(())
    }

    pub fn list_segments(&self, task_id: &str) -> Result<Vec<DownloadSegment>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT task_id, segment_index, start_byte, end_byte, completed_bytes, status, updated_at
                 FROM download_segments
                 WHERE task_id = ?1
                 ORDER BY segment_index ASC",
            )
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map(params![task_id], map_segment_row)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    pub fn update_segment_checkpoint(
        &self,
        task_id: &str,
        segment_index: u32,
        completed_bytes: u64,
        status: SegmentStatus,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let changed = conn
            .execute(
                "UPDATE download_segments
                 SET completed_bytes = ?1, status = ?2, updated_at = ?3
                 WHERE task_id = ?4 AND segment_index = ?5",
                params![
                    completed_bytes as i64,
                    status.as_str(),
                    Utc::now().to_rfc3339(),
                    task_id,
                    segment_index as i64,
                ],
            )
            .map_err(map_db_err)?;
        if changed == 0 {
            return Err(AppError::new(
                ErrorCode::TaskNotFound,
                format!("Segment not found: {task_id}/{segment_index}"),
            ));
        }
        Ok(())
    }

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT max_concurrent_downloads, default_destination_id, auto_start, max_connections_per_download
             FROM settings WHERE id = 1",
            [],
            |row| {
                Ok(Settings {
                    max_concurrent_downloads: row.get::<_, i64>(0)? as u32,
                    default_destination_id: row.get(1)?,
                    auto_start: row.get::<_, i64>(2)? != 0,
                    max_connections_per_download: row.get::<_, i64>(3)? as u32,
                })
            },
        )
        .map_err(map_db_err)
    }

    pub fn update_settings(&self, patch: UpdateSettingsRequest) -> Result<Settings, AppError> {
        let mut current = self.get_settings()?;
        if let Some(v) = patch.max_concurrent_downloads {
            if !(1..=32).contains(&v) {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    "maxConcurrentDownloads must be between 1 and 32",
                ));
            }
            current.max_concurrent_downloads = v;
        }
        if let Some(v) = patch.default_destination_id {
            if v.trim().is_empty() {
                return Err(AppError::new(
                    ErrorCode::PathNotAllowed,
                    "defaultDestinationId cannot be empty",
                ));
            }
            current.default_destination_id = v;
        }
        if let Some(v) = patch.auto_start {
            current.auto_start = v;
        }
        if let Some(v) = patch.max_connections_per_download {
            if !(1..=4).contains(&v) {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    "maxConnectionsPerDownload must be between 1 and 4",
                ));
            }
            current.max_connections_per_download = v;
        }
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE settings SET max_concurrent_downloads=?1, default_destination_id=?2, auto_start=?3,
             max_connections_per_download=?4 WHERE id=1",
            params![
                current.max_concurrent_downloads as i64,
                current.default_destination_id,
                i64::from(current.auto_start),
                current.max_connections_per_download as i64,
            ],
        )
        .map_err(map_db_err)?;
        Ok(current)
    }

    pub fn list_destinations(&self) -> Result<Vec<(String, String, String)>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT id, label, root_path FROM destinations ORDER BY created_at ASC")
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    pub fn insert_destination(
        &self,
        id: &str,
        label: &str,
        root_path: &str,
        created_at: &str,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO destinations (id, label, root_path, created_at) VALUES (?1,?2,?3,?4)",
            params![id, label, root_path, created_at],
        )
        .map_err(map_db_err)?;
        Ok(())
    }

    pub fn get_destination_path(&self, id: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT root_path FROM destinations WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_db_err)
    }

    pub fn remove_destination(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let in_use: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM downloads WHERE destination_id = ?1
                 AND status NOT IN ('completed','canceled')",
                params![id],
                |row| row.get(0),
            )
            .map_err(map_db_err)?;
        if in_use > 0 {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "This folder is still used by active downloads. Finish or remove them first.",
            ));
        }
        let settings = self.get_settings()?;
        if settings.default_destination_id == id {
            return Err(AppError::new(
                ErrorCode::PathNotAllowed,
                "Choose another default destination before removing this folder.",
            ));
        }
        let changed = conn
            .execute("DELETE FROM destinations WHERE id = ?1", params![id])
            .map_err(map_db_err)?;
        if changed == 0 {
            return Err(AppError::new(
                ErrorCode::TaskNotFound,
                format!("Destination not found: {id}"),
            ));
        }
        Ok(())
    }

    pub fn destination_exists(&self, id: &str) -> Result<bool, AppError> {
        if id == "default" {
            return Ok(true);
        }
        Ok(self.get_destination_path(id)?.is_some())
    }

    pub fn list_queues(&self) -> Result<Vec<Queue>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT q.id, q.name,
                        (SELECT COUNT(*) FROM downloads d
                         WHERE d.queue_id = q.id
                           AND d.status NOT IN ('completed','canceled')) as task_count
                 FROM queues q ORDER BY q.id",
            )
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Queue {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    task_count: row.get::<_, i64>(2)? as u32,
                })
            })
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }
}

const BOOTSTRAP_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY
);
CREATE TABLE IF NOT EXISTS downloads (
  id TEXT PRIMARY KEY,
  url TEXT NOT NULL,
  file_name TEXT NOT NULL,
  destination_id TEXT NOT NULL,
  temp_path TEXT,
  final_path TEXT,
  status TEXT NOT NULL,
  downloaded_bytes INTEGER NOT NULL DEFAULT 0,
  total_bytes INTEGER,
  speed_bytes_per_second INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 0,
  error_code TEXT,
  error_message TEXT,
  error_retryable INTEGER,
  etag TEXT,
  last_modified TEXT,
  request_id TEXT,
  retry_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_downloads_request_id
  ON downloads(request_id) WHERE request_id IS NOT NULL;
CREATE TABLE IF NOT EXISTS settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  max_concurrent_downloads INTEGER NOT NULL,
  default_destination_id TEXT NOT NULL,
  auto_start INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS queues (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL
);
INSERT OR IGNORE INTO settings (id, max_concurrent_downloads, default_destination_id, auto_start)
  VALUES (1, 3, 'default', 1);
INSERT OR IGNORE INTO queues (id, name) VALUES ('default', 'Default');
INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);
"#;

fn migrate_v2_checksums(tx: &rusqlite::Transaction<'_>) -> Result<(), AppError> {
    tx.execute_batch(
        "ALTER TABLE downloads ADD COLUMN expected_sha256 TEXT;
         ALTER TABLE downloads ADD COLUMN actual_sha256 TEXT;
         ALTER TABLE downloads ADD COLUMN checksum_result TEXT;
         ALTER TABLE downloads ADD COLUMN finalization TEXT;",
    )
    .map_err(map_db_err)
}

fn migrate_v3_queue(tx: &rusqlite::Transaction<'_>) -> Result<(), AppError> {
    tx.execute_batch(
        "ALTER TABLE downloads ADD COLUMN queue_id TEXT NOT NULL DEFAULT 'default';
         ALTER TABLE downloads ADD COLUMN queue_position INTEGER;
         ALTER TABLE downloads ADD COLUMN host TEXT NOT NULL DEFAULT 'unknown';
         ALTER TABLE downloads ADD COLUMN next_retry_at TEXT;
         CREATE INDEX IF NOT EXISTS idx_downloads_queue_order
           ON downloads(queue_id, queue_position);",
    )
    .map_err(map_db_err)
}

fn migrate_v4_destinations(tx: &rusqlite::Transaction<'_>) -> Result<(), AppError> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS destinations (
           id TEXT PRIMARY KEY,
           label TEXT NOT NULL,
           root_path TEXT NOT NULL UNIQUE,
           created_at TEXT NOT NULL
         );",
    )
    .map_err(map_db_err)
}

fn migrate_v5_segmentation(tx: &rusqlite::Transaction<'_>) -> Result<(), AppError> {
    tx.execute_batch(
        "ALTER TABLE downloads ADD COLUMN transfer_mode TEXT NOT NULL DEFAULT 'single';
         ALTER TABLE settings ADD COLUMN max_connections_per_download INTEGER NOT NULL DEFAULT 1 CHECK(max_connections_per_download BETWEEN 1 AND 4);
         CREATE TABLE IF NOT EXISTS download_segments (
           task_id TEXT NOT NULL REFERENCES downloads(id) ON DELETE CASCADE,
           segment_index INTEGER NOT NULL CHECK(segment_index BETWEEN 0 AND 3),
           start_byte INTEGER NOT NULL CHECK(start_byte >= 0),
           end_byte INTEGER NOT NULL CHECK(end_byte >= start_byte),
           completed_bytes INTEGER NOT NULL DEFAULT 0 CHECK(completed_bytes >= 0 AND completed_bytes <= end_byte - start_byte + 1),
           status TEXT NOT NULL CHECK(status IN ('pending','active','complete','failed')),
           updated_at TEXT NOT NULL,
           PRIMARY KEY (task_id, segment_index)
         );
         CREATE INDEX IF NOT EXISTS idx_download_segments_task
           ON download_segments(task_id);",
    )
    .map_err(map_db_err)
}

pub fn backup_path(db_path: &Path) -> PathBuf {
    let mut bak = db_path.as_os_str().to_owned();
    bak.push(".bak");
    PathBuf::from(bak)
}

fn backup_database_file(db_path: &Path) -> Result<(), AppError> {
    if !db_path.exists() {
        return Ok(());
    }
    let bak = backup_path(db_path);
    std::fs::copy(db_path, &bak).map_err(crate::infrastructure::errors::map_io_err)?;
    Ok(())
}

fn restore_database_file(db_path: &Path) -> Result<(), AppError> {
    let bak = backup_path(db_path);
    if !bak.exists() {
        return Err(AppError::new(
            ErrorCode::StorageNotFound,
            "No database backup is available to restore.",
        ));
    }
    std::fs::copy(&bak, db_path).map_err(crate::infrastructure::errors::map_io_err)?;
    Ok(())
}

pub fn map_db_err(err: rusqlite::Error) -> AppError {
    let code = match &err {
        rusqlite::Error::SqliteFailure(error, _) => match error.code {
            rusqlite::ErrorCode::DiskFull => ErrorCode::DiskFull,
            rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly => {
                ErrorCode::StoragePermission
            }
            rusqlite::ErrorCode::ConstraintViolation => ErrorCode::AlreadyExists,
            _ => ErrorCode::StorageError,
        },
        _ => ErrorCode::StorageError,
    };
    AppError::new(
        code,
        "The download database could not be read or updated. Check disk space and data-folder permissions, then reopen the app.",
    )
}

pub fn is_unique_constraint(err: &AppError) -> bool {
    err.code == ErrorCode::AlreadyExists
        && err.message.contains("download database could not be read")
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadTask> {
    let status_raw: String = row.get(6)?;
    let status = DownloadStatus::parse(&status_raw).unwrap_or(DownloadStatus::Failed);
    let error_code: Option<String> = row.get(11)?;
    let error = match error_code {
        Some(code) => Some(TaskError {
            code: parse_error_code(&code),
            message: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
            retryable: row.get::<_, Option<i64>>(13)?.unwrap_or(0) != 0,
        }),
        None => None,
    };
    let created_at = parse_dt(&row.get::<_, String>(18)?)?;
    let updated_at = parse_dt(&row.get::<_, String>(19)?)?;
    let next_retry_at = row
        .get::<_, Option<String>>(27)?
        .map(|v| parse_dt(&v))
        .transpose()?;
    let transfer_mode = row
        .get::<_, Option<String>>(28)?
        .as_deref()
        .map(TransferMode::parse)
        .unwrap_or_default();
    Ok(DownloadTask {
        id: row.get(0)?,
        url: row.get(1)?,
        file_name: row.get(2)?,
        destination_id: row.get(3)?,
        temp_path: row.get(4)?,
        final_path: row.get(5)?,
        status,
        downloaded_bytes: row.get::<_, i64>(7)? as u64,
        total_bytes: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        speed_bytes_per_second: row.get::<_, i64>(9)? as u64,
        revision: row.get::<_, i64>(10)? as u64,
        error,
        etag: row.get(14)?,
        last_modified: row.get(15)?,
        request_id: row.get(16)?,
        retry_count: row.get::<_, i64>(17)? as u32,
        expected_sha256: row.get(20)?,
        actual_sha256: row.get(21)?,
        checksum_result: row.get(22)?,
        finalization: row.get(23)?,
        queue_id: row
            .get::<_, Option<String>>(24)?
            .unwrap_or_else(|| "default".into()),
        queue_position: row.get(25)?,
        host: row
            .get::<_, Option<String>>(26)?
            .unwrap_or_else(|| "unknown".into()),
        next_retry_at,
        transfer_mode,
        created_at,
        updated_at,
    })
}

fn map_segment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadSegment> {
    Ok(DownloadSegment {
        task_id: row.get(0)?,
        segment_index: row.get::<_, i64>(1)? as u32,
        start_byte: row.get::<_, i64>(2)? as u64,
        end_byte: row.get::<_, i64>(3)? as u64,
        completed_bytes: row.get::<_, i64>(4)? as u64,
        status: SegmentStatus::parse(&row.get::<_, String>(5)?),
        updated_at: parse_dt(&row.get::<_, String>(6)?)?,
    })
}

fn parse_dt(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_error_code(code: &str) -> ErrorCode {
    serde_json::from_value(serde_json::Value::String(code.into())).unwrap_or(ErrorCode::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    pub(super) fn sample_task(id: &str) -> DownloadTask {
        let now = Utc::now();
        DownloadTask {
            id: id.into(),
            url: "https://example.com/a".into(),
            file_name: "a.bin".into(),
            destination_id: "default".into(),
            status: DownloadStatus::Queued,
            downloaded_bytes: 0,
            total_bytes: None,
            speed_bytes_per_second: 0,
            revision: 1,
            error: None,
            created_at: now,
            updated_at: now,
            temp_path: None,
            final_path: None,
            etag: None,
            last_modified: None,
            request_id: Some("req-1".into()),
            retry_count: 0,
            expected_sha256: None,
            actual_sha256: None,
            checksum_result: None,
            finalization: None,
            next_retry_at: None,
            queue_id: "default".into(),
            queue_position: Some(1),
            host: "example.com".into(),
            transfer_mode: TransferMode::Single,
        }
    }

    #[test]
    fn persists_settings_and_downloads() {
        let db = Database::open_in_memory().unwrap();
        let settings = db
            .update_settings(UpdateSettingsRequest {
                max_concurrent_downloads: Some(5),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(settings.max_concurrent_downloads, 5);

        let task = sample_task("t1");
        db.upsert_download(&task).unwrap();
        assert_eq!(db.list_downloads().unwrap().len(), 1);
        assert_eq!(db.get_download("t1").unwrap().file_name, "a.bin");
        assert_eq!(db.get_download("t1").unwrap().host, "example.com");
        assert_eq!(db.list_queues().unwrap()[0].task_count, 1);
    }
}

#[cfg(test)]
mod error_contract_tests {
    use super::*;
    #[test]
    fn all_native_error_codes_survive_database_decoding() {
        for code in ErrorCode::ALL {
            assert_eq!(parse_error_code(code.as_str()), *code);
        }
        assert_eq!(parse_error_code("FUTURE_UNKNOWN"), ErrorCode::Internal);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn existing_v1_rows_survive_ordered_upgrades_through_v3_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let db = Database::open(&path).unwrap();
        // Restore precisely the v1 column/version shape, then open via the real migration.
        db.conn.lock().execute_batch(
            "DROP INDEX IF EXISTS idx_downloads_queue_order;
            DROP INDEX IF EXISTS idx_download_segments_task;
            DROP TABLE IF EXISTS download_segments;
            DROP TABLE IF EXISTS destinations;
            ALTER TABLE downloads DROP COLUMN expected_sha256;
            ALTER TABLE downloads DROP COLUMN actual_sha256;
            ALTER TABLE downloads DROP COLUMN checksum_result;
            ALTER TABLE downloads DROP COLUMN finalization;
            ALTER TABLE downloads DROP COLUMN queue_id;
            ALTER TABLE downloads DROP COLUMN queue_position;
            ALTER TABLE downloads DROP COLUMN host;
            ALTER TABLE downloads DROP COLUMN next_retry_at;
            ALTER TABLE downloads DROP COLUMN transfer_mode;
            ALTER TABLE settings DROP COLUMN max_connections_per_download;
            DELETE FROM schema_migrations WHERE version>=2;
            INSERT INTO downloads(id,url,file_name,destination_id,status,created_at,updated_at)
            VALUES('legacy','https://example.com/file','file','default','paused','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z');",
        ).unwrap();
        drop(db);
        for _ in 0..2 {
            let db = Database::open(&path).unwrap();
            assert_eq!(db.schema_version().unwrap(), 5);
            let task = db.get_download("legacy").unwrap();
            assert_eq!(task.file_name, "file");
            assert!(task.actual_sha256.is_none());
            assert_eq!(task.queue_id, "default");
            assert_eq!(task.host, "unknown");
            let sync: i64 = db
                .conn
                .lock()
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            assert_eq!(sync, 2);
            let journal: String = db
                .conn
                .lock()
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_eq!(journal.to_lowercase(), "wal");
        }
        assert!(backup_path(&path).exists());
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.db");
        {
            let db = Database::open(&path).unwrap();
            db.upsert_download(&tests::sample_task("keep")).unwrap();
        }
        backup_database_file(&path).unwrap();
        std::fs::write(&path, b"corrupt").unwrap();
        restore_database_file(&path).unwrap();
        let db = Database::open(&path).unwrap();
        assert_eq!(db.get_download("keep").unwrap().file_name, "a.bin");
    }

    #[test]
    fn refuses_newer_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.db");
        {
            let db = Database::open(&path).unwrap();
            db.conn
                .lock()
                .execute(
                    "INSERT INTO schema_migrations(version) VALUES(?1)",
                    params![SCHEMA_VERSION + 1],
                )
                .unwrap();
        }
        let Err(err) = Database::open(&path) else {
            panic!("expected schema refusal");
        };
        assert_eq!(err.code, ErrorCode::StorageError);
        assert!(err.message.contains("newer than this app supports"));
    }

    #[test]
    fn wal_busy_timeout_allows_contended_writers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("busy.db");
        let db_a = Database::open(&path).unwrap();
        let db_b = Database::open(&path).unwrap();
        db_a.upsert_download(&tests::sample_task("a")).unwrap();
        let mut task_b = tests::sample_task("b");
        task_b.request_id = Some("req-b".into());
        db_b.upsert_download(&task_b).unwrap();
        assert_eq!(db_a.list_downloads().unwrap().len(), 2);
    }
}

#[cfg(test)]
mod segmentation_tests {
    use super::*;
    use crate::application::segmentation::plan_segments;

    #[test]
    fn v4_upgrade_preserves_existing_task_and_defaults_to_single_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.db");
        let db = Database::open(&path).unwrap();
        db.upsert_download(&super::tests::sample_task("v4"))
            .unwrap();
        db.conn.lock().execute_batch("DROP TABLE download_segments; ALTER TABLE downloads DROP COLUMN transfer_mode; ALTER TABLE settings DROP COLUMN max_connections_per_download; DELETE FROM schema_migrations WHERE version=5;").unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 5);
        assert_eq!(
            db.get_download("v4").unwrap().transfer_mode,
            TransferMode::Single
        );
        assert_eq!(db.get_settings().unwrap().max_connections_per_download, 1);
        assert!(backup_path(&path).exists());
    }

    #[test]
    fn segment_plan_replacement_is_atomic_and_checkpoint_bounds_are_enforced() {
        let db = Database::open_in_memory().unwrap();
        db.upsert_download(&super::tests::sample_task("t")).unwrap();
        let plan = plan_segments("t", 16, 4);
        db.replace_segments(&plan).unwrap();
        assert_eq!(
            db.get_download("t").unwrap().transfer_mode,
            TransferMode::Segmented
        );
        let mut invalid = plan.clone();
        invalid[1].segment_index = 0;
        assert!(db.replace_segments(&invalid).is_err());
        assert_eq!(db.list_segments("t").unwrap(), plan);
        assert!(
            db.update_segment_checkpoint("t", 0, 5, SegmentStatus::Complete)
                .is_err()
        );
        assert_eq!(db.list_segments("t").unwrap()[0].completed_bytes, 0);
        db.remove_download("t").unwrap();
        assert!(db.list_segments("t").unwrap().is_empty());
    }
}
