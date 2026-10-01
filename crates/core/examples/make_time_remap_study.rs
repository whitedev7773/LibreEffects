//! Four instances share a source but have independent playback curves.
use libre_effects_core::{Command, Content, Editor, Property, PropertyPath, TrackEdit};

fn value(
    e: &mut Editor,
    id: u64,
    property: Property,
    frame: u32,
    value: f64,
) -> Result<(), String> {
    e.execute(Command::SetValue {
        id,
        property,
        frame,
        value,
    })
}
fn label(
    e: &mut Editor,
    text: &str,
    position: [f64; 2],
    width: f64,
    font_size: f64,
    color: u32,
) -> Result<(), String> {
    e.execute(Command::AddContent {
        content: Content::Text {
            text: text.into(),
            font_size,
        },
        width,
        height: font_size * 1.5,
        name: text.into(),
    })?;
    let id = e.selected().unwrap();
    e.execute(Command::SetColor { id, color })?;
    e.execute(Command::SetPosition {
        id,
        frame: 0,
        x: position[0],
        y: position[1],
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Shared motion source".into(),
        width: 800,
        height: 320,
        fps: 30,
        duration: 180,
    })?;
    e.execute(Command::AddSolid)?;
    e.execute(Command::SetColor {
        id: 1,
        color: 0x232b40,
    })?;
    e.execute(Command::AddContent {
        content: Content::Solid,
        width: 640.0,
        height: 2.0,
        name: "Motion rail".into(),
    })?;
    e.execute(Command::SetColor {
        id: 2,
        color: 0x536486,
    })?;
    e.execute(Command::AddContent {
        content: Content::Solid,
        width: 92.0,
        height: 92.0,
        name: "Moving square".into(),
    })?;
    let moving = e.selected().unwrap();
    e.execute(Command::SetColor {
        id: moving,
        color: 0x5bd6a2,
    })?;
    for (p, a, b) in [
        (Property::PositionX, 110.0, 690.0),
        (Property::Rotation, 0.0, 360.0),
    ] {
        value(&mut e, moving, p, 0, a)?;
        e.execute(Command::ToggleKeyframe {
            id: moving,
            property: p,
            frame: 0,
        })?;
        value(&mut e, moving, p, 179, b)?;
    }
    label(
        &mut e,
        "SHARED SOURCE",
        [400.0, 55.0],
        680.0,
        28.0,
        0xa9b4ce,
    )?;
    label(
        &mut e,
        "0 seconds                                      6 seconds",
        [400.0, 276.0],
        650.0,
        24.0,
        0xa9b4ce,
    )?;
    e.execute(Command::NewComposition)?;
    e.execute(Command::ConfigureComposition {
        name: "Time Remap Study".into(),
        width: 960,
        height: 540,
        fps: 30,
        duration: 180,
    })?;
    e.execute(Command::SetCompositionBackground(0x111722))?;
    let positions = [
        [250.0, 170.0],
        [710.0, 170.0],
        [250.0, 410.0],
        [710.0, 410.0],
    ];
    let names = ["ORIGINAL", "FAST → SLOW", "HOLD", "REVERSE"];
    for (index, position) in positions.into_iter().enumerate() {
        e.execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 0,
        })?;
        let id = e.selected().unwrap();
        e.execute(Command::RenameLayer {
            id,
            name: names[index].into(),
        })?;
        e.execute(Command::SetPosition {
            id,
            frame: 0,
            x: position[0],
            y: position[1],
        })?;
        for p in [Property::ScaleX, Property::ScaleY] {
            value(&mut e, id, p, 0, 50.0)?;
        }
        if index != 0 {
            e.execute(Command::SetTimeRemap { id, enabled: true })?;
            let keys = match index {
                1 => vec![(0, 0.0), (45, 3.5), (179, 179.0 / 30.0)],
                2 => vec![(0, 0.0), (45, 1.5), (120, 1.5), (179, 179.0 / 30.0)],
                _ => vec![(0, 179.0 / 30.0), (179, 0.0)],
            };
            for (frame, value) in keys {
                e.execute(Command::EditTrack {
                    id,
                    property: PropertyPath::TimeRemap,
                    edit: TrackEdit::Value { frame, value },
                })?;
            }
        }
        label(
            &mut e,
            names[index],
            [position[0], position[1] - 114.0],
            400.0,
            22.0,
            0xe4e9f3,
        )?;
    }
    label(
        &mut e,
        "One source. Four clocks.",
        [480.0, 521.0],
        860.0,
        16.0,
        0x8e9cba,
    )?;
    std::fs::write(output, e.project().to_json()?)?;
    Ok(())
}
