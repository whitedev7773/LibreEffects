//! One editor per user, across executable names and build directories.
//! The OS releases the lock on a crash; the lock file must never be unlinked.
use std::{
    fs::{File, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn report_start_error(error: &str) {
    eprintln!("Cannot start Libre Effects: {error}");
    #[cfg(windows)]
    {
        use windows::{
            Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
            core::PCWSTR,
        };
        let message: Vec<u16> = format!("Cannot start Libre Effects.\n\n{error}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let title: Vec<u16> = "Libre Effects".encode_utf16().chain(Some(0)).collect();
        // Both UTF-16 buffers remain alive for the synchronous native dialog.
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

pub(crate) struct Instance {
    _lock: File,
    activation: PathBuf,
    last_request: Vec<u8>,
}

impl Instance {
    pub fn acquire() -> Result<Option<Self>, String> {
        let root = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_STATE_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
            .ok_or("Cannot locate the user data directory for the editor instance lock")?
            .join("LibreEffects");
        Self::replace_in_directory(&root, std::time::Duration::from_secs(60))
            .map_err(|error| error.to_string())
    }

    fn replace_in_directory(root: &Path, timeout: std::time::Duration) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
        // Only one replacement may wait for the owner. Rapid double-clicks
        // must not create a succession of editors terminating one another.
        let launch = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("editor.launch.lock"))?;
        match launch.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(error)) => return Err(error),
        }
        if let Some(owner) = Self::in_directory(root)? {
            return Ok(Some(owner));
        }
        let deadline = std::time::Instant::now() + timeout;
        loop {
            // The successor never opens its window until the old process has
            // released its lifetime lock (including recovery and child cleanup).
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Some(owner) = Self::try_owner(root)? {
                return Ok(Some(owner));
            }
            if std::time::Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "The existing editor could not finish saving its recovery copy or closing a dialog. No second editor was opened.",
                ));
            }
        }
    }

    fn try_owner(root: &Path) -> io::Result<Option<Self>> {
        let activation = root.join("editor.activate");
        let last_request = read_request(&activation).unwrap_or_default();
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("editor.lock"))?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(Self {
                _lock: lock,
                activation,
                last_request,
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }

    fn in_directory(root: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
        if let Some(owner) = Self::try_owner(root)? {
            return Ok(Some(owner));
        }
        let token = format!(
            "replace:{}:{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        );
        let mut signal = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("editor.activate"))?;
        signal.lock()?;
        signal.set_len(0)?;
        signal.write_all(token.as_bytes())?;
        Ok(None)
    }

    pub fn take_activation(&mut self) -> bool {
        let Ok(request) = read_request(&self.activation) else {
            return false;
        };
        if request.is_empty() || request == self.last_request {
            return false;
        }
        self.last_request = request;
        true
    }
}

fn read_request(path: &Path) -> io::Result<Vec<u8>> {
    let mut value = Vec::new();
    let file = File::open(path)?;
    file.try_lock_shared().map_err(io::Error::other)?;
    file.take(256).read_to_end(&mut value)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_waits_for_cleanup_and_concurrent_launches_do_not_cascade() {
        let root = tempfile::tempdir().unwrap();
        let mut owner = Instance::in_directory(root.path()).unwrap().unwrap();
        std::thread::scope(|scope| {
            let next = scope.spawn(|| {
                Instance::replace_in_directory(root.path(), std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap()
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while !owner.take_activation() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(Instance::try_owner(root.path()).unwrap().is_none());
            assert!(
                Instance::replace_in_directory(root.path(), std::time::Duration::from_millis(50))
                    .unwrap()
                    .is_none()
            );
            drop(owner);
            let mut successor = next.join().unwrap();
            assert!(!successor.take_activation());
            assert!(Instance::try_owner(root.path()).unwrap().is_none());
        });
        assert!(Instance::try_owner(root.path()).unwrap().is_some());
    }
    #[test]
    fn unresponsive_owner_never_allows_a_second_window() {
        let root = tempfile::tempdir().unwrap();
        let _owner = Instance::in_directory(root.path()).unwrap().unwrap();
        assert_eq!(
            Instance::replace_in_directory(root.path(), std::time::Duration::ZERO)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(Instance::try_owner(root.path()).unwrap().is_none());
    }

    #[test]
    fn replacement_request_does_not_release_owner_before_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let mut first = Instance::in_directory(root.path()).unwrap().unwrap();
        assert!(!first.take_activation());
        assert!(Instance::in_directory(root.path()).unwrap().is_none());
        assert!(first.take_activation());
        assert!(!first.take_activation());
        assert!(Instance::in_directory(root.path()).unwrap().is_none());
        assert!(first.take_activation());
        drop(first);
        let mut next = Instance::in_directory(root.path()).unwrap().unwrap();
        assert!(
            !next.take_activation(),
            "stale requests must not steal focus"
        );
        assert!(Instance::in_directory(root.path()).unwrap().is_none());
        assert!(next.take_activation());
    }

    #[test]
    fn simultaneous_starts_have_one_owner_and_crash_releases_it() {
        const CHILD_ROOT: &str = "LIBRE_EFFECTS_INSTANCE_TEST_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let _instance = Instance::in_directory(Path::new(&root)).unwrap().unwrap();
            std::fs::write(Path::new(&root).join("ready"), b"ready").unwrap();
            loop {
                std::thread::park();
            }
        }
        let root = tempfile::tempdir().unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let owners = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| {
                    let barrier = barrier.clone();
                    let root = root.path();
                    scope.spawn(move || {
                        barrier.wait();
                        Instance::in_directory(root).unwrap()
                    })
                })
                .collect();
            workers
                .into_iter()
                .filter_map(|w| w.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(owners.len(), 1);
        drop(owners);
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "single_instance::tests::simultaneous_starts_have_one_owner_and_crash_releases_it",
            ])
            .env(CHILD_ROOT, root.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.path().join("ready").exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                panic!("child exited before acquiring instance lock");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = root.path().join("ready").exists();
        let duplicate = Instance::in_directory(root.path()).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready);
        assert!(duplicate.is_none());
        assert!(Instance::in_directory(root.path()).unwrap().is_some());
    }

    #[test]
    fn lock_io_errors_do_not_silently_allow_a_second_editor() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("editor.lock")).unwrap();
        assert!(Instance::in_directory(root.path()).is_err());
    }
}
