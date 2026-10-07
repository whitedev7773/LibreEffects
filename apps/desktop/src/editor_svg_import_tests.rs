use super::*;

fn imported() -> ImportedSvg {
    crate::svg_import::parse(
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="60"><rect x="5" y="7" width="30" height="20" fill="#2480c0"/></svg>"##,
    )
    .unwrap()
}

fn ready() -> EditorState {
    let mut state = EditorState::default();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.editor.execute(Command::AddRectangle).unwrap();
    state.editor.undo();
    state.selected_layers = state.editor.selected().into_iter().collect();
    state.saved = state.editor.project().clone();
    state.path = Some("working.lep".into());
    state.source_format = Some(crate::project_io::ProjectFormat::Lep);
    state.imported_original = Some("original.json".into());
    state
}

fn finish(state: &mut EditorState, receipt: &Receipt) {
    state.finish_svg_import(receipt, Ok(Some((imported(), "Icon".into()))), false);
}

#[test]
fn svg_chooser_is_single_file_and_cancellation_is_empty() {
    assert_eq!(one_svg_path(None).unwrap(), None);
    assert_eq!(one_svg_path(Some(vec![])).unwrap(), None);
    assert_eq!(
        one_svg_path(Some(vec!["icon.svg".into()])).unwrap(),
        Some("icon.svg".into())
    );
    assert!(one_svg_path(Some(vec!["a.svg".into(), "b.svg".into()])).is_err());
    assert_eq!(svg_layer_name(Path::new("/tmp/My Icon.SVG")), "My Icon");
    assert_eq!(svg_layer_name(Path::new("/")), "Imported SVG");
    assert_eq!(svg_layer_name(Path::new("\n\r.svg")), "Imported SVG");
    assert_eq!(
        svg_layer_name(Path::new(&format!("{}.svg", "a".repeat(300)))).len(),
        128
    );
}

#[test]
fn svg_import_success_is_one_undo_preserving_baseline_and_provenance() {
    let mut state = ready();
    let before = state.editor.project().clone();
    let saved = state.saved.clone();
    let path = state.path.clone();
    let original = state.imported_original.clone();
    let receipt = state.begin_svg_import();
    finish(&mut state, &receipt);
    assert!(state.status.starts_with("Imported SVG"), "{}", state.status);
    assert!(state.pending_svg_import.is_none());
    assert_eq!(state.editor.project().composition().layers().len(), 2);
    assert_eq!(state.saved, saved);
    assert_eq!(state.path, path);
    assert_eq!(state.imported_original, original);
    assert_eq!(
        state.source_format,
        Some(crate::project_io::ProjectFormat::Lep)
    );
    assert_eq!(
        state.selected_layers,
        state.editor.selected().into_iter().collect()
    );
    assert!(state.selected_keys.is_empty());
    assert!(state.dirty());
    let after = state.editor.project().clone();
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    state.editor.redo();
    assert_eq!(state.editor.project(), &after);
    // A repeated completion cannot insert a second layer.
    finish(&mut state, &receipt);
    assert_eq!(state.editor.project(), &after);
}

#[test]
fn svg_import_cancel_and_errors_preserve_source_selection_and_redo() {
    for failure in 0..3 {
        let mut state = ready();
        let before = state.editor.project().clone();
        let selected = state.editor.selected();
        let layers = state.selected_layers.clone();
        let baseline = state.saved.clone();
        let receipt = state.begin_svg_import();
        let result = match failure {
            0 => Ok(None),
            1 => Err("Unsupported SVG element <script>".into()),
            _ => {
                let mut invalid = imported();
                invalid.width = f64::NAN;
                Ok(Some((invalid, "Invalid".into())))
            }
        };
        state.finish_svg_import(&receipt, result, false);
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.editor.selected(), selected);
        assert_eq!(state.selected_layers, layers);
        assert_eq!(state.saved, baseline);
        assert!(state.editor.can_redo());
        assert!(!state.dirty());
        assert!(state.pending_svg_import.is_none());
        assert!(
            state
                .status
                .contains(if failure == 0 { "canceled" } else { "failed" })
        );
    }
}

