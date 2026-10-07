//! Synthetic native timing used only by focused desktop behavior regressions.
use libre_effects_core::{Command, Editor, OpacityEase, OpacityEdit, OpacityInterpolation};

/// Uniform-time cubic: endpoints 50, controls 50 ± 200. At half time its
/// independently derived raw value is -100 or 200, while paint is 0 or 100.
pub(crate) fn overshoot_editor(high: bool) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Native Opacity draft regression".into(),
            width: 128,
            height: 96,
            fps: 30,
            duration: 120,
        })
        .unwrap();
    editor.execute(Command::AddSolid).unwrap();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x123456,
        })
        .unwrap();
    let speed = if high { 600.0 } else { -600.0 };
    for edit in [
        OpacityEdit::Key {
            frame: 0,
            value: 50.0,
        },
        OpacityEdit::Key {
            frame: 30,
            value: 50.0,
        },
        OpacityEdit::Interpolation {
            frame: 0,
            incoming: OpacityInterpolation::Linear,
            outgoing: OpacityInterpolation::Bezier,
        },
        OpacityEdit::Interpolation {
            frame: 30,
            incoming: OpacityInterpolation::Bezier,
            outgoing: OpacityInterpolation::Linear,
        },
        OpacityEdit::TemporalEase {
            frame: 0,
            incoming: OpacityEase {
                speed: -1e-199,
                influence: 23.0,
            },
            outgoing: OpacityEase {
                speed,
                influence: 100.0 / 3.0,
            },
        },
        OpacityEdit::TemporalEase {
            frame: 30,
            incoming: OpacityEase {
                speed: -speed,
                influence: 100.0 / 3.0,
            },
            outgoing: OpacityEase {
                speed: 2e-199,
                influence: 79.0,
            },
        },
    ] {
        editor
            .execute(Command::SetOpacityTiming { id: 1, edit })
            .unwrap();
    }
    editor.clear_history();
    editor
}
