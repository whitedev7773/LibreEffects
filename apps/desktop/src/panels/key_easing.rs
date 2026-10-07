//! Shared scalar-key easing for the timeline and Graph Editor.
use gpui::KeyDownEvent;
use libre_effects_core::{
    AnimatedProperty, Command, KeyRef, Project, TemporalHandle, TemporalMode,
};

pub(super) fn shortcut(event: &KeyDownEvent) -> Option<(bool, bool)> {
    let m = event.keystroke.modifiers;
    if event.keystroke.key != "f9" || event.is_held || m.alt || m.platform {
        return None;
    }
    match (m.control, m.shift) {
        (false, false) => Some((true, true)),
        (false, true) => Some((true, false)),
        (true, true) => Some((false, true)),
        (true, false) => None,
    }
}

/// Validate the whole selection before creating a single undo transaction.
/// Native timing, opaque Path poses and Hold-only Source Text are rejected explicitly.
pub(super) fn selected(
    project: &Project,
    keys: &[KeyRef],
    incoming: bool,
    outgoing: bool,
) -> Result<Option<Command>, String> {
    let mut commands = Vec::new();
    for key in keys {
        if matches!(
            key.property,
            libre_effects_core::PropertyPath::Path(_)
                | libre_effects_core::PropertyPath::SourceText
        ) {
            return Err(
                "Easy Ease supports scalar keys; deselect native timing, path and Source Text keys"
                    .into(),
            );
        }
        let layer = project
            .composition()
            .layer(key.id)
            .ok_or("Selected layer no longer exists")?;
        if layer.locked() {
            return Err("Unlock the selected layers before easing keyframes".into());
        }
        let track = layer.track(key.property).ok_or(
            "Easy Ease supports scalar keys; deselect native timing, path and Source Text keys",
        )?;
        if !track.keys().contains_key(&key.frame) {
            return Err("Selected keyframe no longer exists".into());
        }
        if let Command::Batch(edits) = ease(track, &[*key], incoming, outgoing) {
            commands.extend(edits);
        }
    }
    Ok((!commands.is_empty()).then(|| Command::Batch(commands)))
}

