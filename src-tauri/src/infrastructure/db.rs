use crate::domain::{
    AppError, DownloadStatus, DownloadTask, ErrorCode, Queue, Settings, TaskError,
    UpdateSettingsRequest,
};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::new(ErrorCode::PathNotAllowed, format!("Cannot create db dir: {e}"))
            })?;
        }
        let conn = Connection::open(&path).map_err(map_db_err)?;
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
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
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
            "#,
        )
        .map_err(map_db_err)?;
        Ok(())
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadTask>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, url, file_name, destination_id, temp_path, final_path, status,
                        downloaded_bytes, total_bytes, speed_bytes_per_second, revision,
                        error_code, error_message, error_retryable, etag, last_modified,
                        request_id, retry_count, created_at, updated_at
                 FROM downloads ORDER BY created_at ASC",
            )
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map([], map_task_row)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    pub fn get_download(&self, id: &str) -> Result<DownloadTask, AppError> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, url, file_name, destination_id, temp_path, final_path, status,
                    downloaded_bytes, total_bytes, speed_bytes_per_second, revision,
                    error_code, error_message, error_retryable, etag, last_modified,
                    request_id, retry_count, created_at, updated_at
             FROM downloads WHERE id = ?1",
            params![id],
            map_task_row,
        )
        .optional()
        .map_err(map_db_err)?
        .ok_or_else(|| AppError::new(ErrorCode::TaskNotFound, format!("Download not found: {id}")))
    }

    pub fn find_by_request_id(&self, request_id: &str) -> Result<Option<DownloadTask>, AppError> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT id, url, file_name, destination_id, temp_path, final_path, status,
                    downloaded_bytes, total_bytes, speed_bytes_per_second, revision,
                    error_code, error_message, error_retryable, etag, last_modified,
                    request_id, retry_count, created_at, updated_at
             FROM downloads WHERE request_id = ?1",
            params![request_id],
            map_task_row,
        )
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
                request_id, retry_count, created_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)
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
                updated_at=excluded.updated_at",
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
            ],
        )
        .map_err(map_db_err)?;
        Ok(())
    }

    pub fn remove_download(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock();
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

    pub fn get_settings(&self) -> Result<Settings, AppError> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT max_concurrent_downloads, default_destination_id, auto_start FROM settings WHERE id = 1",
            [],
            |row| {
                Ok(Settings {
                    max_concurrent_downloads: row.get::<_, i64>(0)? as u32,
                    default_destination_id: row.get(1)?,
                    auto_start: row.get::<_, i64>(2)? != 0,
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
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE settings SET max_concurrent_downloads=?1, default_destination_id=?2, auto_start=?3 WHERE id=1",
            params![
                current.max_concurrent_downloads as i64,
                current.default_destination_id,
                i64::from(current.auto_start),
            ],
        )
        .map_err(map_db_err)?;
        Ok(current)
    }

    pub fn list_queues(&self) -> Result<Vec<Queue>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT q.id, q.name,
                        (SELECT COUNT(*) FROM downloads d WHERE d.status NOT IN ('completed','canceled')) as task_count
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

fn map_db_err(err: rusqlite::Error) -> AppError {
    AppError::new(ErrorCode::Internal, format!("Database error: {err}"))
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
        created_at,
        updated_at,
    })
}

fn parse_dt(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn parse_error_code(code: &str) -> ErrorCode {
    match code {
        "INVALID_URL" => ErrorCode::InvalidUrl,
        "UNSUPPORTED_SCHEME" => ErrorCode::UnsupportedScheme,
        "PATH_NOT_ALLOWED" => ErrorCode::PathNotAllowed,
        "ALREADY_EXISTS" => ErrorCode::AlreadyExists,
        "DISK_FULL" => ErrorCode::DiskFull,
        "NETWORK_TIMEOUT" => ErrorCode::NetworkTimeout,
        "HTTP_UNAUTHORIZED" => ErrorCode::HttpUnauthorized,
        "HTTP_FORBIDDEN" => ErrorCode::HttpForbidden,
        "HTTP_RATE_LIMIT" => ErrorCode::HttpRateLimit,
        "RESOURCE_CHANGED" => ErrorCode::ResourceChanged,
        "RANGE_UNSUPPORTED" => ErrorCode::RangeUnsupported,
        "CHECKSUM_MISMATCH" => ErrorCode::ChecksumMismatch,
        "TASK_NOT_FOUND" => ErrorCode::TaskNotFound,
        "INVALID_TRANSITION" => ErrorCode::InvalidTransition,
        _ => ErrorCode::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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

        let now = Utc::now();
        let task = DownloadTask {
            id: "t1".into(),
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
        };
        db.upsert_download(&task).unwrap();
        assert_eq!(db.list_downloads().unwrap().len(), 1);
        assert_eq!(db.get_download("t1").unwrap().file_name, "a.bin");
        assert_eq!(db.list_queues().unwrap()[0].task_count, 1);
    }
}
