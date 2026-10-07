use libre_effects_core::{Command, Content, Editor, Project, ProjectItem, Property};

fn main() -> Result<(), String> {
    let mut editor = Editor::default();
    editor.replace_project(Project::from_json(include_str!(
        "../../../examples/content-study.lfe.json"
    ))?)?;
    editor.execute(Command::ImportAsset {
        content: Content::Image { png: "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAAP0lEQVR4nO3OMQEAEBQA0d/FqIQe2gmlDgWsfMMbbr4XpbbILHUOcAT0OdbNAAAAAAAAAAAAAAD+B7wOIB2wAe+CUWp8C9hoAAAAAElFTkSuQmCC".into() },
        width: 32.0, height: 32.0, name: "Shared tile.png".into(), folder: None, frame: Some(0),
    })?;
    let tile = editor.selected().unwrap();
    editor.execute(Command::SetPosition {
        id: tile,
        frame: 0,
        x: 1080.0,
        y: 500.0,
    })?;
    for property in [Property::ScaleX, Property::ScaleY] {
        editor.execute(Command::SetValue {
            id: tile,
            property,
            frame: 0,
            value: 400.0,
        })?;
    }
    let asset = *editor
        .project()
        .asset_library()
        .assets()
        .keys()
        .next()
        .ok_or("Sample has no image")?;
    editor.execute(Command::NewProjectFolder {
        name: "Footage".into(),
        parent: None,
    })?;
    let footage = *editor
        .project()
        .asset_library()
        .folders()
        .keys()
        .last()
        .unwrap();
    editor.execute(Command::MoveProjectItem {
        item: ProjectItem::Asset(asset),
        folder: Some(footage),
    })?;
    editor.execute(Command::RenameProjectItem {
        item: ProjectItem::Asset(asset),
        name: "Shared tile.png".into(),
    })?;
    editor.execute(Command::NewProjectFolder {
        name: "Compositions".into(),
        parent: None,
    })?;
    let folder = *editor
        .project()
        .asset_library()
        .folders()
        .keys()
        .last()
        .unwrap();
    editor.execute(Command::MoveProjectItem {
        item: ProjectItem::Composition(1),
        folder: Some(folder),
    })?;
    editor.execute(Command::NewComposition)?;
    editor.execute(Command::ConfigureComposition {
        name: "Shared tile animation".into(),
        width: 256,
        height: 256,
        fps: 30,
        duration: 90,
    })?;
    editor.execute(Command::RenameProjectItem {
        item: ProjectItem::Composition(2),
        name: "Shared tile animation".into(),
    })?;
    editor.execute(Command::AddAssetLayer { asset, frame: 0 })?;
    let id = editor.selected().unwrap();
    for property in [Property::ScaleX, Property::ScaleY] {
        editor.execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value: 400.0,
        })?;
    }
    editor.execute(Command::ToggleKeyframe {
        id,
        property: Property::Rotation,
        frame: 0,
    })?;
    editor.execute(Command::SetValue {
        id,
        property: Property::Rotation,
        frame: 89,
        value: 360.0,
    })?;
    editor.activate_composition(1)?;
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/asset-library-study.lfe.json".into());
    std::fs::write(path, editor.project().to_json()?).map_err(|e| e.to_string())
}
