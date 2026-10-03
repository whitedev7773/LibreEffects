//! Reproducible fixture for Shape Contents gradient editing and rendering.
use libre_effects_core::{
    Command, Content, ContentsEdit, ContentsKind, ContentsParam, Editor, GradientParam,
    PaintComposite, Property, ShapeGradient, TrackEdit,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut editor = Editor::default();
    editor.execute(Command::ConfigureComposition {
        name: "Gradient Editing Study".into(),
        width: 1280,
        height: 720,
        fps: 30,
        duration: 90,
    })?;
    editor.execute(Command::AddContent {
        content: Content::Shape(Default::default()),
        width: 960.,
        height: 480.,
        name: "Select Gradient Fill 5 in Contents".into(),
    })?;
    let id = editor.selected().ok_or("Missing shape")?;
    editor.execute(Command::Contents {
        id,
        edit: ContentsEdit::Promote,
    })?;
    editor.execute(Command::Contents {
        id,
        edit: ContentsEdit::Add {
            parent: 1,
            kind: ContentsKind::GradientFill {
                gradient: ShapeGradient::default(),
                even_odd: false,
            },
        },
    })?;
    editor.execute(Command::Contents {
        id,
        edit: ContentsEdit::Composite {
            item: 5,
            mode: PaintComposite::AbovePrevious,
        },
    })?;
    for (parameter, value) in [
        (GradientParam::StartX, 0.),
        (GradientParam::StartY, 240.),
        (GradientParam::EndX, 960.),
        (GradientParam::EndY, 240.),
        (GradientParam::Red(1), 40.),
        (GradientParam::Green(1), 85.),
        (GradientParam::Blue(1), 240.),
        (GradientParam::Red(2), 255.),
        (GradientParam::Green(2), 130.),
        (GradientParam::Blue(2), 170.),
    ] {
        editor.execute(Command::Contents {
            id,
            edit: ContentsEdit::Track {
                item: 5,
                parameter: ContentsParam::Gradient(parameter),
                edit: TrackEdit::Value { frame: 0, value },
            },
        })?;
    }
    for (property, value) in [(Property::PositionX, 640.), (Property::PositionY, 360.)] {
        editor.execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value,
        })?;
    }
    std::fs::write(output, editor.project().to_json()?)?;
    Ok(())
}
