//! Bounded, local project history. No filesystem access, scanning or media work.
use std::path::{Component, Path, PathBuf};

pub const MAX_ENTRIES: usize = 10;
pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_STORAGE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentProjects {
    paths: Vec<PathBuf>,
    revision: u64,
}

/// Normalize separators and `.` without resolving aliases or changing symlink
/// semantics. Parents are rejected: the caller supplies its successful absolute
/// project path. Loading history must never consult the current directory/disk.
fn normalized(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    if !path.is_absolute()
        || text.len() > MAX_PATH_BYTES
        || text.chars().any(char::is_control)
        || !path.extension()?.eq_ignore_ascii_case("lep")
        || path.components().any(|p| p == Component::ParentDir)
    {
        return None;
    }
    Some(path.components().collect())
}

impl RecentProjects {
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn contains(&self, path: &Path) -> bool {
        normalized(path).is_some_and(|path| self.paths.contains(&path))
    }
    /// Call only after a successful native Open or chooser-based Save.
    pub fn remember(&mut self, path: &Path) -> bool {
        let Some(path) = normalized(path) else {
            return false;
        };
        if self.paths.first() == Some(&path) {
            return false;
        }
        self.paths.retain(|entry| entry != &path);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_ENTRIES);
        self.revision = self.revision.wrapping_add(1);
        true
    }
    /// A receipt prevents an old Clear click from erasing newly added history.
    pub fn clear(&mut self, expected_revision: u64) -> bool {
        if expected_revision != self.revision || self.paths.is_empty() {
            return false;
        }
        self.paths.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_STORAGE_BYTES {
            return Err("Recent-project history exceeds its size limit".into());
        }
        let paths: Vec<String> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if paths.len() > MAX_ENTRIES {
            return Err("Recent-project history has too many entries".into());
        }
        let mut history = Self::default();
        // First occurrence wins, preserving persisted most-recent-first order.
        for path in paths {
            if let Some(path) = normalized(Path::new(&path))
                && !history.paths.contains(&path)
            {
                history.paths.push(path);
            }
        }
        Ok(history)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        let bytes = serde_json::to_vec(&self.paths).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_STORAGE_BYTES {
            return Err("Recent-project history exceeds its size limit".into());
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\projects")
        } else {
            PathBuf::from("/projects")
        }
    }
    fn path(name: &str) -> PathBuf {
        root().join(name)
    }

    #[test]
    fn remembers_only_bounded_absolute_native_paths() {
        let mut history = RecentProjects::default();
        for p in [
            PathBuf::from("relative.lep"),
            path("old.lfe.json"),
            path("bad\nname.lep"),
            path("../other.lep"),
            path(&format!("{}.lep", "x".repeat(MAX_PATH_BYTES))),
        ] {
            assert!(!history.remember(&p), "{p:?}");
        }
        assert!(history.paths().is_empty());
        assert_eq!(history.revision(), 0);
        assert!(history.remember(&path("한글 project.lep")));
        assert!(history.remember(&path("CAPS.LEP")));
    }
    #[test]
    fn duplicates_promote_once_and_normalize_dot_components() {
        let mut history = RecentProjects::default();
        history.remember(&path("first.lep"));
        history.remember(&path("second.lep"));
        assert!(history.remember(&root().join(".").join("first.lep")));
        assert_eq!(history.paths(), [path("first.lep"), path("second.lep")]);
        let revision = history.revision();
        assert!(!history.remember(&path("first.lep")));
        assert_eq!(history.revision(), revision);
    }
    #[test]
    fn keeps_ten_most_recent_in_stable_order() {
        let mut history = RecentProjects::default();
        for i in 0..15 {
            history.remember(&path(&format!("{i}.lep")));
        }
        assert_eq!(
            history.paths(),
            (5..15)
                .rev()
                .map(|i| path(&format!("{i}.lep")))
                .collect::<Vec<_>>()
        );
        assert!(!history.contains(&path("4.lep")));
    }
    #[test]
    fn serialization_round_trip_retains_order_without_startup_changes() {
        let mut history = RecentProjects::default();
        history.remember(&path("a.lep"));
        history.remember(&path("b.lep"));
        let loaded = RecentProjects::from_json(&history.to_json().unwrap()).unwrap();
        assert_eq!(loaded.paths(), history.paths());
        assert_eq!(loaded.revision(), 0);
    }
    #[test]
    fn loading_preserves_first_duplicates_and_filters_unsafe_entries() {
        let bytes = serde_json::to_vec(&[
            path("a.lep"),
            path("b.lep"),
            root().join(".").join("a.lep"),
            PathBuf::from("relative.lep"),
            path("c.json"),
            path("../parent.lep"),
            path("\0bad.lep"),
        ])
        .unwrap();
        let loaded = RecentProjects::from_json(&bytes).unwrap();
        assert_eq!(loaded.paths(), [path("a.lep"), path("b.lep")]);
    }
    #[test]
    fn malformed_and_oversized_storage_is_rejected() {
        for bytes in [
            b"{broken".to_vec(),
            b"[1]".to_vec(),
            b"null".to_vec(),
            vec![b' '; MAX_STORAGE_BYTES + 1],
            serde_json::to_vec(&vec![path("a.lep"); MAX_ENTRIES + 1]).unwrap(),
        ] {
            assert!(RecentProjects::from_json(&bytes).is_err());
        }
    }
    #[test]
    fn missing_paths_are_retained_without_filesystem_queries() {
        let mut history = RecentProjects::default();
        let missing = path("nonexistent/nested/offline.lep");
        assert!(history.remember(&missing));
        let loaded = RecentProjects::from_json(&history.to_json().unwrap()).unwrap();
        assert!(loaded.contains(&missing));
    }
    #[test]
    fn clear_is_receipted_and_serializes_empty_history() {
        let mut history = RecentProjects::default();
        history.remember(&path("a.lep"));
        let old = history.revision();
        history.remember(&path("b.lep"));
        assert!(!history.clear(old));
        assert_eq!(history.paths().len(), 2);
        assert!(history.clear(history.revision()));
        assert!(history.paths().is_empty());
        assert_eq!(history.to_json().unwrap(), b"[]");
        assert!(!history.clear(history.revision()));
    }
    #[test]
    fn distinct_paths_with_same_filename_are_not_conflated() {
        let mut history = RecentProjects::default();
        history.remember(&path("one/project.lep"));
        history.remember(&path("two/project.lep"));
        assert_eq!(history.paths().len(), 2);
    }
}
