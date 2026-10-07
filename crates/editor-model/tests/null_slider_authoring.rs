//! Native Null controls use the same content capability as actual core commands.
use libre_effects_core::*;

fn null_scene() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddNull).unwrap();
    editor.clear_history();
    editor
}

fn layer(editor: &Editor) -> &Layer {
    editor.project().composition().layer(1).unwrap()
}

fn edit(editor: &mut Editor, edit: EffectEdit) {
    editor.execute(Command::Effect { id: 1, edit }).unwrap();
}

fn reject_unchanged(editor: &mut Editor, edit: EffectEdit) {
    let before = editor.project().clone();
    let receipt = (
        editor.selected(),
        editor.context_generation(),
        editor.can_undo(),
        editor.can_redo(),
    );
    assert!(editor.execute(Command::Effect { id: 1, edit }).is_err());
    assert_eq!(editor.project(), &before);
    assert_eq!(
        (
            editor.selected(),
            editor.context_generation(),
            editor.can_undo(),
            editor.can_redo(),
        ),
        receipt
    );
}

#[test]
fn null_effect_capability_matches_add_rejections_without_consuming_redo() {
    let mut editor = null_scene();
    edit(&mut editor, EffectEdit::Add(EffectKind::SliderControl));
    let added = editor.project().clone();
    editor.undo();
    assert!(editor.can_redo());
    for kind in EffectKind::ALL {
        assert_eq!(
            layer(&editor).supports_effect_kind(kind),
            kind == EffectKind::SliderControl,
            "{kind:?}"
        );
        if kind != EffectKind::SliderControl {
            reject_unchanged(&mut editor, EffectEdit::Add(kind));
        }
    }
    reject_unchanged(&mut editor, EffectEdit::ConvertLegacy);
    editor.redo();
    assert_eq!(editor.project(), &added);
}

#[test]
fn null_slider_name_value_and_keys_have_exact_history_and_persistence() {
    let mut editor = null_scene();
    let baseline = editor.project().clone();
    edit(&mut editor, EffectEdit::Add(EffectKind::SliderControl));
    let added = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &baseline);
    editor.redo();
    assert_eq!(editor.project(), &added);

    edit(
        &mut editor,
        EffectEdit::Rename {
            effect: 1,
            name: "Duration (FPS)".into(),
        },
    );
    edit(
        &mut editor,
        EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
            value: -12.5,
        },
    );
    edit(
        &mut editor,
        EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
        },
    );
    let first_key = editor.project().clone();
    edit(
        &mut editor,
        EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 20,
            value: 37.5,
        },
    );
    let two_keys = editor.project().clone();
    let effect = &layer(&editor).effect_stack()[0];
    assert_eq!(effect.name(), "Duration (FPS)");
    assert_eq!(effect.value_at(EffectParam::Amount, 10), 12.5);
    assert_eq!(
        effect.parameter(EffectParam::Amount).unwrap().keys().len(),
        2
    );
    assert_eq!(editor.selected(), Some(1));
    editor.undo();
    assert_eq!(editor.project(), &first_key);
    editor.redo();
    assert_eq!(editor.project(), &two_keys);

    edit(
        &mut editor,
        EffectEdit::ToggleKey {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 20,
        },
    );
    assert_eq!(editor.project(), &first_key);
    editor.undo();
    assert_eq!(editor.project(), &two_keys);
    let native = project_file::encode(editor.project(), None).unwrap();
    assert_eq!(project_file::decode(&native).unwrap().project, two_keys);
}

#[test]
fn null_slider_locked_edits_preserve_source_selection_and_history() {
    let mut editor = null_scene();
    edit(&mut editor, EffectEdit::Add(EffectKind::SliderControl));
    editor.execute(Command::ToggleLocked(1)).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    editor.undo();
    assert_eq!(editor.selected(), Some(1));
    assert!(editor.can_redo());
    // Content capability intentionally remains separate from the lock guard.
    assert!(layer(&editor).supports_effect_kind(EffectKind::SliderControl));
    for edit in [
        EffectEdit::Add(EffectKind::SliderControl),
        EffectEdit::Rename {
            effect: 1,
            name: "Blocked".into(),
        },
        EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
            value: 10.0,
        },
        EffectEdit::ToggleAnimation {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
        },
        EffectEdit::ToggleKey {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 10,
        },
    ] {
        reject_unchanged(&mut editor, edit);
    }
    editor.redo();
    assert_eq!(editor.project().composition().layers().len(), 2);
}

#[test]
fn pixel_and_audio_layers_keep_their_existing_effect_capabilities() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    for kind in EffectKind::ALL {
        assert!(layer(&editor).supports_effect_kind(kind));
        edit(&mut editor, EffectEdit::Add(kind));
    }
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Audio {
                path: "control-test.wav".into(),
                audio: AudioMetadata {
                    stream_index: 0,
                    sample_rate: 48000,
                    channels: 2,
                    channel_layout: "stereo".into(),
                    duration: 5.0,
                    start_time: 0.0,
                    file_offset: 0.0,
                },
                start_frame: 0,
                playback: Default::default(),
            },
            name: "Audio".into(),
            width: 10.0,
            height: 10.0,
        })
        .unwrap();
    for kind in EffectKind::ALL {
        assert_eq!(
            layer(&editor).supports_effect_kind(kind),
            kind != EffectKind::LumaKey
        );
        if kind == EffectKind::LumaKey {
            reject_unchanged(&mut editor, EffectEdit::Add(kind));
        } else {
            edit(&mut editor, EffectEdit::Add(kind));
        }
    }
}
