use super::errors::map_io_err;
use crate::domain::{AppError, ErrorCode};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path};

#[derive(Debug)]
pub struct VerifiedFile {
    pub file: std::fs::File,
    pub metadata: std::fs::Metadata,
}
impl VerifiedFile {
    pub fn unchanged(&self) -> Result<(), AppError> {
        let current = self.file.metadata().map_err(map_io_err)?;
        if current.len() != self.metadata.len()
            || current.modified().map_err(map_io_err)?
                != self.metadata.modified().map_err(map_io_err)?
        {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The partial file changed during verification. Add a fresh download.",
            ));
        }
        Ok(())
    }
}

/// All managed file operations are relative to this retained directory capability.
pub struct ManagedFiles {
    dir: Dir,
}
impl ManagedFiles {
    pub fn new(path: &Path) -> Result<Self, AppError> {
        let parent = Dir::open_ambient_dir(
            path.parent().ok_or_else(unsafe_path)?,
            cap_std::ambient_authority(),
        )
        .map_err(map_io_err)?;
        let name = path.file_name().ok_or_else(unsafe_path)?;
        match parent.create_dir(name) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(map_io_err(e)),
        }
        let dir = parent.open_dir_nofollow(name).map_err(|_| unsafe_path())?;
        Ok(Self { dir })
    }
    pub fn name<'a>(&self, path: &'a Path) -> Result<&'a std::ffi::OsStr, AppError> {
        path.file_name().ok_or_else(unsafe_path)
    }
    fn checked(name: &Path) -> Result<(), AppError> {
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
        {
            return Err(unsafe_path());
        }
        Ok(())
    }
    pub fn open(&self, name: &Path, write: bool, create: bool) -> Result<std::fs::File, AppError> {
        Self::checked(name)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(write)
            .create_new(create)
            .follow(FollowSymlinks::No);
        let file = self.dir.open_with(name, &options).map_err(|e| {
            if self
                .dir
                .symlink_metadata(name)
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                unsafe_path()
            } else {
                map_io_err(e)
            }
        })?;
        let metadata = file.metadata().map_err(map_io_err)?;
        if !metadata.is_file() {
            return Err(unsafe_path());
        }
        if write && metadata.nlink() != 1 {
            return Err(unsafe_path());
        }
        Ok(file.into_std())
    }
    pub fn partial(&self, name: &Path, offset: u64) -> Result<std::fs::File, AppError> {
        let mut file = match self.open(name, true, offset == 0) {
            Err(e) if offset == 0 && e.code == ErrorCode::AlreadyExists => {
                self.open(name, true, false)?
            }
            result => result?,
        };
        if file.metadata().map_err(map_io_err)?.len() != offset {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The partial file changed. Add a fresh download.",
            ));
        }
        file.seek(SeekFrom::Start(offset)).map_err(map_io_err)?;
        Ok(file)
    }
    pub fn length(&self, name: &Path) -> Result<Option<u64>, AppError> {
        match self.open(name, false, false) {
            Ok(file) => Ok(Some(file.metadata().map_err(map_io_err)?.len())),
            Err(e) if e.code == ErrorCode::StorageNotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn digest(&self, name: &Path) -> Result<String, AppError> {
        let mut file = self.open(name, false, false)?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = file.read(&mut buffer).map_err(map_io_err)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        Ok(hex::encode(hash.finalize()))
    }
    pub fn publish(
        &self,
        partial: &Path,
        final_name: &Path,
        digest: &str,
        verified: Option<&VerifiedFile>,
    ) -> Result<(), AppError> {
        Self::checked(partial)?;
        Self::checked(final_name)?;
        if let Some(verified) = verified {
            self.same_file(partial, verified)?;
        } else if self.digest(partial)? != digest {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "The partial file changed after verification.",
            ));
        }
        // No-replace publication. A substituted symlink is never opened for writing.
        self.dir
            .hard_link(partial, &self.dir, final_name)
            .map_err(map_io_err)?;
        self.sync()?;
        if let Some(verified) = verified {
            self.same_file(final_name, verified)?;
        } else if self.digest(final_name)? != digest {
            return Err(AppError::new(
                ErrorCode::ChecksumMismatch,
                "The published file changed. Restore the verified file.",
            ));
        }
        Ok(())
    }
    fn same_file(&self, name: &Path, verified: &VerifiedFile) -> Result<(), AppError> {
        verified.unchanged()?;
        let current =
            same_file::Handle::from_file(self.open(name, false, false)?).map_err(map_io_err)?;
        let verified = same_file::Handle::from_file(verified.file.try_clone().map_err(map_io_err)?)
            .map_err(map_io_err)?;
        if current != verified {
            return Err(AppError::new(
                ErrorCode::ResourceChanged,
                "The verified partial was replaced. Both files were kept.",
            ));
        }
        Ok(())
    }
    pub fn remove(&self, name: &Path) -> Result<(), AppError> {
        Self::checked(name)?;
        match self.dir.remove_file(name) {
            Ok(()) => self.sync(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(map_io_err(e)),
        }
    }
    pub fn sync(&self) -> Result<(), AppError> {
        #[cfg(unix)]
        self.dir
            .try_clone()
            .map_err(map_io_err)?
            .into_std_file()
            .sync_all()
            .map_err(map_io_err)?;
        // Windows lacks a portable directory fsync; file sync and SQLite FULL still apply.
        Ok(())
    }
}
fn unsafe_path() -> AppError {
    AppError::new(
        ErrorCode::PathNotAllowed,
        "The download folder or file is unsafe. Restore the app-managed folder and add a fresh download.",
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn retained_directory_prevents_parent_replacement_escape() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = root.path().join("downloads");
        let files = ManagedFiles::new(&path).unwrap();
        std::fs::rename(&path, root.path().join("retained")).unwrap();
        std::os::unix::fs::symlink(outside.path(), &path).unwrap();
        files
            .partial(Path::new("safe.part"), 0)
            .unwrap()
            .write_all(b"safe")
            .unwrap();
        assert!(!outside.path().join("safe.part").exists());
        assert_eq!(
            std::fs::read(root.path().join("retained/safe.part")).unwrap(),
            b"safe"
        );
        assert!(ManagedFiles::new(&path).is_err());
    }
    #[test]
    fn rejects_symlinks_hardlinked_writers_and_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        let files = ManagedFiles::new(&root.path().join("downloads")).unwrap();
        let victim = root.path().join("victim");
        std::fs::write(&victim, b"secret").unwrap();
        std::os::unix::fs::symlink(&victim, root.path().join("downloads/link")).unwrap();
        std::fs::hard_link(&victim, root.path().join("downloads/hard")).unwrap();
        for name in ["link", "hard", "../victim", "/tmp/escape"] {
            assert!(files.partial(Path::new(name), 6).is_err(), "{name}");
        }
        assert_eq!(std::fs::read(victim).unwrap(), b"secret");
    }
}

