//! A small editable title card exercising text, masks, effects and animation.
use libre_effects_core::{Command, Content, Editor, Effects, Interpolation, Mask, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Content & Motion Study".into(),
        width: 1280,
        height: 720,
        fps: 30,
        duration: 90,
    })?;
    for (name, content, width, height, x, y, color) in [
        (
            "Background",
            Content::Rectangle,
            1280.0,
            720.0,
            640.0,
            360.0,
            0x141823,
        ),
        (
            "Soft violet",
            Content::Rectangle,
            410.0,
            330.0,
            950.0,
            360.0,
            0x7663e8,
        ),
        (
            "Masked accent",
            Content::Rectangle,
            180.0,
            180.0,
            1025.0,
            445.0,
            0xffbf78,
        ),
        (
            "Title",
            Content::Text {
                text: "Libre Effects".into(),
                font_size: 92.0,
            },
            760.0,
            130.0,
            510.0,
            300.0,
            0xffffff,
        ),
        (
            "Subtitle",
            Content::Text {
                text: "Create. Animate. Make it yours.".into(),
                font_size: 30.0,
            },
            760.0,
            60.0,
            510.0,
            420.0,
            0xaab6cc,
        ),
    ] {
        e.execute(Command::AddContent {
            content,
            width,
            height,
            name: name.into(),
        })?;
        let id = e.selected().unwrap();
        e.execute(Command::Batch(vec![
            Command::SetColor { id, color },
            Command::SetValue {
                id,
                property: Property::PositionX,
                frame: 0,
                value: x,
            },
            Command::SetValue {
                id,
                property: Property::PositionY,
                frame: 0,
                value: y,
            },
        ]))?;
    }
    e.execute(Command::ToggleLocked(1))?;
    e.execute(Command::SetEffects {
        id: 2,
        effects: Effects {
            blur: 24.0,
            ..Effects::default()
        },
    })?;
    e.execute(Command::SetMask {
        id: 3,
        mask: Some(Mask {
            x: 35.0,
            y: 35.0,
            width: 110.0,
            height: 110.0,
            inverted: true,
        }),
    })?;
    e.execute(Command::SetValue {
        id: 3,
        property: Property::Rotation,
        frame: 0,
        value: 18.0,
    })?;
    for (id, p, end) in [
        (4, Property::PositionY, 330.0),
        (5, Property::Opacity, 45.0),
    ] {
        e.execute(Command::ToggleKeyframe {
            id,
            property: p,
            frame: 0,
        })?;
        e.execute(Command::SetValue {
            id,
            property: p,
            frame: 60,
            value: end,
        })?;
        e.execute(Command::SetInterpolation {
            id,
            property: p,
            frame: 0,
            interpolation: Interpolation::Smooth,
        })?;
    }
    std::fs::write(output, e.project().to_json()?)?;
    Ok(())
}
