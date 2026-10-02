//! Each process owns a locked recovery slot. Abandoned slots are claimed before reading.
use crate::project_io::{read_project, write_bytes, write_project};
use libre_effects_core::Project;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub(crate) struct Candidate {
    pub project: Project,
    pub label: String,
    slot: Slot,
}

struct Slot {
    _lock: File,
    path: PathBuf,
}

impl Slot {
    fn previous(&self) -> PathBuf {
        self.path.with_extension("previous")
    }
    fn clear(&self) -> Result<(), String> {
        for path in [&self.path, &self.previous()] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
    fn write(&self, project: &Project) -> Result<(), String> {
        let json = project.to_json()?;
        // Do not replace the last good checkpoint with an invalid/oversized snapshot.
        crate::project_io::validate_project_size(&json)?;
        if self.path.is_file() {
            let previous = std::fs::read(&self.path).map_err(|e| e.to_string())?;
            write_bytes(&self.previous(), &previous)?;
        }
        write_project(&self.path, &json)
    }
}

pub(crate) struct Session {
    slot: Slot,
    pub generation: u64,
    enabled: bool,
}

impl Session {
    pub fn start() -> Result<(Self, Vec<Candidate>, Vec<String>), String> {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("LibreEffects");
        let (session, candidates, mut warnings) = Self::in_directory(&root.join("recovery"))?;
        let legacy = root.join("recovery.lfe.json");
        if legacy.is_file() {
            warnings.push(format!(
                "Legacy recovery preserved at {}. Use File > Open to restore it.",
                legacy.display()
            ));
        }
        Ok((session, candidates, warnings))
    }
    fn in_directory(root: &Path) -> Result<(Self, Vec<Candidate>, Vec<String>), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let temporary = tempfile::Builder::new()
            .prefix("session-")
            .suffix(".lock")
            .tempfile_in(root)
            .map_err(|e| e.to_string())?;
        temporary.as_file().try_lock().map_err(|e| e.to_string())?;
        let (lock, path) = temporary.keep().map_err(|e| e.to_string())?;
        let session = Self {
            slot: Slot {
                _lock: lock,
                path: path.with_extension("json"),
            },
            generation: 0,
            enabled: true,
        };
        let mut paths: Vec<_> = std::fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| {
                p.extension().is_some_and(|s| s == "json")
                    && p.file_name()
                        .is_some_and(|s| s.to_string_lossy().starts_with("session-"))
            })
            .collect();
        paths.sort_by_key(|p| {
            std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok())
        });
        let (mut candidates, mut warnings) = (Vec::new(), Vec::new());
        for path in paths {
            if path == session.slot.path {
                continue;
            }
            let result = (|| -> Result<Option<Candidate>, String> {
                let lock = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(path.with_extension("lock"))
                    .map_err(|e| e.to_string())?;
                match lock.try_lock() {
                    Ok(()) => {}
                    Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
                    Err(e) => return Err(e.to_string()),
                }
                // Another process may have restored and removed it while we opened its lock.
                if !path.is_file() {
                    return Ok(None);
                }
                let slot = Slot {
                    _lock: lock,
                    path: path.clone(),
                };
                let (project, fallback) = match read_project(&path) {
                    Ok(project) => (project, false),
                    Err(error) => (read_project(&slot.previous()).map_err(|_| error)?, true),
                };
                let label = format!(
                    "{} — {}{}",
                    project.composition().name(),
                    path.file_stem().unwrap_or_default().to_string_lossy(),
                    if fallback {
                        " (previous checkpoint)"
                    } else {
                        ""
                    }
                );
                Ok(Some(Candidate {
                    project,
                    label,
                    slot,
                }))
            })();
            match result {
                Ok(Some(candidate)) => candidates.push(candidate),
                Ok(None) => {}
                Err(error) => {
                    warnings.push(format!("Recovery kept at {}: {error}", path.display()))
                }
            }
        }
        Ok((session, candidates, warnings))
    }
    pub fn checkpoint(&self, generation: u64, project: Option<&Project>) -> Result<(), String> {
        if !self.enabled || generation != self.generation {
            return Ok(());
        }
        match project {
            Some(project) => self.slot.write(project),
            None => self.slot.clear(),
        }
    }
    /// Final replacement snapshot: later autosave tasks must not erase it.
    pub fn preserve_for_replacement(&mut self, project: &Project) -> Result<(), String> {
        self.slot.write(project)?;
        self.generation = self.generation.wrapping_add(1);
        self.enabled = false;
        Ok(())
    }
    pub fn reset(&mut self, close: bool) -> Result<(), String> {
        self.generation = self.generation.wrapping_add(1);
        self.enabled = !close;
        self.slot.clear()
    }
    pub fn restore(&mut self, candidate: &Candidate) -> Result<(), String> {
        // Copy successfully to our own locked slot before consuming the abandoned one.
        self.slot.write(&candidate.project)?;
        candidate.slot.clear()?;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}
impl Candidate {
    pub fn discard(&self) -> Result<(), String> {
        self.slot.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor};
    #[test]
    fn replacement_preserves_latest_edits_against_late_autosave() {
        let root = tempfile::tempdir().unwrap();
        let (mut session, _, _) = Session::in_directory(root.path()).unwrap();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        session.preserve_for_replacement(editor.project()).unwrap();
        session.checkpoint(0, None).unwrap();
        session.checkpoint(session.generation, None).unwrap();
        drop(session);
        let (_, candidates, _) = Session::in_directory(root.path()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(&candidates[0].project, editor.project());
    }
    #[test]
    fn live_sessions_are_isolated_and_abandoned_slots_are_claimed_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut a, _, _) = Session::in_directory(dir.path()).unwrap();
        a.checkpoint(0, Some(&Project::default())).unwrap();
        let (b, found, _) = Session::in_directory(dir.path()).unwrap();
        assert!(found.is_empty());
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        b.checkpoint(0, Some(e.project())).unwrap();
        a.reset(true).unwrap();
        assert!(b.slot.path.is_file());
        drop(b);
        let (mut c, candidates, _) = Session::in_directory(dir.path()).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(Session::in_directory(dir.path()).unwrap().1.is_empty());
        c.restore(&candidates[0]).unwrap();
        assert_eq!(&read_project(&c.slot.path).unwrap(), e.project());
        assert!(!candidates[0].slot.path.exists());
    }
    #[test]
    fn stale_autosave_cannot_recreate_a_closed_or_replaced_document() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, _, _) = Session::in_directory(dir.path()).unwrap();
        s.reset(false).unwrap();
        s.checkpoint(0, Some(&Project::default())).unwrap();
        assert!(!s.slot.path.exists());
        s.checkpoint(s.generation, Some(&Project::default()))
            .unwrap();
        s.reset(true).unwrap();
        s.checkpoint(s.generation, Some(&Project::default()))
            .unwrap();
        assert!(!s.slot.path.exists());
    }
    #[test]
    fn corrupt_latest_uses_previous_and_discard_does_not_remove_other_backups() {
        let dir = tempfile::tempdir().unwrap();
        let (s, _, _) = Session::in_directory(dir.path()).unwrap();
        s.checkpoint(0, Some(&Project::default())).unwrap();
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        s.checkpoint(0, Some(e.project())).unwrap();
        std::fs::write(&s.slot.path, "broken").unwrap();
        drop(s);
        let (other, found, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(found[0].project, Project::default());
        assert!(found[0].label.contains("previous"));
        other.checkpoint(0, Some(e.project())).unwrap();
        found[0].discard().unwrap();
        assert_eq!(&read_project(&other.slot.path).unwrap(), e.project());
    }
}