#[test]
fn svg_import_newer_file_operation_owns_status_and_pending_state() {
    let mut state = ready();
    let first = state.begin_svg_import();
    state.begin_file_operation(); // A newer Open/Save/New may itself be canceled.
    state.status = "Newer operation".into();
    let before = state.editor.project().clone();
    finish(&mut state, &first);
    assert_eq!(state.status, "Newer operation");
    assert_eq!(state.editor.project(), &before);
    let second = state.begin_svg_import();
    finish(&mut state, &first);
    assert_eq!(state.pending_svg_import, Some(second.operation));
    finish(&mut state, &second);
    assert!(state.status.starts_with("Imported SVG"));
}

#[test]
fn svg_import_receipt_rejects_document_source_selection_time_and_pending_input() {
    for case in 0..12 {
        let mut state = ready();
        let receipt = state.begin_svg_import();
        match case {
            0 => state.document_revision += 1,
            1 => {
                state
                    .editor
                    .execute(Command::RenameLayer {
                        id: 1,
                        name: "Changed".into(),
                    })
                    .unwrap();
            }
            2 => state.editor.clear_selection(),
            3 => state.selected_layers.clear(),
            4 => {
                state.selected_keys.insert(KeyRef {
                    id: 1,
                    property: Property::PositionX.into(),
                    frame: 0,
                });
            }
            5 => {
                state.contents_selection =
                    Some((state.editor.project().active_composition_id(), 1, 7))
            }
            6 => state.frame += 1,
            7 => state.tool = Tool::Pen,
            8 => state.playing = true,
            9 => state.preview_caching = true,
            10 => state.fonts_open = true,
            11 => {}
            _ => unreachable!(),
        }
        let before = state.editor.project().clone();
        let selected = state.editor.selected();
        let can_redo = state.editor.can_redo();
        state.finish_svg_import(&receipt, Ok(Some((imported(), "Icon".into()))), case == 11);
        assert!(
            state.status.contains("context changed"),
            "{case}: {}",
            state.status
        );
        assert_eq!(state.editor.project(), &before, "{case}");
        assert_eq!(state.editor.selected(), selected, "{case}");
        assert_eq!(state.editor.can_redo(), can_redo, "{case}");
    }
}

#[test]
fn svg_import_receipt_rejects_source_transport_selection_and_modal_aba() {
    for case in 0..8 {
        let mut state = ready();
        let receipt = state.begin_svg_import();
        match case {
            0 => {
                state.bulk_test_action(&Action::Edit(Command::RenameLayer {
                    id: 1,
                    name: "Temporary".into(),
                }));
                state.bulk_test_action(&Action::Undo);
            }
            1 => {
                state.bulk_test_action(&Action::Seek(10));
                state.bulk_test_action(&Action::Seek(0));
            }
            2 => {
                state.bulk_test_action(&Action::Play);
                state.bulk_test_action(&Action::Play);
            }
            3 => {
                state.bulk_test_action(&Action::SetTool(Tool::Pen));
                state.bulk_test_action(&Action::SetTool(Tool::Select));
            }
            4 => state.retire_colors_context(), // Shell modal open and close.
            5 => {
                state
                    .editor
                    .execute(Command::RenameLayer {
                        id: 1,
                        name: "Core temporary".into(),
                    })
                    .unwrap();
                state.editor.undo();
            }
            6 => {
                state.editor.clear_selection();
                state.editor.select(1);
            }
            7 => {
                let source = state.editor.project().clone();
                state.editor.replace_project(source).unwrap();
            }
            _ => unreachable!(),
        }
        let before = state.editor.project().clone();
        finish(&mut state, &receipt);
        assert!(state.status.contains("context changed"), "{case}");
        assert_eq!(state.editor.project(), &before, "{case}");
    }
}

