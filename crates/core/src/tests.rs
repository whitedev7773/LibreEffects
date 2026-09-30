use super::*;

#[test]
fn bundled_motion_study_loads_and_animates() {
    let project =
        Project::from_json(include_str!("../../../examples/motion-study.lfe.json")).unwrap();
    let comp = project.composition();
    assert_eq!(comp.layers().len(), 3);
    let track = comp.layer(1).unwrap().property(Property::PositionX);
    assert_eq!(track.value_at(0), 330.0);
    assert_eq!(track.value_at(60), 470.0);
    assert!(comp.layer(3).unwrap().locked());
}

#[test]
fn duplicate_layer_preserves_animation_with_unique_identity_and_history() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 30, 400.0);
    editor.execute(Command::DuplicateLayer(1)).unwrap();
    let layers = editor.project().composition().layers();
    assert_eq!(layers.len(), 2);
    assert_ne!(layers[0].id(), layers[1].id());
    assert_eq!(
        layers[0].property(Property::PositionX),
        layers[1].property(Property::PositionX)
    );
    assert_eq!(editor.selected(), Some(2));
    editor.undo();
    assert_eq!(editor.project().composition().layers().len(), 1);
    editor.redo();
    assert_eq!(editor.selected(), Some(2));
}

#[test]
fn layer_range_is_exclusive_validated_and_backward_compatible() {
    let mut editor = editor_with_layer();
    editor
        .execute(Command::SetLayerRange {
            id: 1,
            start: 10,
            end: 40,
        })
        .unwrap();
    let layer = editor.selected_layer().unwrap();
    assert!(!layer.active_at(9, 150));
    assert!(layer.active_at(10, 150));
    assert!(layer.active_at(39, 150));
    assert!(!layer.active_at(40, 150));
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::SetLayerRange {
                id: 1,
                start: 40,
                end: 10
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    let mut json: serde_json::Value = serde_json::from_str(&before.to_json().unwrap()).unwrap();
    let layer = json["composition"]["layers"][0].as_object_mut().unwrap();
    layer.remove("in_frame");
    layer.remove("out_frame");
    let legacy = Project::from_json(&json.to_string()).unwrap();
    assert_eq!(legacy.composition().layers()[0].in_frame(), 0);
    assert_eq!(legacy.composition().layers()[0].out_frame(150), 150);
}

#[test]
fn keyframe_move_preserves_interpolation_and_rejects_collisions_atomically() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 30, 400.0);
    editor
        .execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 30,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
    editor
        .execute(Command::MoveKeyframe {
            id: 1,
            property: Property::PositionX,
            from: 30,
            to: 45,
        })
        .unwrap();
    let track = editor
        .selected_layer()
        .unwrap()
        .property(Property::PositionX);
    assert!(!track.keys().contains_key(&30));
    assert_eq!(track.keys()[&45].interpolation, Interpolation::Hold);
    let before = editor.project().clone();
    assert!(
        editor
            .execute(Command::MoveKeyframe {
                id: 1,
                property: Property::PositionX,
                from: 45,
                to: 0
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    editor.undo();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .contains_key(&30)
    );
}

#[test]
fn disabling_animation_bakes_current_value_and_is_undoable() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 0.0);
    key(&mut editor, 0);
    set(&mut editor, 30, 300.0);
    editor
        .execute(Command::ToggleAnimation {
            id: 1,
            property: Property::PositionX,
            frame: 15,
        })
        .unwrap();
    assert!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .is_empty()
    );
    assert_eq!(value(&editor, 0), 150.0);
    editor.undo();
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .len(),
        2
    );
}

