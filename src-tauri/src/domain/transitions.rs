use super::error::{AppError, ErrorCode};
use super::status::DownloadStatus;

pub fn can_transition(from: DownloadStatus, to: DownloadStatus) -> bool {
    use DownloadStatus::*;
    matches!(
        (from, to),
        (Queued, Connecting)
            | (Queued, Paused)
            | (Queued, Canceled)
            | (Connecting, Downloading)
            | (Connecting, Failed)
            | (Connecting, Paused)
            | (Connecting, Canceled)
            | (Downloading, Paused)
            | (Downloading, Verifying)
            | (Downloading, Failed)
            | (Downloading, Canceled)
            | (Paused, Queued)
            | (Paused, Canceled)
            | (Failed, Queued)
            | (Failed, Canceled)
            | (Verifying, Completed)
            | (Verifying, Failed)
            | (Verifying, Canceled)
    )
}

pub fn ensure_transition(from: DownloadStatus, to: DownloadStatus) -> Result<(), AppError> {
    if can_transition(from, to) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorCode::InvalidTransition,
            format!(
                "Cannot transition from {} to {}",
                from.as_str(),
                to.as_str()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_happy_path() {
        assert!(can_transition(
            DownloadStatus::Queued,
            DownloadStatus::Connecting
        ));
        assert!(can_transition(
            DownloadStatus::Downloading,
            DownloadStatus::Verifying
        ));
        assert!(can_transition(
            DownloadStatus::Verifying,
            DownloadStatus::Completed
        ));
    }

    #[test]
    fn rejects_illegal_transitions() {
        assert!(!can_transition(
            DownloadStatus::Completed,
            DownloadStatus::Queued
        ));
        assert!(!can_transition(
            DownloadStatus::Canceled,
            DownloadStatus::Downloading
        ));
        assert!(ensure_transition(DownloadStatus::Failed, DownloadStatus::Downloading).is_err());
    }

    #[test]
    fn resume_and_retry() {
        assert!(can_transition(
            DownloadStatus::Paused,
            DownloadStatus::Queued
        ));
        assert!(can_transition(
            DownloadStatus::Failed,
            DownloadStatus::Queued
        ));
    }
}
