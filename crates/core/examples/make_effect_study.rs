//! Editable animated effects on point text, with a transparent composition background.
use libre_effects_core::{
    Command, Content, Editor, EffectEdit, EffectKind, EffectParam, Interpolation, MarkerEdit,
    MarkerTarget, Property,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Pass an output .lfe.json path")?;
    let mut e = Editor::default();
    e.execute(Command::ConfigureComposition {
        name: "Animated Effect Study".into(),
        width: 960,
        height: 540,
        fps: 30,
        duration: 90,
    })?;
    e.execute(Command::SetCompositionBackground(0x141823))?;
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "Libre Effects".into(),
            font_size: 100.0,
        },
        width: 620.0,
        height: 140.0,
        name: "Animated title".into(),
    })?;
    let id = e.selected().unwrap();
    e.execute(Command::SetColor {
        id,
        color: 0xb9a7ff,
    })?;
    for kind in [
        EffectKind::GaussianBlur,
        EffectKind::DropShadow,
        EffectKind::Glow,
    ] {
        e.execute(Command::Effect {
            id,
            edit: EffectEdit::Add(kind),
        })?;
    }
    for (effect, parameter, value) in [
        (1, EffectParam::Radius, 12.0),
        (2, EffectParam::Radius, 8.0),
        (2, EffectParam::Opacity, 75.0),
        (2, EffectParam::OffsetY, 12.0),
        (3, EffectParam::Radius, 5.0),
        (3, EffectParam::Amount, 0.35),
    ] {
        e.execute(Command::Effect {
            id,
            edit: EffectEdit::SetValue {
                effect,
                parameter,
                frame: 0,
                value,
            },
        })?;
    }
    e.execute(Command::Effect {
        id,
        edit: EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::Radius,
            frame: 0,
        },
    })?;
    e.execute(Command::Effect {
        id,
        edit: EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Radius,
            frame: 45,
            value: 0.0,
        },
    })?;
    e.execute(Command::Effect {
        id,
        edit: EffectEdit::Interpolate {
            effect: 1,
            parameter: EffectParam::Radius,
            frame: 0,
            interpolation: Interpolation::Smooth,
        },
    })?;
    e.execute(Command::SetValue {
        id,
        property: Property::PositionY,
        frame: 0,
        value: 290.0,
    })?;
    e.execute(Command::ToggleKeyframe {
        id,
        property: Property::PositionY,
        frame: 0,
    })?;
    e.execute(Command::SetValue {
        id,
        property: Property::PositionY,
        frame: 45,
        value: 260.0,
    })?;
    e.execute(Command::SetInterpolation {
        id,
        property: Property::PositionY,
        frame: 0,
        interpolation: Interpolation::Smooth,
    })?;
    for (target, frame, duration, name, color) in [
        (MarkerTarget::Composition, 0, 45, "Reveal", 0xe7bc6a),
        (MarkerTarget::Composition, 45, 45, "Hold", 0x85bfff),
        (MarkerTarget::Layer(id), 45, 0, "In focus", 0xb9a7ff),
    ] {
        e.execute(Command::Marker {
            target,
            edit: MarkerEdit::Add { frame },
        })?;
        let marker = e
            .project()
            .composition()
            .marker_track(target)
            .unwrap()
            .iter()
            .find(|m| m.frame() == frame)
            .unwrap()
            .id();
        e.execute(Command::Marker {
            target,
            edit: MarkerEdit::Update {
                id: marker,
                frame,
                duration,
                name: name.into(),
                color,
            },
        })?;
    }
    std::fs::write(output, e.project().to_json()?)?;
    Ok(())
}
