use crate::domain::ProgressEvent;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::ipc::Channel;

#[derive(Clone, Default)]
pub struct ProgressHub {
    listeners: Arc<Mutex<Vec<Channel<ProgressEvent>>>>,
}

impl ProgressHub {
    pub fn subscribe(&self, channel: Channel<ProgressEvent>) {
        self.listeners.lock().push(channel);
    }

    pub fn emit(&self, event: ProgressEvent) {
        let mut listeners = self.listeners.lock();
        listeners.retain(|channel| channel.send(event.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DownloadStatus;
    use chrono::Utc;

    #[test]
    fn event_revision_is_monotonic_in_payload() {
        let first = ProgressEvent {
            task_id: "t".into(),
            revision: 1,
            transferred_bytes: 1,
            total_bytes: Some(10),
            bytes_per_second: 1,
            status: DownloadStatus::Downloading,
            timestamp: Utc::now(),
        };
        let second = ProgressEvent {
            revision: 2,
            ..first.clone()
        };
        assert!(second.revision > first.revision);
    }
}
