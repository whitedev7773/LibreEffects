//! User named layouts live in the local profile, never in authored project data.
use crate::view_state::WorkspaceView;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Library {
    pub layouts: BTreeMap<String, WorkspaceView>,
}
impl Library {
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        Self::read(&path).unwrap_or_default()
    }
    fn path() -> Option<std::path::PathBuf> {
        crate::color_edit::Workflow::path().map(|p| p.with_file_name("workspaces.json"))
    }
    pub fn persist(&self) -> Result<(), String> {
        let path = Self::path().ok_or("Local profile directory is unavailable")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::project_io::write_bytes(&path, &bytes)
    }
    pub fn read(path: &Path) -> Result<Self, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 65536 {
            return Err("Workspace storage exceeds 64 KiB".into());
        }
        let mut library: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if library.layouts.len() > 16 {
            return Err("At most 16 saved workspaces are supported".into());
        }
        for (name, layout) in &mut library.layouts {
            Self::name(name)?;
            layout.normalize();
        }
        Ok(library)
    }
    fn name(name: &str) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
            return Err("Use a workspace name of 1–48 characters".into());
        }
        Ok(name.into())
    }
    pub fn save(&mut self, name: &str, mut layout: WorkspaceView) -> Result<String, String> {
        let name = Self::name(name)?;
        if self.layouts.len() >= 16 && !self.layouts.contains_key(&name) {
            return Err("At most 16 saved workspaces are supported".into());
        }
        layout.normalize();
        self.layouts.insert(name.clone(), layout);
        Ok(name)
    }
    pub fn rename(&mut self, old: &str, name: &str) -> Result<String, String> {
        let name = Self::name(name)?;
        if name == old {
            return Ok(name);
        }
        if self.layouts.contains_key(&name) {
            return Err("That workspace name already exists".into());
        }
        let layout = self
            .layouts
            .remove(old)
            .ok_or("Select a saved workspace first")?;
        self.layouts.insert(name.clone(), layout);
        Ok(name)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_layouts_validate_names_normalize_sizes_and_do_not_overwrite_on_rename_conflict() {
        let mut library = Library::default();
        let mut view = WorkspaceView::default();
        view.timeline_left = 9999.0;
        assert_eq!(library.save("  Editing  ", view).unwrap(), "Editing");
        assert_eq!(library.layouts["Editing"].timeline_left, 800.0);
        library.save("Text", WorkspaceView::default()).unwrap();
        assert!(library.rename("Editing", "Text").is_err());
        assert!(library.layouts.contains_key("Editing"));
        assert_eq!(library.rename("Editing", "My layout").unwrap(), "My layout");
        assert!(library.save("\n", WorkspaceView::default()).is_err());
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("workspaces.json");
        std::fs::write(&path, serde_json::to_vec(&library).unwrap()).unwrap();
        assert_eq!(Library::read(&path).unwrap().layouts, library.layouts);
        std::fs::write(&path, vec![b' '; 65537]).unwrap();
        assert!(Library::read(&path).is_err());
    }
}
