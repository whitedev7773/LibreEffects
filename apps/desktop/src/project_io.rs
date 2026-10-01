use std::{
    io::{Read, Write},
    path::Path,
};

use libre_effects_core::Project;

const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_project(path: &Path) -> Result<Project, String> {
    let mut json = String::new();
    std::fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut json)
        .map_err(|error| error.to_string())?;
    if json.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 16 MiB".into());
    }
    let project = Project::from_json(&json)?;
    crate::rendering::validate_images(&project)?;
    Ok(project)
}

pub(crate) fn write_project(path: &Path, json: &str) -> Result<(), String> {
    if json.len() as u64 > MAX_BYTES {
        return Err("Project exceeds 16 MiB".into());
    }
    write_bytes(path, json.as_bytes())
}
pub(crate) fn write_bytes(path: &Path, data: &[u8]) -> Result<(), String> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Complete the temporary file before replacing the destination on the same filesystem.
    let mut temporary =
        tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    temporary
        .write_all(data)
        .map_err(|error| error.to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    temporary.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor};

    #[test]
    fn opening_corrupt_embedded_image_reports_error() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: libre_effects_core::Content::Image { png: "YWJj".into() },
                width: 32.0,
                height: 32.0,
                name: "Broken image".into(),
            })
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.lfe.json");
        write_project(&path, &editor.project().to_json().unwrap()).unwrap();
        assert!(read_project(&path).unwrap_err().contains("Invalid image"));
    }

    #[test]
    fn save_replaces_existing_project_and_open_recovers_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.lfe.json");
        write_project(&path, &Project::default().to_json().unwrap()).unwrap();
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        write_project(&path, &editor.project().to_json().unwrap()).unwrap();
        assert_eq!(&read_project(&path).unwrap(), editor.project());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn oversized_save_leaves_existing_file_intact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.lfe.json");
        let original = Project::default().to_json().unwrap();
        write_project(&path, &original).unwrap();
        assert!(write_project(&path, &" ".repeat(MAX_BYTES as usize + 1)).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}
