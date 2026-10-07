//! Local history storage only. Never inspect any path contained in the history.
use libre_effects_editor_model::recent_projects::{MAX_STORAGE_BYTES, RecentProjects};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) fn path() -> Option<PathBuf> {
    // Keep the same profile location as the existing recent-color settings.
    crate::color_edit::Workflow::path().map(|p| p.with_file_name("recent-projects.json"))
}
pub(crate) fn load(path: &Path) -> Result<RecentProjects, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error.to_string()),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Recent-project history is not a regular file".into());
    }
    // Bound the read itself, not just metadata which can change before reading.
    let mut bytes = Vec::new();
    file.take(MAX_STORAGE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    RecentProjects::from_json(&bytes)
}
pub(crate) fn save(path: &Path, history: &RecentProjects) -> Result<(), String> {
    let bytes = history.to_json()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::project_io::write_bytes(path, &bytes)
}

/// One writer shared by routine background writes and the final close drain.
/// Successful newer snapshots supersede queued older tasks, even if those tasks
/// had not acquired the lock before close. The caller holds the mutex for IO.
#[derive(Default)]
pub(crate) struct Writer {
    persisted_revision: u64,
}
impl Writer {
    pub(crate) fn write(&mut self, path: &Path, history: &RecentProjects) -> Result<(), String> {
        if history.revision() <= self.persisted_revision {
            return Ok(());
        }
        save(path, history)?;
        self.persisted_revision = history.revision();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn final_clear_supersedes_an_older_queued_snapshot() {
        let root = tempfile::tempdir().unwrap();
        let storage = root.path().join("recent-projects.json");
        let mut history = RecentProjects::default();
        history.remember(&root.path().join("one.lep"));
        let older = history.clone();
        history.clear(history.revision());
        let mut writer = Writer::default();
        writer.write(&storage, &history).unwrap();
        writer.write(&storage, &older).unwrap();
        assert!(load(&storage).unwrap().paths().is_empty());
    }
    #[test]
    fn persistence_is_bounded_and_clear_never_touches_project_files() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("kept.lep");
        std::fs::write(&project, b"unchanged").unwrap();
        let storage = root.path().join("settings/recent-projects.json");
        let mut history = RecentProjects::default();
        assert!(history.remember(&project));
        save(&storage, &history).unwrap();
        assert_eq!(load(&storage).unwrap().paths(), history.paths());
        assert!(history.clear(history.revision()));
        save(&storage, &history).unwrap();
        assert!(load(&storage).unwrap().paths().is_empty());
        assert_eq!(std::fs::read(&project).unwrap(), b"unchanged");
        std::fs::write(&storage, vec![b' '; MAX_STORAGE_BYTES + 1]).unwrap();
        assert!(load(&storage).is_err());
        assert_eq!(
            std::fs::read_dir(storage.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
}
