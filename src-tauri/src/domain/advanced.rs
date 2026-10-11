use super::{AppError, ErrorCode};
use chrono::{DateTime, LocalResult, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

fn invalid(message: &str) -> AppError {
    AppError::new(ErrorCode::InvalidTransition, message)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskOptions {
    pub id: String,
    pub queue_id: String,
    pub priority: i32,
    pub category: String,
    /// Empty category means automatic. This derived value is never trusted on input.
    #[serde(skip_deserializing)]
    pub resolved_category: String,
    pub tags: Vec<String>,
    pub schedule: Option<Schedule>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub credential_id: Option<String>,
    pub connections: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub local_time: String,
    pub time_zone: String,
    /// Explicit resolution for a repeated local time. Nonexistent times are rejected.
    pub ambiguity: String,
}
impl Schedule {
    pub fn resolve(&self) -> Result<DateTime<Utc>, AppError> {
        let zone: chrono_tz::Tz = self
            .time_zone
            .parse()
            .map_err(|_| invalid("Choose an IANA time zone, such as Asia/Tehran."))?;
        let local = NaiveDateTime::parse_from_str(&self.local_time, "%Y-%m-%dT%H:%M")
            .map_err(|_| invalid("Choose a valid local date and time."))?;
        if !matches!(self.ambiguity.as_str(), "earlier" | "later") {
            return Err(invalid("Choose earlier or later for repeated local times."));
        }
        match zone.from_local_datetime(&local) {
            LocalResult::Single(time) => Ok(time.with_timezone(&Utc)),
            LocalResult::Ambiguous(first, last) => Ok(if self.ambiguity == "later" {
                last
            } else {
                first
            }
            .with_timezone(&Utc)),
            LocalResult::None => Err(invalid(
                "That local time does not exist because the clocks move forward. Choose another time.",
            )),
        }
    }
}
impl TaskOptions {
    pub fn validate(&mut self) -> Result<(), AppError> {
        if self.connections.is_some_and(|n| !(1..=4).contains(&n)) {
            return Err(invalid("Use between one and four connections."));
        }
        if !(-2..=2).contains(&self.priority) {
            return Err(invalid("Priority must be between -2 and 2."));
        }
        if self.category.len() > 80
            || self.tags.len() > 20
            || self.tags.iter().any(|t| t.is_empty() || t.len() > 40)
        {
            return Err(invalid(
                "Use a category of up to 80 characters and up to 20 tags of 40 characters.",
            ));
        }
        self.category = self.category.trim().to_owned();
        self.tags.sort();
        self.tags.dedup();
        self.scheduled_at = self.schedule.as_ref().map(Schedule::resolve).transpose()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuePolicy {
    pub id: String,
    pub name: String,
    pub paused: bool,
    pub max_concurrent: u32,
    /// Zero means unlimited; applies to all connections in this queue combined.
    pub bytes_per_second: u64,
    pub task_count: u64,
}
impl Default for QueuePolicy {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Default".into(),
            paused: false,
            max_concurrent: 3,
            bytes_per_second: 0,
            task_count: 0,
        }
    }
}
impl QueuePolicy {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.name.trim().is_empty()
            || self.name.len() > 80
            || !(1..=32).contains(&self.max_concurrent)
            || self.bytes_per_second > 1_000_000_000
        {
            return Err(invalid(
                "Use a queue name, 1–32 workers and a rate of at most 1 GB/s.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkSettings {
    pub bytes_per_second: u64,
    pub proxy_url: String,
    pub proxy_credential_id: Option<String>,
    pub connect_timeout_secs: u64,
}
impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            bytes_per_second: 0,
            proxy_url: String::new(),
            proxy_credential_id: None,
            connect_timeout_secs: 30,
        }
    }
}
impl NetworkSettings {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.bytes_per_second > 1_000_000_000 || !(1..=120).contains(&self.connect_timeout_secs)
        {
            return Err(invalid(
                "Use a rate of at most 1 GB/s and a connection timeout of 1–120 seconds.",
            ));
        }
        if !self.proxy_url.is_empty() {
            validate_scope(&self.proxy_url, "proxy")?;
        }
        if self.proxy_url.is_empty() && self.proxy_credential_id.is_some() {
            return Err(invalid(
                "Choose a proxy before authorizing proxy credentials.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialGrant {
    pub id: String,
    pub kind: String,
    pub scope: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCredential {
    pub kind: String,
    pub scope: String,
    pub username: String,
    pub password: String,
}
/// Origin-only grants: exact scheme, host and effective port; no paths or embedded secrets.
pub fn validate_scope(raw: &str, kind: &str) -> Result<String, AppError> {
    let url = url::Url::parse(raw)
        .map_err(|_| invalid("Use an origin URL without a path, query or embedded credentials."))?;
    if !matches!(kind, "origin" | "proxy")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || (kind == "origin" && url.scheme() != "https")
        || !matches!(url.scheme(), "http" | "https")
    {
        return Err(invalid(
            "Origin credentials require HTTPS. Use an exact origin without a path, query or embedded credentials.",
        ));
    }
    Ok(url.origin().ascii_serialization())
}
pub fn authorized(grant: &CredentialGrant, kind: &str, url: &str) -> bool {
    grant.kind == kind
        && url::Url::parse(url).is_ok_and(|u| u.origin().ascii_serialization() == grant.scope)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub transferred_bytes: u64,
    pub transfer_millis: u64,
    pub completed_attempts: u64,
    pub failed_attempts: u64,
    pub days: Vec<DailyMeasurement>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyMeasurement {
    pub day: String,
    pub transferred_bytes: u64,
    pub transfer_millis: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dst_gap_is_rejected_and_fold_is_explicit() {
        let schedule = |local: &str, ambiguity: &str| Schedule {
            local_time: local.into(),
            time_zone: "America/New_York".into(),
            ambiguity: ambiguity.into(),
        };
        assert!(schedule("2026-03-08T02:30", "earlier").resolve().is_err());
        let first = schedule("2026-11-01T01:30", "earlier").resolve().unwrap();
        let last = schedule("2026-11-01T01:30", "later").resolve().unwrap();
        assert_eq!((last - first).num_seconds(), 3600);
        assert_eq!(first.to_rfc3339(), "2026-11-01T05:30:00+00:00");
        assert!(schedule("2026-11-01T01:30", "unknown").resolve().is_err());
        assert!(
            Schedule {
                local_time: "2026-10-11T10:00".into(),
                time_zone: "Invalid/Zone".into(),
                ambiguity: "earlier".into()
            }
            .resolve()
            .is_err()
        );
    }
    #[test]
    fn grants_are_exact_origin_and_do_not_accept_embedded_secrets() {
        let grant = CredentialGrant {
            id: "opaque".into(),
            kind: "origin".into(),
            scope: validate_scope("https://example.com", "origin").unwrap(),
        };
        assert!(authorized(
            &grant,
            "origin",
            "https://example.com/file?a=secret"
        ));
        for target in [
            "http://example.com/file",
            "https://example.com:444/file",
            "https://other.example.com/file",
            "https://example.com.evil/file",
        ] {
            assert!(!authorized(&grant, "origin", target));
        }
        assert!(!authorized(&grant, "proxy", "https://example.com/"));
        for scope in [
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com/?token=secret",
            "http://example.com",
        ] {
            assert!(validate_scope(scope, "origin").is_err());
        }
    }
}
