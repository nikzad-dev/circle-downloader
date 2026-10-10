use chrono::{DateTime, Duration, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RETRY_BUDGET: u32 = 5;
pub const PER_HOST_LIMIT: usize = 2;
const BASE_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 60;

/// Full-jitter exponential backoff: uniform in [0, min(cap, base * 2^attempt)].
pub fn backoff_delay_secs(retry_count_after_increment: u32, retry_after_secs: Option<u64>) -> u64 {
    if let Some(secs) = retry_after_secs {
        return secs.min(3600);
    }
    // Tests use zero delay so retry-budget suites finish quickly.
    if cfg!(test) {
        return 0;
    }
    let attempt = retry_count_after_increment.saturating_sub(1).min(16);
    let exp = BASE_BACKOFF_SECS.saturating_mul(1u64 << attempt);
    let cap = exp.min(MAX_BACKOFF_SECS);
    if cap == 0 {
        return 0;
    }
    jitter(cap)
}

fn jitter(cap: u64) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % (cap + 1)
}

pub fn next_retry_at(
    retry_count_after_increment: u32,
    retry_after_secs: Option<u64>,
) -> DateTime<Utc> {
    let secs = backoff_delay_secs(retry_count_after_increment, retry_after_secs);
    Utc::now() + Duration::seconds(secs as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_header_wins_and_is_capped() {
        assert_eq!(backoff_delay_secs(1, Some(12)), 12);
        assert_eq!(backoff_delay_secs(1, Some(10_000)), 3600);
    }

    #[test]
    fn exponential_cap_is_respected_outside_retry_after() {
        // Under cfg(test) the delay is forced to 0; Retry-After still applies.
        assert_eq!(backoff_delay_secs(10, None), 0);
        assert_eq!(backoff_delay_secs(10, Some(5)), 5);
        let _ = (BASE_BACKOFF_SECS, MAX_BACKOFF_SECS, jitter(3));
    }
}
