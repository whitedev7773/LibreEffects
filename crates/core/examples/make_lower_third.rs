//! Editable five-second title overlay; export with the ProRes alpha preset.
use libre_effects_core::{Command, Content, Editor, Interpolation, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Lower Third · Transparent".into(),
        width: 1920,
        height: 1080,
        fps: 30,
        duration: 150,
    })?;
    for (name, content, width, height, x, y, color, opacity) in [
        (
            "Title plate",
            Content::Rectangle,
            820.0,
            180.0,
            490.0,
            895.0,
            0x151b29,
            92.0,
        ),
        (
            "Accent",
            Content::Rectangle,
            8.0,
            180.0,
            84.0,
            895.0,
            0xa08cff,
            100.0,
        ),
        (
            "Name",
            Content::Text {
                text: "Your name".into(),
                font_size: 62.0,
            },
            720.0,
            90.0,
            490.0,
            871.0,
            0xffffff,
            100.0,
        ),
        (
            "Role",
            Content::Text {
                text: "Designer / Motion Artist".into(),
                font_size: 28.0,
            },
            720.0,
            44.0,
            490.0,
            942.0,
            0xb7c3d9,
            100.0,
        ),
    ] {
        e.execute(Command::AddContent {
            content,
            width,
            height,
            name: name.into(),
        })?;
        let id = e.selected().unwrap();
        e.execute(Command::SetColor { id, color })?;
        e.execute(Command::SetValue {
            id,
            property: Property::PositionY,
            frame: 0,
            value: y,
        })?;
        for (property, values) in [
            (Property::PositionX, [x - 40.0, x, x, x - 40.0]),
            (Property::Opacity, [0.0, opacity, opacity, 0.0]),
        ] {
            e.execute(Command::ToggleKeyframe {
                id,
                property,
                frame: 0,
            })?;
            for (frame, value) in [0, 18, 126, 149].into_iter().zip(values) {
                e.execute(Command::SetValue {
                    id,
                    property,
                    frame,
                    value,
                })?;
                e.execute(Command::SetInterpolation {
                    id,
                    property,
                    frame,
                    interpolation: Interpolation::Smooth,
                })?;
            }
        }
    }
    std::fs::write(output, e.project().to_json()?)?;
    Ok(())
}
