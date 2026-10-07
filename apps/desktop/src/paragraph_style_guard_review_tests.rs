//! Independent review regressions for paragraph caret and overflow geometry.
//! These deliberately avoid the Paragraph panel's private planning helpers.
use crate::text_edit::{Session, layout::Layout};
use libre_effects_core::{Project, TextAlign, TextStyle};

fn style() -> TextStyle {
    TextStyle {
        paragraph: true,
        leading: 1.25,
        paragraph_left_indent: 30.,
        paragraph_right_indent: 24.,
        paragraph_first_line_indent: 60.,
        paragraph_space_before: 11.,
        paragraph_space_after: 9.,
        ..Default::default()
    }
}

#[test]
fn paragraph_review_blank_line_carets_hit_testing_and_ime_rects_use_literal_origins() {
    for (align, x) in [
        (TextAlign::Left, 90.),
        (TextAlign::Center, 133.),
        (TextAlign::Right, 176.),
    ] {
        let style = TextStyle { align, ..style() };
        let layout = Layout::shape("\n\n", 24., 200., &style);
        assert!(layout.cells.is_empty());
        assert_eq!(
            layout.carets,
            vec![(0, [x, 11.]), (1, [x, 61.]), (2, [x, 111.])]
        );
        for (index, y) in [11., 61., 111.].into_iter().enumerate() {
            assert_eq!(layout.caret(index), [x, y]);
            assert_eq!(layout.hit_caret([x, y + 12.]), (index, [x, y]));
            assert_eq!(layout.caret_rect([x, y]), [x, y, 1., 24. * 1.2]);
        }
    }
}

#[test]
fn paragraph_review_wrapped_glyph_regions_translate_each_line_local_alignment() {
    let text = "AAAA BBBB CCCC";
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        let style = TextStyle { align, ..style() };
        let layout = Layout::shape(text, 24., 200., &style);
        // Literal source slices, line-local width and origins. Standalone point
        // shaping is used only to recover glyph regions within those intervals.
        for (start, end, x, y, width) in [(0, 4, 90., 11., 86.), (5, 14, 30., 41., 146.)] {
            let local_style = TextStyle {
                paragraph: false,
                ..style.clone()
            };
            let local = Layout::shape(&text[start..end], 24., width, &local_style);
            let actual: Vec<_> = layout
                .cells
                .iter()
                .filter(|cell| cell.range.start >= start && cell.range.end <= end)
                .collect();
            assert_eq!(actual.len(), local.cells.len());
            for (actual, expected) in actual.into_iter().zip(&local.cells) {
                assert_eq!(
                    actual.range,
                    start + expected.range.start..start + expected.range.end
                );
                assert_eq!(
                    (actual.x1, actual.x2, actual.y),
                    (x + expected.x1, x + expected.x2, y)
                );
                let point = [(actual.x1 + actual.x2) / 2., y + 12.];
                assert!(layout.contains(point));
                assert_eq!(layout.hit_character(point), actual.range.start);
                assert!(layout.carets.iter().any(|(_, at)| *at == [actual.x1, y]));
            }
        }
    }
}

#[test]
fn paragraph_review_vertical_navigation_visits_wrapped_and_empty_lines_despite_large_gaps() {
    let mut session = Session::new_box(&Project::default(), 0, 0, [0., 0., 200., 5000.]).unwrap();
    session.font_size = 24.;
    session.style = TextStyle {
        paragraph_space_before: 700.,
        paragraph_space_after: 800.,
        ..style()
    };
    session
        .buffer
        .replace(None, "AAAA BBBB CCCC\n\nD", false, None)
        .unwrap();
    session.buffer.select(0, false);
    let layout = Layout::new(&session);
    assert_eq!(session.caret_position(&layout)[1], 700.);
    for y in [730., 2260., 3790.] {
        session.vertical(true, true);
        assert_eq!(session.caret_position(&layout)[1], y);
        assert_eq!(session.buffer.anchor, 0);
    }
    for y in [2260., 730., 700.] {
        session.vertical(false, false);
        assert_eq!(session.caret_position(&layout)[1], y);
    }
    assert_eq!(session.buffer.caret, 0);
}

#[test]
fn paragraph_review_exhausted_continuation_width_stops_ink_without_losing_source_ranges() {
    let text = "AAAA BBBB CCCC";
    let style = TextStyle {
        paragraph_left_indent: 110.,
        paragraph_right_indent: 0.,
        paragraph_first_line_indent: -110.,
        paragraph_space_before: 0.,
        paragraph_space_after: 0.,
        ..style()
    };
    let flow = crate::text_flow::lines(text, 24., 100., &style);
    assert!(flow.len() > 1);
    assert_eq!((flow[0].x, flow[0].width), (0., 100.));
    assert!(flow[0].fits_width);
    assert_eq!((flow[1].x, flow[1].width), (110., -10.));
    assert!(!flow[1].fits_width);
    assert_eq!(crate::text_flow::composed_count(&flow, 5000.), 1);
    assert_eq!(
        flow.iter()
            .map(|line| &text[line.range.clone()])
            .collect::<String>(),
        text
    );
    let layout = Layout::shape(text, 24., 100., &style);
    assert!(
        layout
            .carets
            .iter()
            .all(|(index, point)| text.is_char_boundary(*index)
                && point.iter().all(|value| value.is_finite()))
    );
}

#[test]
fn paragraph_review_empty_exhausted_intervals_keep_signed_logical_caret_alignment() {
    for (align, x) in [
        (TextAlign::Left, 110.),
        (TextAlign::Center, 105.),
        (TextAlign::Right, 100.),
    ] {
        let style = TextStyle {
            align,
            paragraph_left_indent: 110.,
            paragraph_right_indent: 0.,
            paragraph_first_line_indent: 0.,
            paragraph_space_before: 7.,
            paragraph_space_after: 0.,
            ..style()
        };
        let flow = crate::text_flow::lines("", 24., 100., &style);
        assert_eq!((flow[0].x, flow[0].width), (110., -10.));
        assert_eq!(crate::text_flow::composed_count(&flow, 5000.), 0);
        let layout = Layout::shape("", 24., 100., &style);
        assert_eq!(layout.carets, vec![(0, [x, 7.])]);
        assert_eq!(layout.hit_caret([x, 19.]), (0, [x, 7.]));
    }
}
