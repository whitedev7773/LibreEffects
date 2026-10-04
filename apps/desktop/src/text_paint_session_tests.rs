//! Text transactions retain base typography and all independent paint tracks.
use super::*;
use libre_effects_core::{
    PropertyPath, TemporalHandle, TextFont, TextPaint, TextParam, TextStyle, TrackEdit,
};

fn animated_text(paragraph: bool) -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureComposition {
            name: "Text sessions".into(),
            width: 480,
            height: 320,
            fps: 30,
            duration: 90,
        })
        .unwrap();
    editor
        .execute(Command::AddContent {
            content: Content::Text {
                text: "one two three four five six".into(),
                font_size: 48.,
            },
            width: 180.,
            height: 180.,
            name: "Text".into(),
        })
        .unwrap();
    editor
        .execute(Command::SetColor {
            id: 1,
            color: 0x204060,
        })
        .unwrap();
    editor
        .execute(Command::SetTextStyle {
            id: 1,
            style: TextStyle {
                paragraph,
                tracking: 10.,
                leading: 1.25,
                stroke_enabled: true,
                stroke_color: 0xf02040,
                stroke_width: 4.,
                ..Default::default()
            },
        })
        .unwrap();
    for (parameter, value) in [
        TextParam::FillRed,
        TextParam::FillGreen,
        TextParam::FillBlue,
        TextParam::StrokeRed,
        TextParam::StrokeGreen,
        TextParam::StrokeBlue,
        TextParam::StrokeWidth,
    ]
    .into_iter()
    .zip([224., 192., 160., 48., 112., 224., 48.])
    {
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
        editor
            .execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::Value { frame: 60, value },
            })
            .unwrap();
    }
    for (parameter, frame, incoming, slope, influence) in [
        (TextParam::FillRed, 0, false, 5., 0.4),
        (TextParam::StrokeWidth, 60, true, 0.3, 0.6),
    ] {
        editor
            .execute(Command::SetTemporalHandle {
                id: 1,
                property: PropertyPath::Text(parameter),
                frame,
                incoming,
                handle: TemporalHandle { slope, influence },
            })
            .unwrap();
    }
    editor
}

fn unchanged_paint(before: &Project, after: &Project) {
    let a = before.composition().layer(1).unwrap();
    let b = after.composition().layer(1).unwrap();
    assert_eq!(a.color(), b.color());
    assert_eq!(a.text_style().stroke_color, b.text_style().stroke_color);
    assert_eq!(a.text_style().stroke_width, b.text_style().stroke_width);
    for parameter in TextParam::ALL {
        assert_eq!(
            a.track(PropertyPath::Text(parameter)),
            b.track(PropertyPath::Text(parameter)),
            "{parameter:?}"
        );
        for frame in [0, 15, 30, 45, 60] {
            assert_eq!(
                a.text_value_at(parameter, frame),
                b.text_value_at(parameter, frame)
            );
        }
    }
}

fn geometry(session: &Session) -> (Vec<(usize, [f64; 2])>, Vec<(Range<usize>, f64, f64, f64)>) {
    let layout = layout::Layout::new(session);
    (
        layout.carets.clone(),
        layout
            .cells
            .iter()
            .map(|c| (c.range.clone(), c.x1, c.x2, c.y))
            .collect(),
    )
}

fn flow(session: &Session) -> Vec<(Range<usize>, usize, f64, bool)> {
    crate::text_flow::lines(
        &session.buffer.text,
        session.font_size,
        session.width,
        &session.style,
    )
    .iter()
    .map(|line| {
        (
            line.range.clone(),
            line.visible_end,
            line.bottom,
            line.fits_width,
        )
    })
    .collect()
}

#[test]
fn text_paint_source_cancel_commit_and_recovery_draft_preserve_keys_and_handles() {
    for paragraph in [false, true] {
        let mut editor = animated_text(paragraph);
        let before = editor.project().clone();
        let before_json = before.to_json().unwrap();
        let base_layer = before.composition().layer(1).unwrap();
        assert!(
            base_layer
                .track(PropertyPath::Text(TextParam::FillRed))
                .unwrap()
                .keys()[&0]
                .temporal
                .outgoing
                .is_some()
        );
        assert!(
            base_layer
                .track(PropertyPath::Text(TextParam::StrokeWidth))
                .unwrap()
                .keys()[&60]
                .temporal
                .incoming
                .is_some()
        );
        assert_ne!(
            base_layer
                .text_value_at(TextParam::StrokeWidth, 30)
                .unwrap(),
            base_layer.text_style().stroke_width
        );
        let draft;
        {
            let mut session = Session::new(&before, 7, 30, Some(1), [0.; 2]).unwrap();
            assert_eq!(session.style, base_layer.text_style());
            assert_eq!(session.font_size, 48.);
            session.buffer.all();
            session
                .buffer
                .replace(None, "edited source at the middle frame", false, None)
                .unwrap();
            if paragraph {
                session.width = 240.;
                session.height = 220.;
            }
            draft = session.project().unwrap();
            unchanged_paint(&before, &draft);
            assert_eq!(
                draft.composition().layer(1).unwrap().text_style(),
                base_layer.text_style()
            );
            assert_eq!(editor.project(), &before);
            assert!(session.valid(&before, 7, 30));
            assert!(!session.valid(&before, 7, 31));
            assert!(!session.valid(&before, 8, 30));
        } // Cancel: discard the isolated session, including its resized paragraph box.
        assert_eq!(editor.project().to_json().unwrap(), before_json);

        // Exercise the real durable recovery codec using a draft at frame30;
        // no draft paint samples may leak into either the live source or base style.
        let directory = tempfile::tempdir().unwrap();
        let (recovery, candidates, warnings) =
            crate::recovery::Session::in_directory(directory.path()).unwrap();
        assert!(candidates.is_empty() && warnings.is_empty());
        recovery.checkpoint(0, Some(&draft)).unwrap();
        drop(recovery);
        let (mut restored, candidates, warnings) =
            crate::recovery::Session::in_directory(directory.path()).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].project, draft);
        unchanged_paint(&before, &candidates[0].project);
        assert!(restored.restore(&candidates[0]).unwrap().is_none());
        assert_eq!(editor.project(), &before);
        assert_eq!(
            Project::from_json(&draft.to_json().unwrap()).unwrap(),
            draft
        );
        let native = crate::project_io::encode_native_project(&draft, None).unwrap();
        assert_eq!(
            crate::project_io::decode_project(&native).unwrap().project,
            draft
        );

        let mut session = Session::new(&before, 7, 30, Some(1), [0.; 2]).unwrap();
        session.buffer.all();
        session
            .buffer
            .replace(None, "edited source at the middle frame", false, None)
            .unwrap();
        if paragraph {
            session.width = 240.;
            session.height = 220.;
        }
        editor.execute(session.command()).unwrap();
        assert_eq!(editor.project(), &draft);
        unchanged_paint(&before, editor.project());
        assert!(
            matches!(editor.project().composition().layer(1).unwrap().content(), Content::Text { font_size, .. } if *font_size == 48.)
        );
        editor.undo();
        assert_eq!(editor.project(), &before);
        editor.redo();
        assert_eq!(editor.project(), &draft);
    }
}

