use libre_effects_core::{Command, Content, Editor, MediaSharing, Project, project_file};
use std::sync::Arc;

fn shared_image() -> Project {
    let mut editor = Editor::default();
    editor
        .execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            name: "Shared".into(),
            width: 8.,
            height: 8.,
        })
        .unwrap();
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    editor.project().clone()
}
fn wire_clone(project: &Project) -> Project {
    serde_json::from_slice(&serde_json::to_vec(project).unwrap()).unwrap()
}
fn image(project: &Project, id: u64) -> &Arc<str> {
    let Content::Image { png } = project.composition().layer(id).unwrap().content() else {
        panic!()
    };
    png
}
#[test]
fn explicit_aliases_preserve_original_handles_and_native_bytes() {
    let source = shared_image();
    let mut decoded = wire_clone(&source);
    assert!(!Arc::ptr_eq(image(&decoded, 1), image(&decoded, 2)));
    assert_ne!(
        project_file::encode(&decoded, None).unwrap(),
        project_file::encode(&source, None).unwrap()
    );
    decoded
        .restore_media_sharing(&source.media_sharing(), Some(&source))
        .unwrap();
    assert!(Arc::ptr_eq(image(&decoded, 1), image(&source, 1)));
    assert!(Arc::ptr_eq(image(&decoded, 1), image(&decoded, 2)));
    assert_eq!(
        project_file::encode(&decoded, None).unwrap(),
        project_file::encode(&source, None).unwrap()
    );
}
#[test]
fn equal_but_independent_payloads_are_not_globally_deduplicated() {
    let source = wire_clone(&shared_image());
    let mut decoded = wire_clone(&source);
    decoded
        .restore_media_sharing(&source.media_sharing(), Some(&source))
        .unwrap();
    assert!(!Arc::ptr_eq(image(&decoded, 1), image(&decoded, 2)));
    assert!(Arc::ptr_eq(image(&decoded, 1), image(&source, 1)));
    assert!(Arc::ptr_eq(image(&decoded, 2), image(&source, 2)));
    assert_eq!(
        project_file::encode(&decoded, None).unwrap(),
        project_file::encode(&source, None).unwrap()
    );
}
#[test]
fn source_group_splits_and_equal_payload_merges_reject_without_mutation() {
    let shared = shared_image();
    let independent = wire_clone(&shared);
    for (source, aliases) in [
        (&shared, independent.media_sharing()),
        (&independent, shared.media_sharing()),
    ] {
        let mut candidate = wire_clone(source);
        let before = candidate.media_sharing();
        assert!(
            candidate
                .restore_media_sharing(&aliases, Some(source))
                .is_err()
        );
        assert_eq!(candidate.media_sharing(), before);
    }
}
#[test]
fn incomplete_duplicate_unknown_and_mixed_alias_members_reject_atomically() {
    let source = shared_image();
    let valid = serde_json::to_value(source.media_sharing()).unwrap();
    let mut variants = Vec::new();
    let mut missing = valid.clone();
    missing["images"].as_array_mut().unwrap().clear();
    variants.push(missing);
    let mut duplicate = valid.clone();
    let member = duplicate["images"][0][0].clone();
    duplicate["images"][0].as_array_mut().unwrap().push(member);
    variants.push(duplicate);
    let mut unknown = valid.clone();
    unknown["images"][0][0] = serde_json::json!({"Layer":[1,99999]});
    variants.push(unknown);
    let mut mixed = valid.clone();
    mixed["sequences"] = mixed["images"].clone();
    variants.push(mixed);
    for value in variants {
        let aliases: MediaSharing = serde_json::from_value(value).unwrap();
        let mut candidate = wire_clone(&source);
        let before = candidate.media_sharing();
        assert!(
            candidate
                .restore_media_sharing(&aliases, Some(&source))
                .is_err()
        );
        assert_eq!(candidate.media_sharing(), before);
    }
}
#[test]
fn unequal_payload_alias_group_rejects_before_any_repair() {
    let source = shared_image();
    let mut value = serde_json::to_value(&source).unwrap();
    value["composition"]["layers"][0]["content"]["Image"]["png"] = "ZGVm".into();
    let mut candidate: Project = serde_json::from_value(value).unwrap();
    let before = candidate.media_sharing();
    assert!(
        candidate
            .restore_media_sharing(&source.media_sharing(), None)
            .is_err()
    );
    assert_eq!(candidate.media_sharing(), before);
}
#[test]
fn sequence_aliases_preserve_order_and_layer_timing() {
    let mut editor = Editor::default();
    editor
        .execute(Command::ImportAsset {
            content: Content::ImageSequence {
                frames: Arc::new(vec!["b.png".into(), "a.png".into()]),
                fps: 2.into(),
                missing: Default::default(),
                start_frame: 0,
                playback: Default::default(),
            },
            width: 8.,
            height: 8.,
            name: "Sequence".into(),
            folder: None,
            frame: Some(0),
        })
        .unwrap();
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    editor.execute(Command::ReverseVideo { id: 2 }).unwrap();
    let source = editor.project().clone();
    let mut decoded = wire_clone(&source);
    decoded
        .restore_media_sharing(&source.media_sharing(), Some(&source))
        .unwrap();
    assert_eq!(decoded, source);
    let Content::ImageSequence { frames: a, .. } =
        decoded.composition().layer(1).unwrap().content()
    else {
        panic!()
    };
    let Content::ImageSequence { frames: b, .. } =
        decoded.composition().layer(2).unwrap().content()
    else {
        panic!()
    };
    let Content::ImageSequence {
        frames: original, ..
    } = source.composition().layer(1).unwrap().content()
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(a, b) && Arc::ptr_eq(a, original));
    assert_eq!(a.as_ref(), &["b.png", "a.png"]);
    assert_eq!(
        project_file::encode(&decoded, None).unwrap(),
        project_file::encode(&source, None).unwrap()
    );
}
#[test]
fn duplicate_source_references_are_rejected_before_mutation() {
    let source = shared_image();
    let mut value = serde_json::to_value(&source).unwrap();
    let duplicate = value["composition"]["layers"][0].clone();
    value["composition"]["layers"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let mut candidate: Project = serde_json::from_value(value).unwrap();
    assert!(
        candidate
            .restore_media_sharing(&source.media_sharing(), None)
            .unwrap_err()
            .contains("Duplicate")
    );
}
