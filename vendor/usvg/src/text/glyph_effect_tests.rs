use crate::{GlyphRenderEffect, Node, Options, Tree, WriteOptions};

fn fixture(text: &str, spacing: f32) -> Tree {
    fixture_with_attributes(text, spacing, "")
}

fn fixture_with_attributes(text: &str, spacing: f32, attributes: &str) -> Tree {
    let mut options = Options::default();
    options.fontdb_mut().load_font_data(
        include_bytes!("../../../../apps/desktop/assets/fonts/WantedSans-Regular.ttf").to_vec(),
    );
    Tree::from_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='300' height='200'><text x='10' y='80' font-family='Wanted Sans' font-size='64' letter-spacing='{spacing}' xml:space='preserve' {attributes}>{text}</text></svg>"
    ), &options).unwrap()
}
fn node(tree: &Tree) -> &crate::Text {
    match &tree.root().children()[0] {
        Node::Text(text) => text,
        _ => panic!("expected text"),
    }
}
fn effects(tree: &Tree) -> Vec<GlyphRenderEffect> {
    node(tree)
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .enumerate()
        .map(|(unit, _)| GlyphRenderEffect {
            unit,
            transform: crate::Transform::default(),
            dx: 0.,
            dy: 0.,
            opacity: 1.,
        })
        .collect()
}

#[test]
fn exact_identity_retains_original_flattened_svg() {
    let tree = fixture("office e&#x301; AV", 0.);
    let expected = tree.to_string(&WriteOptions::default());
    let effects = effects(&tree);
    let actual = tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(actual.to_string(&WriteOptions::default()), expected);
}
#[test]
fn source_ranges_are_authoritative_before_negative_spacing_hides_glyphs() {
    let tree = fixture("AAAA", -1000.);
    let ranges: Vec<_> = node(&tree)
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .map(|glyph| glyph.source_range.clone())
        .collect();
    assert_eq!(ranges, vec![3..4]);
    let tree = fixture("e&#x301;AAA", 0.);
    let ranges: Vec<_> = node(&tree)
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .map(|glyph| glyph.source_range.clone())
        .collect();
    assert_eq!(ranges.first(), Some(&(0..3)));
    assert!(ranges
        .iter()
        .all(|range| "e\u{301}AAA".is_char_boundary(range.start)
            && "e\u{301}AAA".is_char_boundary(range.end)));
}
#[test]
fn malformed_effects_fail_without_returning_a_partially_changed_tree() {
    let tree = fixture("AA", 0.);
    assert!(tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(vec![])))
        .is_err());
    let mut effects = effects(&tree);
    effects[0].opacity = f32::NAN;
    assert!(tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .is_err());
    effects[0].opacity = 0.5;
    effects[0].dx = f32::INFINITY;
    assert!(tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .is_err());
}
#[test]
fn opaque_translation_preserves_path_merging_and_opacity_uses_protected_units() {
    let tree = fixture("AAAA", 0.);
    let mut effects = effects(&tree);
    let baseline_paths = node(&tree).flattened().children().len();
    for effect in &mut effects {
        effect.dx = 20.;
    }
    let moved = tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(node(&moved).flattened().children().len(), baseline_paths);
    for effect in &mut effects {
        effect.opacity = 0.5;
        effect.unit /= 2;
    }
    let faded = tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(node(&faded).flattened().children().len(), 2);
    for child in node(&faded).flattened().children() {
        let Node::Group(group) = child else {
            panic!("expected opacity group")
        };
        assert_eq!(group.opacity().get(), 0.5);
    }
}

#[test]
fn protected_units_cannot_split_attenuation_or_disagree_about_effects() {
    let tree = fixture("AAA", 0.);
    let mut effects = effects(&tree);
    effects[0].unit = 0;
    effects[1].unit = 1;
    effects[2].unit = 0;
    for effect in &mut effects {
        effect.opacity = 0.5;
    }
    assert!(tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .is_err());
    effects[1].unit = 0;
    effects[1].dx = 3.;
    assert!(tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .is_err());
}

