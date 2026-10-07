use super::*;

fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ImportAsset {
            content: Content::Audio {
                path: "synthetic-tone.wav".into(),
                audio: AudioMetadata {
                    stream_index: 0,
                    sample_rate: 48_000,
                    channels: 2,
                    channel_layout: "stereo".into(),
                    duration: 5.0,
                    start_time: 0.0,
                    file_offset: 0.0,
                },
                start_frame: 0,
                playback: Default::default(),
            },
            width: 1.0,
            height: 1.0,
            name: "Audio source".into(),
            folder: None,
            frame: None,
        })
        .unwrap();
    editor
        .execute(Command::AddAssetLayer { asset: 1, frame: 0 })
        .unwrap();
    editor.execute(Command::AddSolid).unwrap();
    editor
        .execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Add(EffectKind::AudioSpectrum),
        })
        .unwrap();
    let mut settings = settings(&editor).clone();
    settings.source = Some(SpectrumSource { layer: 1 });
    settings.bands = 1920;
    set(&mut editor, settings).unwrap();
    editor
}
fn settings(editor: &Editor) -> &AudioSpectrumSettings {
    editor
        .project()
        .composition()
        .layer(2)
        .unwrap()
        .effect_stack()[0]
        .audio_spectrum()
        .unwrap()
}
fn set(editor: &mut Editor, settings: AudioSpectrumSettings) -> Result<(), String> {
    editor.execute(Command::Effect {
        id: 2,
        edit: EffectEdit::SetAudioSpectrum {
            effect: 1,
            settings,
        },
    })
}
fn check_refs(comp: &Composition) {
    for layer in comp.layers() {
        for source in layer.spectrum_sources() {
            assert!(comp.layer(source).unwrap().content().audio().is_some());
        }
    }
}
#[test]
fn spectrum_settings_schema_roundtrip_bounds_and_noop_history() {
    let mut editor = scene();
    let before = editor.project().clone();
    assert_eq!(before.version, 79);
    assert_eq!(
        Project::from_json(&before.to_json().unwrap()).unwrap(),
        before
    );
    let mut bad = serde_json::to_value(&before).unwrap();
    bad["version"] = 78.into();
    assert!(
        Project::from_json(&bad.to_string())
            .unwrap_err()
            .contains("79")
    );
    bad["version"] = 79.into();
    bad["composition"]["layers"][0]["effect_stack"][0]["audio_spectrum"]["guessed_ae_default"] =
        true.into();
    assert!(Project::from_json(&bad.to_string()).is_err());
    for bands in [0, 4097] {
        let mut bad = settings(&editor).clone();
        bad.bands = bands;
        assert!(set(&mut editor, bad).is_err());
        assert_eq!(editor.project(), &before);
    }
    let mut changed = settings(&editor).clone();
    changed.bands = 4096;
    set(&mut editor, changed).unwrap();
    let after = editor.project().clone();
    editor.undo();
    let same = settings(&editor).clone();
    set(&mut editor, same).unwrap();
    assert_eq!(editor.project(), &before);
    editor.redo();
    assert_eq!(editor.project(), &after);
}
#[test]
fn missing_incompatible_and_locked_sources_fail_atomically() {
    let mut editor = scene();
    let before = editor.project().clone();
    for source in [2, 999] {
        let mut bad = settings(&editor).clone();
        bad.source = Some(SpectrumSource { layer: source });
        assert!(set(&mut editor, bad).is_err());
        assert_eq!(editor.project(), &before);
    }
    assert!(editor.execute(Command::RemoveLayer(1)).is_err());
    assert_eq!(editor.project(), &before);
    editor.execute(Command::ToggleLocked(2)).unwrap();
    let locked = editor.project().clone();
    let mut changed = settings(&editor).clone();
    changed.bands = 10;
    assert!(set(&mut editor, changed).is_err());
    assert_eq!(editor.project(), &locked);
    editor.undo();
    editor
        .execute(Command::Batch(vec![
            Command::RemoveLayer(1),
            Command::RemoveLayer(2),
        ]))
        .unwrap();
    assert!(editor.project().composition().layers().is_empty());
    editor.undo();
    assert_eq!(editor.project(), &before);
}
#[test]
fn duplication_clipboard_and_composition_remap_stable_references() {
    let mut editor = scene();
    let before = editor.project().clone();
    editor
        .execute(Command::DuplicateLayers(vec![1, 2]))
        .unwrap();
    check_refs(editor.project().composition());
    let copied = editor
        .project()
        .composition()
        .layers()
        .iter()
        .find(|l| l.id() > 2 && l.effect_stack().len() == 1)
        .unwrap();
    assert!(copied.spectrum_sources().next().unwrap() > 2);
    editor.undo();
    assert_eq!(editor.project(), &before);
    let external = editor.copy_layers(&[2]).unwrap();
    let closure = editor.copy_layers(&[1, 2]).unwrap();
    editor.execute(Command::DuplicateComposition).unwrap();
    check_refs(editor.project().composition());
    assert!(
        editor
            .project()
            .composition()
            .layers()
            .iter()
            .flat_map(Layer::spectrum_sources)
            .all(|id| id > 2)
    );
    let duplicate = editor.project().clone();
    assert!(editor.execute(Command::PasteLayers(external)).is_err());
    assert_eq!(editor.project(), &duplicate);
    editor.execute(Command::PasteLayers(closure)).unwrap();
    check_refs(editor.project().composition());
}
#[test]
fn split_and_precompose_require_the_audio_dependency_closure() {
    let mut editor = scene();
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::SplitLayers {
                ids: vec![1],
                frame: 30
            })
            .is_err()
    );
    assert!(
        editor
            .execute(Command::Precompose {
                layers: vec![1],
                name: "Incomplete".into()
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    editor
        .execute(Command::SplitLayers {
            ids: vec![1, 2],
            frame: 30,
        })
        .unwrap();
    let comp = editor.project().composition();
    check_refs(comp);
    for layer in comp.layers() {
        for source in layer.spectrum_sources() {
            assert_eq!(layer.in_frame(), comp.layer(source).unwrap().in_frame());
        }
    }
    editor.undo();
    editor
        .execute(Command::Precompose {
            layers: vec![1, 2],
            name: "Audio closure".into(),
        })
        .unwrap();
    check_refs(editor.project().composition_by_id(2).unwrap());
    editor.undo();
    assert_eq!(editor.project(), &before);
}
#[test]
fn presets_unbind_sources_and_reorder_rename_mute_keep_identity() {
    let mut editor = scene();
    let preset = EffectPreset::capture(
        editor.project().composition().layer(2).unwrap(),
        None,
        30.into(),
        "Native spectrum",
    )
    .unwrap();
    assert_eq!(serde_json::to_value(&preset).unwrap()["version"], 6);
    assert!(
        preset.effects()[0]
            .audio_spectrum()
            .unwrap()
            .source
            .is_none()
    );
    assert_eq!(
        EffectPreset::from_json(&preset.to_json().unwrap()).unwrap(),
        preset
    );
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Renamed audio".into(),
        })
        .unwrap();
    editor
        .execute(Command::MoveLayer { id: 1, index: 0 })
        .unwrap();
    editor
        .execute(Command::SetAudioEnabled {
            id: 1,
            enabled: false,
        })
        .unwrap();
    assert_eq!(settings(&editor).source.unwrap().layer, 1);
    assert_eq!(editor.project().spectrum_source_layers(1).unwrap(), vec![1]);
}
#[test]
fn difference_requires_new_schema_without_rewriting_legacy_modes() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
        .execute(Command::SetBlendMode {
            id: 1,
            mode: BlendMode::Multiply,
        })
        .unwrap();
    let legacy = editor.project().clone();
    assert_eq!(legacy.version, 17);
    editor
        .execute(Command::SetBlendMode {
            id: 1,
            mode: BlendMode::Difference,
        })
        .unwrap();
    assert_eq!(editor.project().version, 79);
    let mut incompatible = editor.project().clone();
    incompatible.version = 78;
    assert!(Project::from_json(&serde_json::to_string(&incompatible).unwrap()).is_err());
    editor.undo();
    assert_eq!(editor.project(), &legacy);
}