#[test]
fn svg_import_availability_blocks_interleaved_operations_and_modal_edits() {
    for case in 0..9 {
        let mut state = ready();
        assert!(state.svg_import_available());
        match case {
            0 => state.saving = true,
            1 => state.collecting = true,
            2 => state.importing_video = true,
            3 => state.exporting = true,
            4 => state.new_composition_requested = true,
            5 => state.close_after_save = true,
            6 => state.media_open = true,
            7 => state.fonts_open = true,
            8 => state.queue_open = true,
            _ => unreachable!(),
        }
        assert!(!state.svg_import_available(), "{case}");
    }
    let mut state = ready();
    let receipt = state.begin_svg_import();
    assert!(!state.svg_import_available());
    state.finish_svg_import(&receipt, Ok(None), false);
    assert!(state.svg_import_available());
}

#[test]
fn svg_import_workspace_capture_retires_transient_selection_aba_only_while_pending() {
    let mut state = ready();
    let input = state.input_context_generation();
    state.retire_pending_svg_import();
    assert_eq!(state.input_context_generation(), input);
    let receipt = state.begin_svg_import();
    state.retire_pending_svg_import();
    let original = state.selected_keys.clone();
    state.selected_keys.insert(KeyRef {
        id: 1,
        property: Property::PositionX.into(),
        frame: 0,
    });
    state.selected_keys = original;
    let before = state.editor.project().clone();
    finish(&mut state, &receipt);
    assert!(state.status.contains("context changed"));
    assert_eq!(state.editor.project(), &before);
    assert!(state.editor.can_redo());
}

#[test]
fn svg_inline_style_parser_success_and_failure_use_atomic_ui_transaction() {
    let mut state = ready();
    let before = state.editor.project().clone();
    let selection = state.selected_layers.clone();
    let saved = state.saved.clone();
    let path = state.path.clone();
    let provenance = state.imported_original.clone();
    for source in [
        "<svg width='80' height='60'><rect width='30' height='20' style='fill:red;stroke:url(https://invalid.example/x)'/></svg>",
        "<svg width='80' height='60'><rect width='30' height='20' style='stroke-width:-1;stroke-width:2'/></svg>",
        "<svg width='80' height='60'><rect width='30' height='20' style='fill:red!important'/></svg>",
    ] {
        let receipt = state.begin_svg_import();
        let parsed = crate::svg_import::parse(source.as_bytes())
            .map(|imported| Some((imported, "Inline".into())));
        assert!(parsed.is_err());
        state.finish_svg_import(&receipt, parsed, false);
        assert_eq!(state.editor.project(), &before);
        assert_eq!(state.selected_layers, selection);
        assert_eq!(state.saved, saved);
        assert_eq!(state.path, path);
        assert_eq!(state.imported_original, provenance);
        assert!(state.editor.can_redo());
        assert!(!state.dirty());
        assert!(state.status.contains("failed"));
    }
    let parsed = crate::svg_import::parse(br##"<svg width="80" height="60" style="fill:red;opacity:.7"><rect width="30" height="20" fill="green" style="fill:blue;fill:#2480c0;stroke:none"/></svg>"##).unwrap();
    let expected = parsed.contents.clone();
    let receipt = state.begin_svg_import();
    state.finish_svg_import(&receipt, Ok(Some((parsed, "Inline".into()))), false);
    let after = state.editor.project().clone();
    assert_eq!(
        state.editor.selected_layer().unwrap().content(),
        &Content::ShapeContents(expected)
    );
    assert!(state.status.starts_with("Imported SVG"));
    state.editor.undo();
    assert_eq!(state.editor.project(), &before);
    state.editor.redo();
    assert_eq!(state.editor.project(), &after);
    assert_eq!(state.saved, saved);
    assert_eq!(state.path, path);
    assert_eq!(state.imported_original, provenance);
}