#[test]
fn opt_in_serialization_preserves_every_original_glyph_coordinate() {
    fn points(group: &crate::Group, result: &mut Vec<Vec<[u32; 2]>>) {
        for node in group.children() {
            match node {
                Node::Group(group) => points(group, result),
                Node::Text(text) => points(text.flattened(), result),
                Node::Path(path) => result.push(
                    path.data()
                        .points()
                        .iter()
                        .map(|point| [point.x.to_bits(), point.y.to_bits()])
                        .collect(),
                ),
                _ => {}
            }
        }
    }
    let tree = fixture("wanted_logoABe&#x301;", 0.);
    let restored = Tree::from_str(
        &tree.to_string_with_unique_resource_ids(&WriteOptions::default()),
        &Options::default(),
    )
    .unwrap();
    let mut expected = vec![];
    let mut actual = vec![];
    points(tree.root(), &mut expected);
    points(restored.root(), &mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn baseline_origin_excludes_combining_mark_outline_offsets() {
    let tree = fixture("x&#x301;", 0.);
    let glyphs: Vec<_> = node(&tree)
        .layouted()
        .iter()
        .flat_map(|span| &span.positioned_glyphs)
        .collect();
    assert!(
        glyphs.len() > 1,
        "fixture must have a separately positioned mark"
    );
    let mut has_outline_offset = false;
    for glyph in glyphs {
        let origin = glyph.baseline_origin();
        assert_eq!(origin, crate::tiny_skia_path::Point::from_xy(10.0, 80.0));
        let mut outline = crate::tiny_skia_path::Point::from_xy(0.0, 0.0);
        glyph.transform().map_point(&mut outline);
        has_outline_offset |= outline != origin;
    }
    assert!(has_outline_offset);
}

#[test]
fn affine_pieces_transform_complete_strokes_and_keep_opacity_units_distinct() {
    let tree = fixture_with_attributes("AA", 0., "stroke='red' stroke-width='6'");
    let mut effects = effects(&tree);
    effects[0].transform = crate::Transform::from_scale(2.0, 0.5);
    effects[0].dx = 7.0;
    effects[0].dy = -3.0;
    let changed = tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    let children = node(&changed).flattened().children();
    assert_eq!(children.len(), 2);
    let Node::Group(first) = &children[0] else {
        panic!("expected affine piece")
    };
    assert_eq!(
        first.transform(),
        crate::Transform::from_row(2.0, 0.0, 0.0, 0.5, 7.0, -3.0)
    );
    let Node::Path(path) = &first.children()[0] else {
        panic!("expected original stroke path")
    };
    assert_eq!(path.stroke().unwrap().width().get(), 6.0);
    // Original outline coordinates are retained, so the group scales stroke
    // thickness and paint servers with exactly the same matrix as glyph ink.
    let first_glyph = &node(&tree).layouted()[0].positioned_glyphs[0];
    assert!(path.data().bounds().left() >= first_glyph.baseline_origin().x);
    for effect in &mut effects {
        effect.transform = crate::Transform::from_scale(2.0, 0.5);
        effect.dx = 0.0;
        effect.dy = 0.0;
        effect.opacity = 0.5;
    }
    let faded = tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(node(&faded).flattened().children().len(), 2);
    for child in node(&faded).flattened().children() {
        let Node::Group(group) = child else {
            panic!("expected independent opacity")
        };
        assert_eq!(group.opacity().get(), 0.5);
        assert_eq!(group.transform(), crate::Transform::from_scale(2.0, 0.5));
    }
    effects[1].unit = effects[0].unit;
    let protected = tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(node(&protected).flattened().opacity().get(), 0.5);
}

#[test]
fn affine_rotation_precedes_position_and_preserves_original_paint() {
    let tree = fixture("AA", 0.);
    let mut effects = effects(&tree);
    for effect in &mut effects {
        effect.transform = crate::Transform::from_row(0., 1., -1., 0., 0., 0.);
        effect.dx = 20.;
        effect.dy = 30.;
    }
    let changed = tree
        .clone()
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .unwrap();
    assert_eq!(
        node(&changed).flattened().transform(),
        crate::Transform::from_row(0., 1., -1., 0., 20., 30.)
    );
    let Node::Path(original) = &node(&tree).flattened().children()[0] else {
        panic!()
    };
    let Node::Path(actual) = &node(&changed).flattened().children()[0] else {
        panic!()
    };
    assert_eq!(original.data(), actual.data());
}

#[test]
fn zero_axis_affines_collapse_ink_with_finite_empty_bounds() {
    for (sx, sy) in [(0., 0.), (0., 1.), (1., 0.)] {
        let tree = fixture_with_attributes("AA", 0., "stroke='red' stroke-width='6'");
        let mut effects = effects(&tree);
        for effect in &mut effects {
            effect.transform = crate::Transform::from_scale(sx, sy);
        }
        let changed = tree
            .clone()
            .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
            .unwrap();
        assert!(node(&changed).flattened().children().is_empty());
        assert_eq!(node(&changed).stroke_bounding_box().width(), 0.0);
        assert_eq!(node(&changed).stroke_bounding_box().height(), 0.0);
        let serialized = changed.to_string_with_unique_resource_ids(&WriteOptions::default());
        assert!(!serialized.contains("NaN"));
        assert!(!serialized.contains("inf"));
        Tree::from_str(&serialized, &Options::default()).unwrap();
        effects[1].transform = crate::Transform::default();
        let mixed = tree
            .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
            .unwrap();
        assert_eq!(node(&mixed).flattened().children().len(), 1);
    }
}

#[test]
fn invalid_or_inconsistent_affines_fail_atomically() {
    let tree = fixture("AA", 0.);
    let mut effects = effects(&tree);
    for invalid in [f32::NAN, f32::INFINITY] {
        effects[0].transform = crate::Transform::from_row(invalid, 0., 0., 1., 0., 0.);
        assert!(tree
            .clone()
            .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
            .is_err());
    }
    effects[0].transform = crate::Transform::from_scale(2., 1.);
    effects[1].unit = effects[0].unit;
    assert!(tree
        .with_text_glyph_effects(|_| Ok(Some(effects.clone())))
        .is_err());
}
