use super::*;

#[test]
fn editor_context_generation_detects_source_history_and_selection_aba() {
    let mut editor = Editor::default();
    assert_eq!(editor.context_generation(), 0);
    editor.execute(Command::AddRectangle).unwrap();
    let generation = editor.context_generation();
    let source = editor.current.clone();
    editor
        .execute(Command::RenameLayer {
            id: 1,
            name: "Changed".into(),
        })
        .unwrap();
    assert_eq!(editor.context_generation(), generation + 1);
    editor.undo();
    assert_eq!(editor.current, source);
    assert_eq!(editor.context_generation(), generation + 2);
    editor.redo();
    assert_eq!(editor.context_generation(), generation + 3);
    editor.undo();
    let generation = editor.context_generation();
    editor.clear_selection();
    editor.select(1);
    assert_eq!(editor.current, source);
    assert_eq!(editor.context_generation(), generation + 2);
    let generation = editor.context_generation();
    editor.clear_history();
    assert_eq!(editor.current, source);
    assert_eq!(editor.context_generation(), generation + 1);
    editor.clear_history();
    assert_eq!(editor.context_generation(), generation + 1);
}

#[test]
fn editor_context_generation_preserves_exact_noops_failures_and_serialized_sources() {
    let mut editor = Editor::default();
    editor.undo();
    editor.redo();
    editor.select(9);
    editor.clear_selection();
    editor.clear_history();
    assert_eq!(editor.context_generation(), 0);
    editor.execute(Command::AddRectangle).unwrap();
    let generation = editor.context_generation();
    let source = editor.project().to_json().unwrap();
    editor.select(1);
    editor.select(99);
    editor
        .execute(Command::MoveLayer { id: 1, index: 0 })
        .unwrap();
    assert!(editor.execute(Command::RemoveLayer(99)).is_err());
    assert!(
        editor
            .execute(Command::ImportSvg {
                contents: ShapeContents::default(),
                width: 100.,
                height: 100.,
                name: "Empty".into()
            })
            .is_err()
    );
    let mut invalid = editor.project().clone();
    invalid.version = 0;
    assert!(editor.replace_project(invalid).is_err());
    assert_eq!(editor.context_generation(), generation);
    assert_eq!(editor.project().to_json().unwrap(), source);
    editor.clear_selection();
    editor.select(1);
    assert_eq!(editor.project().to_json().unwrap(), source);
    assert!(!source.contains("context_generation"));
}

#[test]
fn editor_context_generation_detects_equal_replacement_and_composition_aba() {
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle).unwrap();
    let project = editor.project().clone();
    let generation = editor.context_generation();
    editor.replace_project(project.clone()).unwrap();
    assert_eq!(editor.project(), &project);
    assert_eq!(editor.context_generation(), generation + 1);
    editor.execute(Command::NewComposition).unwrap();
    let source = editor.current.clone();
    let generation = editor.context_generation();
    editor.activate_composition(1).unwrap();
    editor.activate_composition(2).unwrap();
    assert_eq!(editor.current, source);
    assert_eq!(editor.context_generation(), generation + 2);
    editor.activate_composition(2).unwrap();
    assert!(editor.activate_composition(99).is_err());
    assert_eq!(editor.context_generation(), generation + 2);
}
