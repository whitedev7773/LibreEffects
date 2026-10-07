//! Reproducible LEP example with one binary PNG shared by two animated layers.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use libre_effects_core::*;
use std::{io::Write, path::Path, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args()
        .nth(1)
        .ok_or("Pass a new output .lep path")?;
    let path = Path::new(&destination);
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lep"))
    {
        return Err("Native project examples use the .lep extension".into());
    }
    let mut editor = Editor::default();
    editor.replace_project(Project::from_json(include_str!(
        "../../../examples/gradient-study.lfe.json"
    ))?)?;
    editor.execute(Command::ConfigureComposition {
        name: "Libre Effects Project Study".into(),
        width: 1280,
        height: 720,
        fps: 30,
        duration: 90,
    })?;
    let png: Arc<str> =
        Arc::from(STANDARD.encode(include_bytes!("../../../examples/native-study-image.png")));
    for (name, width, height, x, y) in [
        ("Shared PNG A", 320., 180., 300., 230.),
        ("Shared PNG B", 160., 90., 990., 500.),
    ] {
        editor.execute(Command::AddContent {
            content: Content::Image { png: png.clone() },
            width,
            height,
            name: name.into(),
        })?;
        let id = editor.selected().ok_or("Missing image layer")?;
        for (property, value) in [(Property::PositionX, x), (Property::PositionY, y)] {
            editor.execute(Command::SetValue {
                id,
                property,
                frame: 0,
                value,
            })?;
        }
    }
    editor.execute(Command::EditTrack {
        id: 3,
        property: Property::Rotation.into(),
        edit: TrackEdit::ToggleAnimation { frame: 0 },
    })?;
    editor.execute(Command::EditTrack {
        id: 3,
        property: Property::Rotation.into(),
        edit: TrackEdit::Value {
            frame: 60,
            value: 30.,
        },
    })?;
    let view = br#"{"version":1,"compositions":{"1":{"frame":30}},"workspace":{"snapping":true}}"#;
    let bytes = project_file::encode(editor.project(), Some(view))?;
    let loaded = project_file::decode(&bytes)?;
    assert_eq!(&loaded.project, editor.project());
    assert_eq!(loaded.view, Some(view.as_slice()));
    // Examples never replace an existing project. App saves additionally use atomic replacement.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    println!("Wrote {} bytes to {}", bytes.len(), path.display());
    Ok(())
}