#[cfg(all(test, unix))]
mod publication_tests {
    use super::*;
    #[test]
    fn publication_rejects_substituted_source_and_existing_symlink_destination() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("downloads");
        let files = ManagedFiles::new(&path).unwrap();
        let partial = Path::new("file.part");
        std::fs::write(path.join(partial), b"verified").unwrap();
        let file = files.open(partial, false, false).unwrap();
        let verified = VerifiedFile {
            metadata: file.metadata().unwrap(),
            file,
        };
        std::fs::rename(path.join(partial), path.join("old.part")).unwrap();
        std::fs::write(path.join(partial), b"substitute").unwrap();
        assert_eq!(
            files
                .publish(partial, Path::new("final"), "unused", Some(&verified))
                .unwrap_err()
                .code,
            ErrorCode::ResourceChanged
        );
        assert!(!path.join("final").exists());
        let victim = root.path().join("victim");
        std::fs::write(&victim, b"secret").unwrap();
        std::os::unix::fs::symlink(&victim, path.join("final")).unwrap();
        let file = files.open(partial, false, false).unwrap();
        let verified = VerifiedFile {
            metadata: file.metadata().unwrap(),
            file,
        };
        assert_eq!(
            files
                .publish(partial, Path::new("final"), "unused", Some(&verified))
                .unwrap_err()
                .code,
            ErrorCode::AlreadyExists
        );
        assert_eq!(std::fs::read(victim).unwrap(), b"secret");
    }
    #[test]
    fn publication_rejects_content_changes_after_hashing() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("downloads");
        let files = ManagedFiles::new(&path).unwrap();
        let partial = Path::new("file.part");
        std::fs::write(path.join(partial), b"verified").unwrap();
        let file = files.open(partial, false, false).unwrap();
        let verified = VerifiedFile {
            metadata: file.metadata().unwrap(),
            file,
        };
        std::fs::write(path.join(partial), b"changed after hashing").unwrap();
        assert_eq!(
            files
                .publish(partial, Path::new("final"), "unused", Some(&verified))
                .unwrap_err()
                .code,
            ErrorCode::ResourceChanged
        );
        assert!(!path.join("final").exists());
    }
}
