pub mod error;
pub mod filename;
pub mod status;
pub mod task;
pub mod transitions;
pub mod url_policy;

pub use error::{AppError, ErrorCode};
pub use status::DownloadStatus;
pub use task::{
    CreateDownloadRequest, DownloadTask, ProgressEvent, Queue, RemoveDownloadRequest,
    RemoveDownloadResult, Settings, TaskError, UpdateSettingsRequest,
};
