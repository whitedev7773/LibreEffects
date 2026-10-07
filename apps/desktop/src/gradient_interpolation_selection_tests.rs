//! The opaque compound selection must not fall through to whole-layer actions.
use super::*;

fn scene() -> EditorState {
    let mut state = EditorState::default();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.bulk_test_action(&Action::Select(1));
    state.editor.clear_history();
    state.colors_key_owned.set(true);
    state
}

#[test]
fn colors_domain_consumes_shell_menu_and_selection_actions_without_layer_fallthrough() {
    let mut state = scene();
    let before = state.editor.project().clone();
    let selected = state.selected_layers.clone();
    for action in [
        Action::CopySelection,
        Action::CutSelection,
        Action::PasteSelection,
        Action::CopyKeys,
        Action::PasteKeys,
        Action::CopyLayers,
        Action::PasteLayers,
        Action::DeleteSelection,
        Action::DuplicateSelection,
        Action::SplitSelection,
        Action::TrimSelection(true),
        Action::TrimSelection(false),
        Action::NudgeSelection(1., 0.),
        Action::PrecomposeSelection,
        Action::PreviousKey,
        Action::NextKey,
        Action::ToggleTimeRemap,
        Action::FreezeTimeRemap,
    ] {
        state.bulk_test_action(&action);
        assert!(state.status.starts_with("Use the selected Colors key's"));
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, selected);
        assert!(state.colors_key_owned.get());
        assert!(!state.editor.can_undo());
        assert!(!state.editor.can_redo());
    }
}

#[test]
fn colors_domain_clears_on_explicit_domain_navigation_even_equal_return_actions() {
    for action in [
        Action::Select(1),
        Action::SelectMany(1, false, false),
        Action::Play,
        Action::SetTool(Tool::Select),
        Action::GraphProperty(1, Property::PositionX.into()),
        Action::ToggleGraph,
        Action::Filter(Some(PropertyFilter::Animated)),
        Action::ToggleExpanded,
        Action::ActivateComposition(1),
        Action::New,
        Action::Open,
        Action::BeginText(None, [0.; 2]),
        Action::BeginParagraph([0.; 4]),
    ] {
        let mut state = scene();
        assert!(!state.prepare_colors_selection_action(&action));
        assert!(!state.colors_key_owned.get());
        assert!(!state.prepare_colors_selection_action(&Action::DeleteSelection));
    }
}

#[test]
fn colors_domain_does_not_change_source_or_views_and_safe_actions_retain_ownership() {
    let mut state = scene();
    let project = state.editor.project().clone();
    let before = state.capture_views();
    for action in [
        Action::Seek(0),
        Action::Step(0),
        Action::Save,
        Action::SaveAs,
        Action::Undo,
        Action::Redo,
        Action::ZoomTimeline(1.),
    ] {
        assert!(!state.prepare_colors_selection_action(&action));
        assert!(state.colors_key_owned.get());
        assert_eq!(state.editor.project(), &project);
        assert_eq!(state.capture_views(), before);
    }
    state.colors_key_owned.set(false);
    assert_eq!(state.capture_views(), before);
    state.colors_key_owned.set(true);
    state.composition_changed();
    assert!(!state.colors_key_owned.get());
}

#[test]
fn colors_clipboard_epoch_retires_equal_history_modal_file_and_domain_actions_but_not_seeks() {
    let mut state = scene();
    let initial = state.colors_clipboard_generation();
    for action in [Action::Seek(0), Action::Step(0)] {
        state.begin_colors_action(&action);
        assert_eq!(state.colors_clipboard_generation(), initial);
    }
    for action in [
        Action::Undo,
        Action::Redo,
        Action::New,
        Action::Open,
        Action::Select(1),
        Action::SetTool(Tool::Select),
        Action::OpenGradient(1),
        Action::CancelGradient,
        Action::ManageFonts,
        Action::ToggleGraph,
    ] {
        let before = state.colors_clipboard_generation();
        state.begin_colors_action(&action);
        assert!(state.colors_clipboard_generation() > before);
    }
    let before = state.colors_clipboard_generation();
    state.step_history(false);
    assert!(state.colors_clipboard_generation() > before);
    let before = state.colors_clipboard_generation();
    state.apply_edit(&Command::ToggleLocked(1));
    assert!(state.colors_clipboard_generation() > before);
}