#[test]
fn canvas_position_is_one_undo_step_and_invalid_y_is_atomic() {
    let mut editor = editor_with_layer();
    let before = editor.project().clone();
    editor
        .execute(Command::SetPosition {
            id: 1,
            frame: 0,
            x: 500.0,
            y: 400.0,
        })
        .unwrap();
    assert_eq!(value(&editor, 0), 500.0);
    editor.undo();
    assert_eq!(editor.project(), &before);
    assert!(
        editor
            .execute(Command::SetPosition {
                id: 1,
                frame: 0,
                x: 100.0,
                y: f64::NAN
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
}

#[test]
fn composition_settings_preserve_content_and_reject_destructive_shortening() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 100, 100.0);
    let command = |duration| Command::ConfigureComposition {
        name: "Main".into(),
        width: 1280,
        height: 720,
        fps: 24,
        duration,
    };
    let before = editor.project().clone();
    assert!(editor.execute(command(90)).is_err());
    assert_eq!(editor.project(), &before);
    editor.execute(command(240)).unwrap();
    assert_eq!(editor.project().composition().duration(), 240);
    assert_eq!(editor.project().composition().layers().len(), 1);
    let encoded = editor.project().to_json().unwrap();
    assert_eq!(Project::from_json(&encoded).unwrap(), *editor.project());
    editor.undo();
    assert_eq!(editor.project(), &before);
}

#[test]
fn locked_layers_reject_new_commands_and_rename_is_validated() {
    let mut editor = editor_with_layer();
    assert!(
        editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "  ".into()
            })
            .is_err()
    );
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: " Hero ".into(),
        })
        .unwrap();
    assert_eq!(editor.selected_layer().unwrap().name(), "Hero");
    editor.execute(Command::ToggleLocked(1)).unwrap();
    for command in [
        Command::DuplicateLayer(1),
        Command::RenameLayer {
            id: 1,
            name: "Other".into(),
        },
        Command::SetPosition {
            id: 1,
            frame: 0,
            x: 20.0,
            y: 20.0,
        },
        Command::SetLayerRange {
            id: 1,
            start: 1,
            end: 50,
        },
        Command::ToggleAnimation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
        },
    ] {
        assert!(editor.execute(command).is_err());
    }
}

fn editor_with_layer() -> Editor {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    editor
}

fn set(editor: &mut Editor, frame: Frame, value: f64) {
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::PositionX,
            frame,
            value,
        })
        .unwrap();
}

fn key(editor: &mut Editor, frame: Frame) {
    editor
        .execute(Command::ToggleKeyframe {
            id: 1,
            property: Property::PositionX,
            frame,
        })
        .unwrap();
}

fn value(editor: &Editor, frame: Frame) -> f64 {
    editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .property(Property::PositionX)
        .value_at(frame)
}

#[test]
fn animation_clamps_endpoints_and_interpolates_in_frame_time() {
    let mut editor = editor_with_layer();
    set(&mut editor, 10, 100.0);
    key(&mut editor, 10);
    set(&mut editor, 30, 300.0);
    assert_eq!(value(&editor, 0), 100.0);
    assert_eq!(value(&editor, 10), 100.0);
    assert_eq!(value(&editor, 20), 200.0);
    assert_eq!(value(&editor, 30), 300.0);
    assert_eq!(value(&editor, 149), 300.0);
}

#[test]
fn hold_and_smooth_use_the_outgoing_key() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 0.0);
    key(&mut editor, 0);
    set(&mut editor, 100, 100.0);
    for (interpolation, expected) in [(Interpolation::Hold, 0.0), (Interpolation::Smooth, 15.625)] {
        editor
            .execute(Command::SetInterpolation {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                interpolation,
            })
            .unwrap();
        assert_eq!(value(&editor, 25), expected);
        assert_eq!(value(&editor, 100), 100.0);
    }
}

#[test]
fn editing_same_frame_replaces_key_and_removing_last_key_preserves_value() {
    let mut editor = editor_with_layer();
    key(&mut editor, 10);
    set(&mut editor, 10, 42.0);
    set(&mut editor, 10, 84.0);
    assert_eq!(
        editor
            .selected_layer()
            .unwrap()
            .property(Property::PositionX)
            .keys()
            .len(),
        1
    );
    key(&mut editor, 10);
    assert_eq!(value(&editor, 0), 84.0);
    assert_eq!(value(&editor, 149), 84.0);
}

#[test]
fn layer_order_and_selection_survive_undo_redo() {
    let mut editor = editor_with_layer();
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(editor.project().composition().layers()[0].id(), 2);
    editor
        .execute(Command::MoveLayer { id: 2, index: 1 })
        .unwrap();
    assert_eq!(editor.selected(), Some(2));
    editor.execute(Command::RemoveLayer(2)).unwrap();
    assert_eq!(editor.selected(), Some(1));
    editor.undo();
    assert_eq!(editor.selected(), Some(2));
    assert_eq!(editor.project().composition().layers()[1].id(), 2);
    editor.redo();
    assert_eq!(editor.project().composition().layers().len(), 1);
}

