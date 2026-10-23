use super::{retryable_io_error, RetryMmapDirectory};
use std::io;
#[cfg(windows)]
use std::time::Duration;

#[test]
fn retryable_error_kinds_cover_transient_io_failures() {
    for kind in [
        io::ErrorKind::Interrupted,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::WouldBlock,
    ] {
        assert!(retryable_io_error(&io::Error::from(kind)));
    }
    assert!(!retryable_io_error(&io::Error::from(
        io::ErrorKind::InvalidData
    )));
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path, thread};
    use tantivy::directory::Directory;
    use tempfile::TempDir;

    const NO_SHARING: u32 = 0;

    fn release_after(file: std::fs::File, delay: Duration) {
        thread::spawn(move || {
            thread::sleep(delay);
            drop(file);
        });
    }

    #[test]
    fn open_write_retries_a_short_sharing_violation() {
        let temp = TempDir::new().unwrap();
        let directory = RetryMmapDirectory::open(temp.path()).unwrap();
        let relative = Path::new("segment.store");
        let absolute = temp.path().join(relative);
        drop(
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&absolute)
                .unwrap(),
        );
        let locked = OpenOptions::new()
            .write(true)
            .share_mode(NO_SHARING)
            .open(&absolute)
            .unwrap();
        let release_path = absolute.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(15));
            drop(locked);
            std::fs::remove_file(release_path).unwrap();
        });

        let mut writer = directory.open_write(relative).unwrap();
        std::io::Write::write_all(&mut writer, b"segment").unwrap();
    }

    #[test]
    fn atomic_write_retries_a_short_replace_violation() {
        let temp = TempDir::new().unwrap();
        let directory = RetryMmapDirectory::open(temp.path()).unwrap();
        let relative = Path::new("meta.json");
        let absolute = temp.path().join(relative);
        std::fs::write(&absolute, b"old").unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(NO_SHARING)
            .open(&absolute)
            .unwrap();
        release_after(locked, Duration::from_millis(15));

        directory.atomic_write(relative, b"new").unwrap();

        assert_eq!(std::fs::read(absolute).unwrap(), b"new");
    }

    #[test]
    fn delete_retries_a_short_sharing_violation() {
        let temp = TempDir::new().unwrap();
        let directory = RetryMmapDirectory::open(temp.path()).unwrap();
        let relative = Path::new("obsolete.segment");
        let absolute = temp.path().join(relative);
        std::fs::write(&absolute, b"old").unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(NO_SHARING)
            .open(&absolute)
            .unwrap();
        release_after(locked, Duration::from_millis(15));

        directory.delete(relative).unwrap();

        assert!(!absolute.exists());
    }
}
