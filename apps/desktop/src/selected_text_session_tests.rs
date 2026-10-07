use super::*;
use libre_effects_core::{RichText, TextCharacterPatch, TextStyleRun};

fn state(source: &str) -> crate::editor::EditorState {
    let mut state = crate::editor::EditorState::default();
    state
        .editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: source.into(),
                font_size: 24.0,
            },
            width: 400.0,
            height: 100.0,
            name: "Selection".into(),
        })
        .unwrap();
    state.text_session = Some(
        Session::new(
            state.editor.project(),
            state.document_revision,
            0,
            Some(1),
            [0.0, 0.0],
        )
        .unwrap(),
    );
    state
}
#[test]
fn selection_receipt_retires_after_style_history_selection_and_session_aba() {
    let mut state = state("A한\r\nZ");
    let session = state.text_session.as_mut().unwrap();
    session.buffer.all();
    let initial = session.selection_target().unwrap();
    assert!(initial.current(&state));
    state
        .text_session
        .as_mut()
        .unwrap()
        .format_selection(&TextCharacterPatch::FontSize(48.0))
        .unwrap();
    state.text_session.as_mut().unwrap().buffer.history(false);
    assert!(!initial.current(&state));
    let receipt = state
        .text_session
        .as_ref()
        .unwrap()
        .selection_target()
        .unwrap();
    state.text_session.as_mut().unwrap().buffer.select(0, false);
    state.text_session.as_mut().unwrap().buffer.all();
    assert!(!receipt.current(&state));
    let receipt = state
        .text_session
        .as_ref()
        .unwrap()
        .selection_target()
        .unwrap();
    state.text_session = Some(
        Session::new(
            state.editor.project(),
            state.document_revision,
            0,
            Some(1),
            [0.0, 0.0],
        )
        .unwrap(),
    );
    state.text_session.as_mut().unwrap().buffer.all();
    assert!(!receipt.current(&state));
}
#[test]
fn formatting_and_source_join_commit_exact_final_runs_in_one_undo() {
    let mut state = state("e \u{301}");
    let layer = state.editor.selected_layer().unwrap();
    let red = layer.base_character_style().unwrap();
    let mut blue = red.clone();
    blue.fill_color = 0x0000ff;
    state
        .editor
        .execute(Command::SetRichText {
            id: 1,
            rich_text: Some(
                RichText::new(
                    "e \u{301}",
                    red.clone(),
                    vec![
                        TextStyleRun {
                            start: 0,
                            end: 1,
                            style: red.clone(),
                        },
                        TextStyleRun {
                            start: 1,
                            end: 4,
                            style: blue,
                        },
                    ],
                )
                .unwrap(),
            ),
        })
        .unwrap();
    let baseline = state.editor.project().clone();
    let mut session = Session::new(&baseline, 0, 0, Some(1), [0., 0.]).unwrap();
    session.buffer.all();
    session
        .format_selection(&TextCharacterPatch::FillColor(red.fill_color))
        .unwrap();
    session.buffer.replace(Some(1..2), "", false, None).unwrap();
    state.editor.execute(session.command()).unwrap();
    let applied = state.editor.project().clone();
    assert_eq!(
        applied.composition().layer(1).unwrap().source_text_at(0),
        Some("e\u{301}")
    );
    assert_eq!(
        applied
            .composition()
            .layer(1)
            .unwrap()
            .rich_text()
            .unwrap()
            .runs
            .len(),
        1
    );
    state.editor.undo();
    assert_eq!(state.editor.project(), &baseline);
    state.editor.redo();
    assert_eq!(state.editor.project(), &applied);
}
#[test]
fn style_only_command_uses_exact_rich_edit_and_marked_source_is_untouched() {
    let mut state = state("AB");
    let session = state.text_session.as_mut().unwrap();
    session.buffer.all();
    session
        .format_selection(&TextCharacterPatch::FontSize(48.0))
        .unwrap();
    let Command::Batch(commands) = session.command() else {
        panic!()
    };
    assert!(matches!(commands.as_slice(), [Command::SetRichText { .. }]));
    session.buffer.replace(None, "한", true, None).unwrap();
    let before = (
        session.buffer.text.clone(),
        session.buffer.rich_text.clone(),
        session.buffer.marked.clone(),
        session.buffer.generation(),
    );
    assert!(
        session
            .format_selection(&TextCharacterPatch::FontSize(32.0))
            .is_err()
    );
    assert_eq!(
        (
            session.buffer.text.clone(),
            session.buffer.rich_text.clone(),
            session.buffer.marked.clone(),
            session.buffer.generation()
        ),
        before
    );
}
