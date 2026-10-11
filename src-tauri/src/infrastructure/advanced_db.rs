use super::*;
use crate::domain::advanced::*;

pub(super) fn migrate(tx: &rusqlite::Transaction<'_>) -> Result<(), AppError> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS download_sources(task_id TEXT PRIMARY KEY, effective_url TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS task_options(task_id TEXT PRIMARY KEY, priority INTEGER NOT NULL DEFAULT 0, scheduled_at TEXT, data TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS idx_task_schedule ON task_options(scheduled_at);
    CREATE TABLE IF NOT EXISTS queue_policies(queue_id TEXT PRIMARY KEY, data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS network_settings(id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS credential_grants(id TEXT PRIMARY KEY, kind TEXT NOT NULL, scope TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS transfer_measurements(id TEXT PRIMARY KEY, task_id TEXT NOT NULL, day TEXT NOT NULL, bytes INTEGER NOT NULL, millis INTEGER NOT NULL, outcome TEXT NOT NULL);
    CREATE INDEX IF NOT EXISTS idx_measurement_day ON transfer_measurements(day);
    CREATE INDEX IF NOT EXISTS idx_runnable ON downloads(status,next_retry_at,queue_id,queue_position);").map_err(map_db_err)
}
fn encode<T: serde::Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value)
        .map_err(|_| AppError::new(ErrorCode::Internal, "Could not encode preferences."))
}
fn decode<T: serde::de::DeserializeOwned>(value: String) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}
impl Database {
    pub fn download_source(&self, id: &str) -> Result<Option<String>, AppError> {
        self.conn
            .lock()
            .query_row(
                "SELECT effective_url FROM download_sources WHERE task_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_db_err)
    }
    pub fn save_download_source(&self, id: &str, url: &str) -> Result<(), AppError> {
        self.conn.lock().execute("INSERT INTO download_sources(task_id,effective_url) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET effective_url=excluded.effective_url",params![id,url]).map_err(map_db_err)?;
        Ok(())
    }
    pub fn task_options(&self, id: &str) -> Result<TaskOptions, AppError> {
        let task = self.get_download(id)?;
        let stored: Option<String> = self
            .conn
            .lock()
            .query_row(
                "SELECT data FROM task_options WHERE task_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(map_db_err)?;
        let mut options = stored
            .map(decode::<TaskOptions>)
            .transpose()
            .map_err(map_db_err)?
            .unwrap_or_default();
        options.id = id.into();
        options.queue_id = task.queue_id;
        options.resolved_category = if options.category.is_empty() {
            crate::domain::category::from_filename(&task.file_name)
        } else {
            options.category.clone()
        };
        Ok(options)
    }
    pub fn all_task_options(&self) -> Result<Vec<TaskOptions>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT d.id,d.queue_id,o.data,d.file_name FROM downloads d LEFT JOIN task_options o ON o.task_id=d.id ORDER BY d.created_at,d.id").map_err(map_db_err)?;
        stmt.query_map([], |r| {
            let stored: Option<String> = r.get(2)?;
            let mut value = stored
                .map(decode::<TaskOptions>)
                .transpose()?
                .unwrap_or_default();
            value.id = r.get(0)?;
            value.queue_id = r.get(1)?;
            value.resolved_category = if value.category.is_empty() {
                crate::domain::category::from_filename(&r.get::<_, String>(3)?)
            } else {
                value.category.clone()
            };
            Ok(value)
        })
        .map_err(map_db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_err)
    }
    pub fn save_task_options(&self, options: &TaskOptions) -> Result<(), AppError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(map_db_err)?;
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM queues WHERE id=?1)",
                [&options.queue_id],
                |r| r.get(0),
            )
            .map_err(map_db_err)?;
        if !exists {
            return Err(AppError::new(
                ErrorCode::InvalidTransition,
                "Choose an existing queue.",
            ));
        }
        let changed = tx
            .execute(
                "UPDATE downloads SET queue_id=?1, revision=revision+1, updated_at=?2 WHERE id=?3",
                params![options.queue_id, Utc::now().to_rfc3339(), options.id],
            )
            .map_err(map_db_err)?;
        if changed == 0 {
            return Err(AppError::new(
                ErrorCode::TaskNotFound,
                "Download not found.",
            ));
        }
        tx.execute("INSERT INTO task_options(task_id,priority,scheduled_at,data) VALUES(?1,?2,?3,?4) ON CONFLICT(task_id) DO UPDATE SET priority=excluded.priority,scheduled_at=excluded.scheduled_at,data=excluded.data", params![options.id,options.priority,options.scheduled_at.map(|d| d.to_rfc3339()),encode(options)?]).map_err(map_db_err)?;
        tx.commit().map_err(map_db_err)
    }
    pub fn queue_policies(&self) -> Result<Vec<QueuePolicy>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT q.id,q.name,p.data,(SELECT COUNT(*) FROM downloads d WHERE d.queue_id=q.id AND d.status NOT IN ('completed','canceled','failed')) FROM queues q LEFT JOIN queue_policies p ON p.queue_id=q.id ORDER BY q.id").map_err(map_db_err)?;
        stmt.query_map([], |r| {
            let json: Option<String> = r.get(2)?;
            let mut p = json
                .map(decode::<QueuePolicy>)
                .transpose()?
                .unwrap_or_default();
            p.id = r.get(0)?;
            p.name = r.get(1)?;
            p.task_count = r.get::<_, i64>(3)? as u64;
            Ok(p)
        })
        .map_err(map_db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_err)
    }
    pub fn save_queue(&self, policy: &QueuePolicy) -> Result<(), AppError> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(map_db_err)?;
        tx.execute("INSERT INTO queues(id,name) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET name=excluded.name",params![policy.id,policy.name]).map_err(map_db_err)?;
        tx.execute("INSERT INTO queue_policies(queue_id,data) VALUES(?1,?2) ON CONFLICT(queue_id) DO UPDATE SET data=excluded.data",params![policy.id,encode(policy)?]).map_err(map_db_err)?;
        tx.commit().map_err(map_db_err)
    }
    pub fn network_settings(&self) -> Result<NetworkSettings, AppError> {
        let json: Option<String> = self
            .conn
            .lock()
            .query_row("SELECT data FROM network_settings WHERE id=1", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(map_db_err)?;
        json.map(decode)
            .transpose()
            .map_err(map_db_err)
            .map(|v| v.unwrap_or_default())
    }
    pub fn save_network(&self, settings: &NetworkSettings) -> Result<(), AppError> {
        self.conn.lock().execute("INSERT INTO network_settings(id,data) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET data=excluded.data",[encode(settings)?]).map_err(map_db_err)?;
        Ok(())
    }
    pub fn grants(&self) -> Result<Vec<CredentialGrant>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT id,kind,scope FROM credential_grants ORDER BY id")
            .map_err(map_db_err)?;
        stmt.query_map([], |r| {
            Ok(CredentialGrant {
                id: r.get(0)?,
                kind: r.get(1)?,
                scope: r.get(2)?,
            })
        })
        .map_err(map_db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_db_err)
    }
    pub fn save_grant(&self, grant: &CredentialGrant) -> Result<(), AppError> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO credential_grants(id,kind,scope) VALUES(?1,?2,?3)",
                params![grant.id, grant.kind, grant.scope],
            )
            .map_err(map_db_err)?;
        Ok(())
    }
    pub fn revoke_grant(&self, id: &str) -> Result<(), AppError> {
        self.conn
            .lock()
            .execute("DELETE FROM credential_grants WHERE id=?1", [id])
            .map_err(map_db_err)?;
        Ok(())
    }
    pub fn record_measurement(
        &self,
        id: &str,
        task_id: &str,
        bytes: u64,
        millis: u64,
        outcome: &str,
    ) -> Result<(), AppError> {
        self.conn.lock().execute("INSERT OR IGNORE INTO transfer_measurements(id,task_id,day,bytes,millis,outcome) VALUES(?1,?2,?3,?4,?5,?6)",params![id,task_id,Utc::now().format("%Y-%m-%d").to_string(),bytes.min(i64::MAX as u64) as i64,millis.min(i64::MAX as u64) as i64,outcome]).map_err(map_db_err)?;
        Ok(())
    }
    pub fn statistics(&self) -> Result<Statistics, AppError> {
        let conn = self.conn.lock();
        let (bytes,millis,completed,failed) = conn.query_row("SELECT COALESCE(SUM(bytes),0),COALESCE(SUM(millis),0),COALESCE(SUM(outcome='completed'),0),COALESCE(SUM(outcome='failed'),0) FROM transfer_measurements",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(map_db_err)?;
        let mut stmt=conn.prepare("SELECT day,SUM(bytes),SUM(millis) FROM transfer_measurements GROUP BY day ORDER BY day DESC LIMIT 30").map_err(map_db_err)?;
        let days = stmt
            .query_map([], |r| {
                Ok(DailyMeasurement {
                    day: r.get(0)?,
                    transferred_bytes: r.get::<_, i64>(1)? as u64,
                    transfer_millis: r.get::<_, i64>(2)? as u64,
                })
            })
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(Statistics {
            transferred_bytes: bytes as u64,
            transfer_millis: millis as u64,
            completed_attempts: completed as u64,
            failed_attempts: failed as u64,
            days,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_policy_schedule_priority_and_measurements_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("advanced.db");
        {
            let db = Database::open(&path).unwrap();
            let mut t = super::super::tests::sample_task("one");
            t.request_id = None;
            db.upsert_download(&t).unwrap();
            t.id = "two".into();
            db.upsert_download(&t).unwrap();
            let mut options = db.task_options("one").unwrap();
            options.priority = 2;
            options.category = "Work".into();
            options.tags = vec!["reports".into()];
            options.schedule = Some(Schedule {
                local_time: "2030-11-03T01:30".into(),
                time_zone: "America/New_York".into(),
                ambiguity: "later".into(),
            });
            options.validate().unwrap();
            db.save_task_options(&options).unwrap();
            assert_eq!(db.list_runnable_queued(Utc::now()).unwrap()[0].id, "two");
            db.record_measurement("attempt", "one", 1024, 100, "completed")
                .unwrap();
            db.record_measurement("attempt", "one", 1024, 100, "completed")
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.task_options("one").unwrap().category, "Work");
        assert_eq!(db.statistics().unwrap().transferred_bytes, 1024);
        db.remove_download("one").unwrap();
        assert_eq!(db.statistics().unwrap().completed_attempts, 1);
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    #[test]
    fn aging_eventually_outweighs_priority_and_future_schedules_are_excluded() {
        let db = Database::open_in_memory().unwrap();
        for (id, priority, age) in [
            ("old-low", -2, 600),
            ("new-high", 2, 0),
            ("new-normal", 0, 0),
        ] {
            let mut task = super::super::tests::sample_task(id);
            task.request_id = None;
            task.created_at = Utc::now() - chrono::Duration::seconds(age);
            db.upsert_download(&task).unwrap();
            let mut options = db.task_options(id).unwrap();
            options.priority = priority;
            db.save_task_options(&options).unwrap();
        }
        assert_eq!(
            db.list_runnable_queued(Utc::now())
                .unwrap()
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["old-low", "new-high", "new-normal"]
        );
        let mut options = db.task_options("old-low").unwrap();
        options.scheduled_at = Some(Utc::now() + chrono::Duration::hours(2));
        db.save_task_options(&options).unwrap();
        assert_eq!(db.list_runnable_queued(Utc::now()).unwrap().len(), 2);
        assert!(db.earliest_retry_at().unwrap().unwrap() > Utc::now());
    }
    #[test]
    #[ignore = "repeated large-history benchmark; run with --ignored --nocapture"]
    fn large_history_benchmark() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("history.db")).unwrap();
        {
            // Fixed dataset and transaction: setup is excluded from timing.
            let mut conn = db.conn.lock();
            let tx = conn.transaction().unwrap();
            for i in 0..20_000 {
                tx.execute("INSERT INTO downloads(id,url,file_name,destination_id,status,created_at,updated_at,queue_id,queue_position) VALUES(?1,'https://example.com/file','file.zip','default',?2,'2026-10-11T00:00:00+00:00','2026-10-11T00:00:00+00:00','default',?3)",params![format!("history-{i}"),if i%100==0{"queued"}else{"completed"},i]).unwrap();
                tx.execute("INSERT INTO transfer_measurements(id,task_id,day,bytes,millis,outcome) VALUES(?1,?1,'2026-10-11',65536,100,'completed')",[format!("attempt-{i}")]).unwrap();
            }
            tx.commit().unwrap();
        }
        let mut history = vec![];
        let mut runnable = vec![];
        let mut statistics = vec![];
        // One untimed warmup, then seven runs against identical data/connection/WAL settings.
        for round in 0..8 {
            let start = std::time::Instant::now();
            assert_eq!(db.list_downloads().unwrap().len(), 20_000);
            let a = start.elapsed().as_secs_f64() * 1000.;
            let start = std::time::Instant::now();
            assert_eq!(db.list_runnable_queued(Utc::now()).unwrap().len(), 200);
            let b = start.elapsed().as_secs_f64() * 1000.;
            let start = std::time::Instant::now();
            assert_eq!(db.statistics().unwrap().completed_attempts, 20_000);
            let c = start.elapsed().as_secs_f64() * 1000.;
            if round > 0 {
                history.push(a);
                runnable.push(b);
                statistics.push(c);
            }
        }
        for (name, mut samples) in [
            ("history_20000", history),
            ("runnable_200", runnable),
            ("statistics_20000", statistics),
        ] {
            let raw = samples.clone();
            samples.sort_by(f64::total_cmp);
            let mean = samples.iter().sum::<f64>() / samples.len() as f64;
            let variance = samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / (samples.len() - 1) as f64;
            eprintln!(
                "N4B {name}: samples_ms={raw:?}; median_ms={}; sample_variance_ms2={variance}",
                samples[samples.len() / 2]
            );
        }
    }
}