pub(super) fn ease(
    track: &AnimatedProperty,
    keys: &[KeyRef],
    incoming: bool,
    outgoing: bool,
) -> Command {
    let mut commands = Vec::new();
    for &KeyRef {
        id,
        property,
        frame,
    } in keys
    {
        let sides = [
            (
                true,
                incoming && track.keys().range(..frame).next_back().is_some(),
            ),
            (
                false,
                outgoing && track.keys().range(frame + 1..).next().is_some(),
            ),
        ];
        if sides.iter().any(|(_, yes)| *yes) {
            commands.push(Command::SetTemporalMode {
                id,
                property,
                frame,
                mode: TemporalMode::Independent,
            });
            for (incoming, _) in sides.into_iter().filter(|(_, yes)| *yes) {
                commands.push(Command::SetTemporalHandle {
                    id,
                    property,
                    frame,
                    incoming,
                    handle: TemporalHandle {
                        slope: 0.0,
                        influence: 1.0 / 3.0,
                    },
                });
            }
        }
    }
    Command::Batch(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Editor, Property, TrackEdit};

    fn event(key: &str) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke::parse(key).unwrap(),
            is_held: false,
        }
    }
    fn scene() -> (Editor, Vec<KeyRef>) {
        let mut editor = Editor::default();
        let mut selected = Vec::new();
        for id in 1..=2 {
            editor.execute(Command::AddRectangle).unwrap();
            for property in [Property::PositionX, Property::PositionY] {
                for (frame, value) in [(10, 600.0), (30, 900.0), (60, 720.0)] {
                    for edit in [
                        TrackEdit::ToggleKey { frame },
                        TrackEdit::Value { frame, value },
                    ] {
                        editor
                            .execute(Command::EditTrack {
                                id,
                                property: property.into(),
                                edit,
                            })
                            .unwrap();
                    }
                }
                selected.push(KeyRef {
                    id,
                    property: property.into(),
                    frame: 30,
                });
            }
        }
        (editor, selected)
    }
    #[test]
    fn shortcuts_preserve_requested_side_and_ignore_other_chords_or_repeat() {
        for (key, expected) in [
            ("f9", Some((true, true))),
            ("shift-f9", Some((true, false))),
            ("ctrl-shift-f9", Some((false, true))),
            ("ctrl-f9", None),
            ("alt-f9", None),
            ("alt-shift-f9", None),
            ("cmd-f9", None),
            ("f8", None),
        ] {
            let mut e = event(key);
            assert_eq!(shortcut(&e), expected, "{key}");
            e.is_held = true;
            assert_eq!(shortcut(&e), None);
        }
    }
    #[test]
    fn multiple_layers_and_channels_ease_atomically_and_preserve_opposite_curves() {
        for chord in ["f9", "shift-f9", "ctrl-shift-f9"] {
            let (mut e, keys) = scene();
            let before = e.project().clone();
            let (incoming, outgoing) = shortcut(&event(chord)).unwrap();
            e.execute(
                selected(e.project(), &keys, incoming, outgoing)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            for k in &keys {
                let old = before
                    .composition()
                    .layer(k.id)
                    .unwrap()
                    .track(k.property)
                    .unwrap();
                let track = e
                    .project()
                    .composition()
                    .layer(k.id)
                    .unwrap()
                    .track(k.property)
                    .unwrap();
                assert_eq!(
                    track
                        .keys()
                        .iter()
                        .map(|(f, k)| (*f, k.value))
                        .collect::<Vec<_>>(),
                    old.keys()
                        .iter()
                        .map(|(f, k)| (*f, k.value))
                        .collect::<Vec<_>>()
                );
                for (side, edited) in [(true, incoming), (false, outgoing)] {
                    let handle = track.temporal_handle(30, side).unwrap();
                    if edited {
                        assert_eq!(handle.slope, 0.0);
                        assert_eq!(handle.influence, 1.0 / 3.0);
                    } else {
                        assert_eq!(handle, old.temporal_handle(30, side).unwrap());
                        for f in if side { 10..30 } else { 30..60 } {
                            assert!((track.sample(f as f64) - old.sample(f as f64)).abs() < 1e-6);
                        }
                    }
                }
            }
            let after = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            assert_eq!(&after, e.project());
            e.undo();
            assert_eq!(e.project(), &before);
            e.redo();
            assert_eq!(e.project(), &after);
            let r = crate::rendering::Renderer::new();
            for frame in [10, 20, 30, 45, 60] {
                assert_eq!(
                    r.render(&after, frame, 384).unwrap(),
                    r.render_output(&after, frame, 384, 216).unwrap()
                );
            }
        }
    }
    #[test]
    fn source_text_rejects_every_f9_direction_and_mixed_selection_atomically() {
        let (mut editor, numeric) = scene();
        editor
            .execute(Command::AddContent {
                content: libre_effects_core::Content::Text {
                    text: "First".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Title".into(),
            })
            .unwrap();
        let source = KeyRef {
            id: 3,
            property: libre_effects_core::PropertyPath::SourceText,
            frame: 30,
        };
        editor
            .execute(Command::EditTrack {
                id: source.id,
                property: source.property,
                edit: TrackEdit::ToggleAnimation { frame: 10 },
            })
            .unwrap();
        for frame in [30, 60] {
            editor
                .execute(Command::EditTrack {
                    id: source.id,
                    property: source.property,
                    edit: TrackEdit::ToggleKey { frame },
                })
                .unwrap();
        }
        editor
            .execute(Command::RenameLayer {
                id: 3,
                name: "Redo witness".into(),
            })
            .unwrap();
        editor.undo();
        let before = editor.project().clone();
        for chord in ["f9", "shift-f9", "ctrl-shift-f9"] {
            let (incoming, outgoing) = shortcut(&event(chord)).unwrap();
            for keys in [
                vec![source],
                vec![numeric[0], source],
                vec![source, numeric[0]],
            ] {
                assert!(
                    selected(&before, &keys, incoming, outgoing)
                        .unwrap_err()
                        .contains("Source Text")
                );
                assert_eq!(editor.project(), &before);
                assert!(editor.can_redo());
            }
        }
        editor.redo();
        assert_eq!(
            editor.project().composition().layer(3).unwrap().name(),
            "Redo witness"
        );
    }

    #[test]
    fn invalid_selection_rejects_whole_edit_and_empty_or_missing_side_is_noop() {
        let (mut e, keys) = scene();
        assert!(selected(e.project(), &[], true, true).unwrap().is_none());
        let first = KeyRef {
            frame: 10,
            ..keys[0]
        };
        assert!(
            selected(e.project(), &[first], true, false)
                .unwrap()
                .is_none()
        );
        let last = KeyRef {
            frame: 60,
            ..keys[0]
        };
        assert!(
            selected(e.project(), &[last], false, true)
                .unwrap()
                .is_none()
        );
        let missing = KeyRef {
            frame: 31,
            ..keys[0]
        };
        assert!(selected(e.project(), &[keys[0], missing], true, true).is_err());
        let missing_layer = KeyRef { id: 999, ..keys[0] };
        assert!(selected(e.project(), &[keys[0], missing_layer], true, true).is_err());
        let path = KeyRef {
            property: libre_effects_core::PropertyPath::Path(libre_effects_core::PathTarget::Shape),
            ..keys[0]
        };
        assert!(
            selected(e.project(), &[keys[0], path], true, true)
                .unwrap_err()
                .contains("path keys")
        );
        e.execute(Command::ToggleLocked(2)).unwrap();
        let locked = e.project().clone();
        assert!(selected(e.project(), &keys, true, true).is_err());
        assert_eq!(e.project(), &locked);
    }
}
