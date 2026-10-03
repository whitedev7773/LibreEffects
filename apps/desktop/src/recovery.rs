//! Each process owns a locked recovery slot. Abandoned slots are claimed before reading.
use crate::project_io::{
    decode_project, encode_native_project, read_bytes, read_project, write_bytes,
};
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

fn sidecar(path: &Path, extension: &str) -> PathBuf {
    if path.extension().is_some_and(|value| value == "lep") {
        // Native and legacy slots with the same stem own independent locks and
        // backups. Keep old JSON pair names intact for cross-version recovery.
        path.with_extension(format!("lep.{extension}"))
    } else {
        path.with_extension(extension)
    }
}

impl Slot {
    fn previous(&self) -> PathBuf {
        sidecar(&self.path, "previous")
    }
    fn clear(&self) -> Result<(), String> {
        for path in [&self.path, &self.previous()] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(format!(
                        "Cannot remove recovery checkpoint {}: {e}",
                        path.display()
                    ));
                }
            }
        }
        Ok(())
    }
    fn write(&self, project: &Project) -> Result<(), String> {
        self.write_with(project, write_bytes)
    }
    fn write_with(
        &self,
        project: &Project,
        mut write: impl FnMut(&Path, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        // Finish all validation and encoding before touching either generation.
        let bytes = encode_native_project(project, None)?;
        if self.path.is_file() {
            let previous = read_bytes(&self.path)?;
            // A corrupt current file must not displace the last valid backup.
            if decode_project(&previous).is_ok() {
                write(&self.previous(), &previous)?;
            }
        }
        write(&self.path, &bytes)
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
    pub(crate) fn in_directory(root: &Path) -> Result<(Self, Vec<Candidate>, Vec<String>), String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let (lock, path) = loop {
            let temporary = tempfile::Builder::new()
                .prefix("session-")
                .suffix(".lep.lock")
                .tempfile_in(root)
                .map_err(|e| e.to_string())?;
            temporary.as_file().try_lock().map_err(|e| e.to_string())?;
            let project_path = temporary.path().with_extension("");
            if project_path.exists() || sidecar(&project_path, "previous").exists() {
                continue;
            }
            break temporary.keep().map_err(|e| e.to_string())?;
        };
        let session = Self {
            slot: Slot {
                _lock: lock,
                path: path.with_extension(""),
            },
            generation: 0,
            enabled: true,
        };
        let mut paths: Vec<_> = std::fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|s| s.to_string_lossy().starts_with("session-"))
            })
            .filter_map(
                |path| match path.extension().and_then(|extension| extension.to_str()) {
                    Some("json" | "lep") => Some(path),
                    Some("previous") => {
                        let without_previous = path.with_extension("");
                        Some(
                            if without_previous
                                .extension()
                                .is_some_and(|extension| extension == "lep")
                            {
                                without_previous
                            } else {
                                path.with_extension("json")
                            },
                        )
                    }
                    _ => None,
                },
            )
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        paths.sort_by_key(|p| {
            std::cmp::Reverse(
                std::fs::metadata(p)
                    .or_else(|_| std::fs::metadata(sidecar(p, "previous")))
                    .and_then(|m| m.modified())
                    .ok(),
            )
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
                    .open(sidecar(&path, "lock"))
                    .map_err(|e| e.to_string())?;
                match lock.try_lock() {
                    Ok(()) => {}
                    Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
                    Err(e) => return Err(e.to_string()),
                }
                // Another process may have restored and removed it while we opened its lock.
                if !path.is_file() && !sidecar(&path, "previous").is_file() {
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
    pub fn restore(&mut self, candidate: &Candidate) -> Result<Option<String>, String> {
        // Copy successfully to our own locked slot before consuming the abandoned one.
        self.slot.write(&candidate.project)?;
        // The durable owned copy commits the restore. A partial old-slot cleanup
        // must not leave the old document live or allow its late autosave to
        // overwrite the only remaining restored copy.
        self.generation = self.generation.wrapping_add(1);
        Ok(candidate.slot.clear().err().map(|error| {
            format!("Restored project is safely checkpointed, but old recovery cleanup is incomplete: {error}")
        }))
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
    fn legacy_and_native_same_stem_slots_have_independent_locks_and_backups() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("session-shared.json");
        let native = dir.path().join("session-shared.lep");
        let base = Project::default();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        std::fs::write(&legacy, base.to_json().unwrap()).unwrap();
        write_bytes(
            &native,
            &encode_native_project(editor.project(), None).unwrap(),
        )
        .unwrap();
        let lock = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(sidecar(&native, "lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let (_, legacy_candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(legacy_candidates.len(), 1);
        assert_eq!(legacy_candidates[0].slot.path, legacy);
        assert_eq!(legacy_candidates[0].project, base);
        drop(lock);
        let (_, native_candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(native_candidates.len(), 1);
        assert_eq!(native_candidates[0].slot.path, native);
        assert_eq!(&native_candidates[0].project, editor.project());
        assert!(Session::in_directory(dir.path()).unwrap().1.is_empty());
        assert_ne!(
            legacy_candidates[0].slot.previous(),
            native_candidates[0].slot.previous()
        );
    }

    #[test]
    fn previous_fallback_sniffs_both_formats_and_discard_keeps_other_pairs() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("session-shared.json");
        let native = dir.path().join("session-shared.lep");
        let base = Project::default();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        // Deliberately put each supported format behind the other slot's backup
        // name. Pair selection follows slot identity; decoding follows bytes.
        std::fs::write(&legacy, b"broken legacy current").unwrap();
        std::fs::write(&native, b"broken native current").unwrap();
        write_bytes(
            &sidecar(&legacy, "previous"),
            &encode_native_project(editor.project(), None).unwrap(),
        )
        .unwrap();
        std::fs::write(sidecar(&native, "previous"), base.to_json().unwrap()).unwrap();
        let (_, candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(candidates.len(), 2);
        let old = candidates
            .iter()
            .find(|candidate| candidate.slot.path == legacy)
            .unwrap();
        let new = candidates
            .iter()
            .find(|candidate| candidate.slot.path == native)
            .unwrap();
        assert_eq!(&old.project, editor.project());
        assert_eq!(new.project, base);
        assert!(old.label.contains("previous checkpoint"));
        assert!(new.label.contains("previous checkpoint"));
        new.discard().unwrap();
        assert!(legacy.is_file());
        assert!(sidecar(&legacy, "previous").is_file());
        assert!(!native.exists());
        assert!(!sidecar(&native, "previous").exists());
    }

    #[test]
    fn failed_checkpoint_never_rotates_corruption_over_last_good_previous() {
        let dir = tempfile::tempdir().unwrap();
        let (session, _, _) = Session::in_directory(dir.path()).unwrap();
        let base = Project::default();
        session.checkpoint(0, Some(&base)).unwrap();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        session.checkpoint(0, Some(editor.project())).unwrap();
        let last_good = std::fs::read(session.slot.previous()).unwrap();
        std::fs::write(&session.slot.path, b"corrupt current").unwrap();
        let mut writes = Vec::new();
        assert!(
            session
                .slot
                .write_with(editor.project(), |path, _| {
                    writes.push(path.to_path_buf());
                    Err("Simulated current write failure".into())
                })
                .is_err()
        );
        assert_eq!(writes, vec![session.slot.path.clone()]);
        assert_eq!(std::fs::read(session.slot.previous()).unwrap(), last_good);
        assert_eq!(
            std::fs::read(&session.slot.path).unwrap(),
            b"corrupt current"
        );
        drop(session);
        let (_, candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].project, base);
    }

    #[test]
    fn invalid_new_or_oversized_prior_checkpoint_changes_neither_generation() {
        let dir = tempfile::tempdir().unwrap();
        let (session, _, _) = Session::in_directory(dir.path()).unwrap();
        session.checkpoint(0, Some(&Project::default())).unwrap();
        session.checkpoint(0, Some(&Project::default())).unwrap();
        let current = std::fs::read(&session.slot.path).unwrap();
        let previous = std::fs::read(session.slot.previous()).unwrap();
        let mut invalid = Editor::default();
        invalid
            .execute(Command::AddContent {
                content: libre_effects_core::Content::Image { png: "YWJj".into() },
                width: 10.0,
                height: 10.0,
                name: "Broken image".into(),
            })
            .unwrap();
        assert!(
            session
                .slot
                .write_with(invalid.project(), |_, _| panic!(
                    "must validate before writes"
                ))
                .is_err()
        );
        assert_eq!(std::fs::read(&session.slot.path).unwrap(), current);
        assert_eq!(std::fs::read(session.slot.previous()).unwrap(), previous);
        let file = File::options()
            .write(true)
            .open(&session.slot.path)
            .unwrap();
        file.set_len(libre_effects_core::project_file::MAX_FILE_BYTES as u64 + 1)
            .unwrap();
        assert!(
            session
                .slot
                .write_with(&Project::default(), |_, _| panic!(
                    "oversized prior must not rotate"
                ))
                .is_err()
        );
        assert_eq!(std::fs::read(session.slot.previous()).unwrap(), previous);
    }

    #[test]
    fn failed_restore_and_unreadable_discovery_preserve_abandoned_data() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("session-old.json");
        let bytes = Project::default().to_json().unwrap();
        std::fs::write(&legacy, &bytes).unwrap();
        let (mut session, candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        std::fs::create_dir(&session.slot.path).unwrap();
        std::fs::write(session.slot.path.join("sentinel"), b"keep me").unwrap();
        assert!(session.restore(&candidates[0]).is_err());
        assert_eq!(session.generation, 0);
        assert_eq!(std::fs::read(&legacy).unwrap(), bytes.as_bytes());
        assert_eq!(
            std::fs::read(session.slot.path.join("sentinel")).unwrap(),
            b"keep me"
        );
        let broken = dir.path().join("session-broken.lep");
        std::fs::write(&broken, b"broken current").unwrap();
        std::fs::write(sidecar(&broken, "previous"), b"broken previous").unwrap();
        let (_, found, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(found.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("session-broken.lep"));
        assert_eq!(std::fs::read(&broken).unwrap(), b"broken current");
        assert_eq!(
            std::fs::read(sidecar(&broken, "previous")).unwrap(),
            b"broken previous"
        );
    }

    #[test]
    fn cleanup_failure_commits_restore_and_rejects_late_old_autosaves() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("session-restore.json");
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        std::fs::write(&source, editor.project().to_json().unwrap()).unwrap();
        let (mut session, candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        let previous = candidates[0].slot.previous();
        std::fs::create_dir(&previous).unwrap();
        std::fs::write(previous.join("sentinel"), b"keep me").unwrap();
        let stale_generation = session.generation;
        let warning = session.restore(&candidates[0]).unwrap().unwrap();
        assert!(warning.contains("cleanup is incomplete"));
        assert!(warning.contains("session-restore.previous"));
        assert!(!source.exists());
        assert_eq!(
            std::fs::read(previous.join("sentinel")).unwrap(),
            b"keep me"
        );
        assert_ne!(session.generation, stale_generation);
        session.checkpoint(stale_generation, None).unwrap();
        session
            .checkpoint(stale_generation, Some(&Project::default()))
            .unwrap();
        assert_eq!(&read_project(&session.slot.path).unwrap(), editor.project());
    }

    #[test]
    fn previous_only_slots_are_discovered_once_under_their_original_locks() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("session-legacy.json");
        let native = dir.path().join("session-native.lep");
        let project = Project::default();
        std::fs::write(sidecar(&legacy, "previous"), project.to_json().unwrap()).unwrap();
        write_bytes(
            &sidecar(&native, "previous"),
            &encode_native_project(&project, None).unwrap(),
        )
        .unwrap();
        let lock = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(sidecar(&native, "lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let (_, legacy_candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(legacy_candidates.len(), 1);
        assert_eq!(legacy_candidates[0].slot.path, legacy);
        drop(lock);
        let (_, native_candidates, warnings) = Session::in_directory(dir.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(native_candidates.len(), 1);
        assert_eq!(native_candidates[0].slot.path, native);
        assert!(Session::in_directory(dir.path()).unwrap().1.is_empty());
        native_candidates[0].discard().unwrap();
        assert!(!sidecar(&native, "previous").exists());
        assert!(sidecar(&legacy, "previous").is_file());
    }

    #[test]
    fn uncommitted_canvas_text_roundtrips_recovery_without_mutating_document() {
        let root = tempfile::tempdir().unwrap();
        let (mut recovery, _, _) = Session::in_directory(root.path()).unwrap();
        let base = Project::default();
        let mut text = crate::text_edit::Session::new(&base, 0, 0, None, [10.0, 20.0]).unwrap();
        text.buffer
            .replace(None, "복구할\nText draft", false, None)
            .unwrap();
        let draft = text.project().unwrap();
        recovery
            .checkpoint(recovery.generation, Some(&draft))
            .unwrap();
        assert_eq!(read_project(&recovery.slot.path).unwrap(), draft);
        assert!(base.composition().layers().is_empty());
        let mut editor = Editor::default();
        editor.execute(text.command()).unwrap();
        recovery.preserve_for_replacement(editor.project()).unwrap();
        recovery.checkpoint(0, Some(&base)).unwrap();
        drop(recovery);
        let (_, candidates, warnings) = Session::in_directory(root.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].project, draft);
    }
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
        assert_eq!(a.slot.path.extension().unwrap(), "lep");
        assert!(
            std::fs::read(&a.slot.path)
                .unwrap()
                .starts_with(libre_effects_core::project_file::MAGIC)
        );
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
