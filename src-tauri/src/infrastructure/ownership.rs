use crate::domain::{AppError, ErrorCode};
use crate::infrastructure::errors::map_io_err;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

const LOCK_FILE: &str = ".circle-downloader.lock";

/// Exclusive ownership for cooperating app processes, released by the OS on exit.
/// Keep the file in place: unlinking it would allow competing locks on different inodes.
pub struct DataDirectoryLease {
    _file: File,
}

impl DataDirectoryLease {
    pub fn acquire(data_dir: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(data_dir).map_err(map_io_err)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(data_dir.join(LOCK_FILE)).map_err(map_io_err)?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => AppError::new(
                ErrorCode::DataDirectoryInUse,
                "Another Circle Downloader process is using this data folder. Close that process before reopening the app.",
            ),
            TryLockError::Error(error) => map_io_err(error),
        })?;
        Ok(Self { _file: file })
    }
}

impl Drop for DataDirectoryLease {
    fn drop(&mut self) {
        // End logical ownership even if a subprocess briefly inherited the open description
        // before exec. Process termination still releases the lock through the OS.
        let _ = self._file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropping_owner_unlocks_even_while_a_duplicate_file_description_exists() {
        let dir = tempfile::tempdir().unwrap();
        let lease = DataDirectoryLease::acquire(dir.path()).unwrap();
        let duplicate = lease._file.try_clone().unwrap();
        drop(lease);
        let _new_owner = DataDirectoryLease::acquire(dir.path()).unwrap();
        drop(duplicate);
    }
}
