//! Regenerate the bundled curve/parenting demo using the same commands as the UI.
use libre_effects_core::{Bezier, Command, Editor, Interpolation, Project, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut editor = Editor::default();
    editor.replace_project(Project::from_json(include_str!(
        "../../../examples/motion-study.lfe.json"
    ))?)?;
    editor.execute(Command::ConfigureComposition {
        name: "Curve & Parent Study".into(),
        width: 1280,
        height: 720,
        fps: 30,
        duration: 180,
    })?;
    editor.execute(Command::RenameLayer {
        id: 1,
        name: "Violet — animated parent".into(),
    })?;
    editor.execute(Command::RenameLayer {
        id: 2,
        name: "Apricot — child + local rotation".into(),
    })?;
    editor.execute(Command::SetValue {
        id: 2,
        property: Property::PositionX,
        frame: 0,
        value: 630.0,
    })?;
    editor.execute(Command::SetParent {
        id: 2,
        parent: Some(1),
        frame: 0,
    })?;
    for frame in [0, 60, 120] {
        editor.execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame,
            interpolation: Interpolation::Bezier(Bezier {
                x1: 0.22,
                y1: 0.0,
                x2: 0.7,
                y2: 1.25,
            }),
        })?;
    }
    std::fs::write(output, editor.project().to_json()?)?;
    Ok(())
}
