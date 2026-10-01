//! One editor per user, across executable names and build directories.
//! The OS releases the lock on a crash; the lock file must never be unlinked.
use std::{
    fs::{File, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

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
        Self::in_directory(&root).map_err(|error| error.to_string())
    }

    fn in_directory(root: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(root)?;
        let activation = root.join("editor.activate");
        // Read before acquiring: a request arriving just after acquisition must
        // still activate the window when GPUI finishes starting.
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
            Err(TryLockError::WouldBlock) => {
                let token = format!("{}:{:?}", std::process::id(), std::time::SystemTime::now());
                // Serialize requests on a separate short-lived lock. Concurrent
                // atomic file replacements can fail with sharing violations on
                // Windows; the long-lived editor lock must remain untouched.
                let mut signal = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&activation)?;
                signal.lock()?;
                signal.set_len(0)?;
                signal.write_all(token.as_bytes())?;
                Ok(None)
            }
            Err(TryLockError::Error(error)) => Err(error),
        }
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

pub(crate) fn activate_window(window: &gpui::Window) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                GetLastActivePopup, IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
                ShowWindowAsync,
            },
        };
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            let owner = HWND(handle.hwnd.get() as *mut _);
            // SAFETY: GPUI owns this live HWND and calls us on its UI thread.
            // Activate its owned dialog when a file picker is open; activating
            // the disabled owner instead can leave the picker without focus.
            unsafe {
                if IsIconic(owner).as_bool() {
                    let _ = ShowWindowAsync(owner, SW_RESTORE);
                }
                let popup = GetLastActivePopup(owner);
                let target = if !popup.is_invalid() && IsWindowVisible(popup).as_bool() {
                    popup
                } else {
                    owner
                };
                let _ = SetForegroundWindow(target);
            }
            return;
        }
    }
    window.activate_window();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_requests_activation_without_replacing_owner() {
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