#[test]
fn text_paint_changes_never_move_carets_wraps_or_source_ranges() {
    for paragraph in [false, true] {
        let mut editor = animated_text(paragraph);
        let base = editor.project().clone();
        let first = Session::new(&base, 0, 0, Some(1), [0.; 2]).unwrap();
        let expected_geometry = geometry(&first);
        let expected_flow = flow(&first);
        for frame in [0, 15, 30, 45, 60] {
            let session = Session::new(&base, 0, frame, Some(1), [0.; 2]).unwrap();
            assert_eq!(geometry(&session), expected_geometry);
            assert_eq!(flow(&session), expected_flow);
            // Also prove geometry ignores even the sampled render-only style.
            let mut sampled = session.clone();
            let layer = base.composition().layer(1).unwrap();
            sampled.style.stroke_width =
                layer.text_value_at(TextParam::StrokeWidth, frame).unwrap();
            sampled.style.stroke_color = layer.text_color_at(TextPaint::Stroke, frame).unwrap();
            assert_eq!(geometry(&sampled), expected_geometry);
            assert_eq!(flow(&sampled), expected_flow);
        }
        editor
            .execute(Command::EditText {
                id: 1,
                parameter: TextParam::StrokeWidth,
                edit: TrackEdit::Value {
                    frame: 30,
                    value: 1000.,
                },
            })
            .unwrap();
        let command = editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .text_color_command(TextPaint::Fill, 0xabcdef, 30)
            .unwrap();
        editor.execute(command).unwrap();
        let changed = Session::new(editor.project(), 1, 30, Some(1), [0.; 2]).unwrap();
        assert_eq!(geometry(&changed), expected_geometry);
        assert_eq!(flow(&changed), expected_flow);
        assert_eq!(changed.font_size, first.font_size);
        assert_eq!(changed.style, first.style);
        assert_eq!(changed.buffer.text, first.buffer.text);
    }
}

#[test]
fn text_paint_survives_point_paragraph_conversion_and_actual_font_replacement() {
    let mut editor = animated_text(false);
    let before = editor.project().clone();
    for paragraph in [true, false] {
        let command = crate::text_flow::convert(
            editor.project().composition().layer(1).unwrap(),
            paragraph,
            30,
        );
        editor.execute(command).unwrap();
        unchanged_paint(&before, editor.project());
        assert_eq!(
            editor
                .project()
                .composition()
                .layer(1)
                .unwrap()
                .text_style()
                .paragraph,
            paragraph
        );
    }
    let mut style = editor
        .project()
        .composition()
        .layer(1)
        .unwrap()
        .text_style();
    style.font_family = "Missing animated text font QA".into();
    editor
        .execute(Command::SetTextStyle { id: 1, style })
        .unwrap();
    let before_font = editor.project().clone();
    let base_style = before_font.composition().layer(1).unwrap().text_style();
    let from = TextFont::of(&base_style);
    let to = TextFont::of(&TextStyle::default());
    let replacement =
        crate::font_usage::Replacement::new(&before_font, 9, from, to.clone()).unwrap();
    assert_eq!(replacement.count, 1);
    editor
        .execute(replacement.command(&before_font, 9).unwrap())
        .unwrap();
    unchanged_paint(&before_font, editor.project());
    let mut expected_style = base_style;
    to.apply(&mut expected_style);
    assert_eq!(
        editor
            .project()
            .composition()
            .layer(1)
            .unwrap()
            .text_style(),
        expected_style
    );
    assert_eq!(
        editor.project().composition().layer(1).unwrap().content(),
        before_font.composition().layer(1).unwrap().content()
    );
    let replaced = editor.project().clone();
    editor.undo();
    assert_eq!(editor.project(), &before_font);
    editor.redo();
    assert_eq!(editor.project(), &replaced);
    let native = crate::project_io::encode_native_project(&replaced, None).unwrap();
    assert_eq!(
        crate::project_io::decode_project(&native).unwrap().project,
        replaced
    );
    assert_eq!(
        Project::from_json(&replaced.to_json().unwrap()).unwrap(),
        replaced
    );
}