#[test]
fn fresh_edit_clears_redo_and_noop_does_not_add_history() {
    let mut editor = editor_with_layer();
    set(&mut editor, 0, 42.0);
    editor.undo();
    assert!(editor.can_redo());
    set(&mut editor, 0, 52.0);
    assert!(!editor.can_redo());
    set(&mut editor, 0, 52.0);
    editor.undo();
    assert_eq!(value(&editor, 0), 960.0);
}

#[test]
fn locked_layers_reject_mutations_without_changing_history() {
    let mut editor = editor_with_layer();
    editor.execute(Command::ToggleLocked(1)).unwrap();
    let before = editor.project().clone();
    assert!(editor.execute(Command::RemoveLayer(1)).is_err());
    assert!(
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                value: 0.0,
            })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
    editor.undo();
    assert!(!editor.selected_layer().unwrap().locked());
}

#[test]
fn invalid_commands_are_atomic() {
    let mut editor = editor_with_layer();
    let before = editor.project().clone();
    for (property, frame, value) in [
        (Property::Opacity, 0, 101.0),
        (Property::PositionX, 150, 0.0),
        (Property::PositionX, 0, f64::NAN),
        (Property::Rotation, 0, f64::INFINITY),
    ] {
        assert!(
            editor
                .execute(Command::SetValue {
                    id: 1,
                    property,
                    frame,
                    value
                })
                .is_err()
        );
        assert_eq!(editor.project(), &before);
    }
    assert!(
        editor
            .execute(Command::MoveLayer { id: 1, index: 10 })
            .is_err()
    );
    assert_eq!(editor.project(), &before);
}

#[test]
fn serialized_project_roundtrips_animation_and_preserves_ids() {
    let mut editor = editor_with_layer();
    key(&mut editor, 0);
    set(&mut editor, 60, 1200.0);
    let loaded = Project::from_json(&editor.project().to_json().unwrap()).unwrap();
    assert_eq!(&loaded, editor.project());
    editor.replace_project(loaded).unwrap();
    editor.execute(Command::AddRectangle).unwrap();
    assert_eq!(editor.selected(), Some(2));
}

#[test]
fn corrupt_and_future_projects_are_rejected() {
    let editor = editor_with_layer();
    let mut project = editor.project().clone();
    project.version = 2;
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.version = 1;
    project.composition.fps = 0;
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.composition.fps = 30;
    project
        .composition
        .layers
        .push(project.composition.layers[0].clone());
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
    project.composition.layers.pop();
    project.composition.layers[0]
        .properties
        .remove(&Property::AnchorX);
    assert!(Project::from_json(&project.to_json().unwrap()).is_err());
}

#[test]
fn replacing_project_can_be_undone() {
    let mut editor = editor_with_layer();
    let previous = editor.project().clone();
    editor.replace_project(Project::default()).unwrap();
    assert!(editor.project().composition().layers().is_empty());
    editor.undo();
    assert_eq!(editor.project(), &previous);
    assert_eq!(editor.selected(), Some(1));
}

#[test]
fn transform_applies_anchor_then_scale_then_rotation_then_position() {
    let mut editor = editor_with_layer();
    editor
        .execute(Command::SetValue {
            id: 1,
            property: Property::Rotation,
            frame: 0,
            value: 90.0,
        })
        .unwrap();
    let corners = editor.selected_layer().unwrap().corners_at(0);
    assert!((corners[0][0] - 1060.0).abs() < 1e-9);
    assert!((corners[0][1] - 380.0).abs() < 1e-9);
}

#[test]
fn invalid_project_replacement_preserves_current_work_and_history() {
    let mut editor = editor_with_layer();
    let previous = editor.project().clone();
    let mut invalid = Project::default();
    invalid.composition.duration = 0;
    assert!(editor.replace_project(invalid).is_err());
    assert_eq!(editor.project(), &previous);
    editor.undo();
    assert!(editor.project().composition().layers().is_empty());
}
