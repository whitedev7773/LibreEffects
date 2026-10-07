use libre_effects_ae_project::*;

const JSON: &[u8] = include_bytes!("fixtures/synthetic-project.json");
fn fixture() -> AeProject {
    serde_json::from_slice(JSON).unwrap()
}
fn comp(project: &mut AeProject) -> &mut Composition {
    match &mut project.items[0] {
        Item::Composition(value) => value,
        _ => unreachable!(),
    }
}
fn text(project: &mut AeProject) -> &mut TextDocument {
    match &mut comp(project).layers[0].source {
        LayerSource::Text { document, .. } => document,
        _ => unreachable!(),
    }
}
fn rejected(project: AeProject, code: DiagnosticCode) {
    let error = validate_project(project, &Limits::default()).unwrap_err();
    assert_eq!(error.diagnostics[0].code, code, "{error}");
}
fn reject_limit(limits: Limits) {
    let error = parse_json(JSON, &limits).unwrap_err();
    assert_eq!(
        error.diagnostics[0].code,
        DiagnosticCode::ResourceLimit,
        "{error}"
    );
}

#[test]
fn exact_typed_roundtrip_preserves_rich_text_ids_times_and_provenance() {
    let project = parse_json(JSON, &Limits::default()).unwrap();
    let encoded = serde_json::to_vec(project.project()).unwrap();
    assert_eq!(
        parse_json(&encoded, &Limits::default()).unwrap().project(),
        project.project()
    );
    let comp = project.composition(101).unwrap();
    assert_eq!(comp.name, "Synthetic timing board");
    assert_eq!(
        comp.frame_duration(),
        RationalTime {
            numerator: 1001,
            denominator: 30000
        }
    );
    assert_eq!(
        comp.layers.iter().map(|layer| layer.id).collect::<Vec<_>>(),
        [1001, 1002, 1003]
    );
    assert_eq!(
        comp.layers[0].start_time,
        RationalTime {
            numerator: -1,
            denominator: 2
        }
    );
    assert_eq!(comp.layers[0].in_point.numerator, 0);
    assert_eq!(comp.layers[0].label, 4);
    assert_eq!(comp.layers[0].parent_id, Some(1002));
    assert!(!comp.layers[1].enabled);
    let LayerSource::Text { document, .. } = &comp.layers[0].source else {
        panic!()
    };
    assert_eq!(document.text.as_bytes(), "A🙂\r\nB".as_bytes());
    assert_eq!(document.runs[1].start_utf16, 3);
    assert_eq!(document.runs[1].style.font_size, 18.0);
    assert_eq!(
        document.default_style.font.postscript_name,
        "LiberationSans"
    );
    assert_eq!(comp.layers[0].sliders[0].name, "Synthetic amount");
    assert_eq!(
        comp.layers[0].properties[4]
            .expression
            .as_ref()
            .unwrap()
            .source,
        "value"
    );
}

#[test]
fn closure_is_dependency_first_and_never_name_based() {
    let mut source = fixture();
    let name = comp(&mut source).name.clone();
    let Item::Composition(child) = &mut source.items[1] else {
        panic!()
    };
    child.name = name;
    let project = validate_project(source, &Limits::default()).unwrap();
    let closure = project.composition_closure(101).unwrap();
    assert_eq!(closure.item_ids, [202, 101]);
    assert_eq!(closure.composition_ids, [202, 101]);
    assert!(closure.is_supported());
    assert_eq!(
        project.composition_closure(999).unwrap_err().diagnostics[0].code,
        DiagnosticCode::DanglingReference
    );
}

#[test]
fn unsupported_is_local_to_selected_closure_but_global_features_block_all() {
    let mut source = fixture();
    source.items.push(Item::Unsupported(UnsupportedItem {
        id: 900,
        name: "Unrelated camera".into(),
        dependency_ids: vec![],
        unsupported: vec![UnsupportedFeature {
            code: "camera".into(),
            detail: "No 3D renderer".into(),
        }],
    }));
    let validated = validate_project(source.clone(), &Limits::default()).unwrap();
    assert!(validated.composition_closure(101).unwrap().is_supported());
    source.unsupported.push(UnsupportedFeature {
        code: "color_management".into(),
        detail: "Unknown rendering profile".into(),
    });
    assert!(
        !validate_project(source, &Limits::default())
            .unwrap()
            .composition_closure(101)
            .unwrap()
            .is_supported()
    );
}

#[test]
fn explicit_layer_property_and_text_unknowns_block_closure() {
    for site in 0..4 {
        let mut source = fixture();
        let feature = UnsupportedFeature {
            code: "synthetic_unknown".into(),
            detail: "Test unresolved semantics".into(),
        };
        match site {
            0 => comp(&mut source).layers[0].unsupported.push(feature),
            1 => comp(&mut source).layers[0].properties[0]
                .unsupported
                .push(feature),
            2 => comp(&mut source).layers[0].sliders[0]
                .property
                .unsupported
                .push(feature),
            _ => text(&mut source).unsupported.push(feature),
        }
        let closure = validate_project(source, &Limits::default())
            .unwrap()
            .composition_closure(101)
            .unwrap();
        assert_eq!(closure.diagnostics.len(), 1);
        assert_eq!(
            closure.diagnostics[0].code,
            DiagnosticCode::UnsupportedFeature
        );
    }
}

#[test]
fn three_d_is_preserved_and_blocked() {
    let mut source = fixture();
    comp(&mut source).layers[0].three_d = true;
    let valid = validate_project(source, &Limits::default()).unwrap();
    assert!(valid.composition(101).unwrap().layers[0].three_d);
    assert!(!valid.composition_closure(101).unwrap().is_supported());
}

#[test]
fn duplicate_item_layer_slider_property_and_root_ids_reject() {
    let mut source = fixture();
    source.items.push(source.items[0].clone());
    rejected(source, DiagnosticCode::DuplicateId);
    let mut source = fixture();
    comp(&mut source).layers[2].id = 1002;
    rejected(source, DiagnosticCode::DuplicateId);
    let mut source = fixture();
    let slider = comp(&mut source).layers[0].sliders[0].clone();
    comp(&mut source).layers[0].sliders.push(slider);
    rejected(source, DiagnosticCode::DuplicateId);
    let mut source = fixture();
    let prop = comp(&mut source).layers[0].properties[0].clone();
    comp(&mut source).layers[0].properties.push(prop);
    rejected(source, DiagnosticCode::DuplicateId);
    let mut source = fixture();
    source.root_composition_ids.push(101);
    rejected(source, DiagnosticCode::DuplicateId);
}

#[test]
fn cross_composition_duplicate_layer_ids_reject() {
    let mut source = fixture();
    let Item::Composition(child) = &mut source.items[1] else {
        panic!()
    };
    child.layers[0].id = 1001;
    rejected(source, DiagnosticCode::DuplicateId);
}

#[test]
fn dangling_parent_source_and_root_reject() {
    let mut source = fixture();
    comp(&mut source).layers[0].parent_id = Some(2001);
    rejected(source, DiagnosticCode::DanglingReference);
    let mut source = fixture();
    comp(&mut source).layers[2].source = LayerSource::Composition { item_id: 999 };
    rejected(source, DiagnosticCode::DanglingReference);
    let mut source = fixture();
    source.root_composition_ids[0] = 999;
    rejected(source, DiagnosticCode::DanglingReference);
}

#[test]
fn parent_and_item_cycles_reject() {
    let mut source = fixture();
    comp(&mut source).layers[1].parent_id = Some(1001);
    rejected(source, DiagnosticCode::CyclicReference);
    let mut source = fixture();
    comp(&mut source).layers[2].source = LayerSource::Composition { item_id: 101 };
    rejected(source, DiagnosticCode::CyclicReference);
    let mut source = fixture();
    let Item::Composition(child) = &mut source.items[1] else {
        panic!()
    };
    child.layers[0].source = LayerSource::Composition { item_id: 101 };
    rejected(source, DiagnosticCode::CyclicReference);
}

#[test]
fn unseen_shared_dependency_depth_is_still_bounded() {
    let mut source = fixture();
    let mut first = source.items[1].clone();
    let Item::Composition(ref mut first_comp) = first else {
        panic!()
    };
    first_comp.id = 1;
    first_comp.layers.clear();
    let mut second = first.clone();
    let Item::Composition(ref mut second_comp) = second else {
        panic!()
    };
    second_comp.id = 2;
    let mut layer = comp(&mut source).layers[2].clone();
    layer.id = 3001;
    layer.source = LayerSource::Composition { item_id: 1 };
    second_comp.layers.push(layer);
    comp(&mut source).layers[2].source = LayerSource::Composition { item_id: 2 };
    source.items.extend([first, second]);
    let limits = Limits {
        max_dependency_depth: 2,
        ..Limits::default()
    };
    assert_eq!(
        validate_project(source, &limits).unwrap_err().diagnostics[0].code,
        DiagnosticCode::ResourceLimit
    );
}

#[test]
fn utf16_runs_reject_surrogate_split_gap_overlap_and_incomplete_coverage() {
    for (start, end) in [(0, 2), (1, 3), (0, 4), (0, 1), (0, 99)] {
        let mut source = fixture();
        let run = &mut text(&mut source).runs[0];
        run.start_utf16 = start;
        run.end_utf16 = end;
        rejected(source, DiagnosticCode::InvalidValue);
    }
    let mut source = fixture();
    text(&mut source).runs.pop();
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn empty_default_styled_text_is_valid_but_empty_run_is_not() {
    let mut source = fixture();
    text(&mut source).text.clear();
    text(&mut source).runs.clear();
    validate_project(source, &Limits::default()).unwrap();
    let mut source = fixture();
    text(&mut source).runs[0].end_utf16 = 0;
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn nonfinite_direct_values_and_json_overflow_reject() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut source = fixture();
        comp(&mut source).layers[0].properties[0].value = Some(vec![value, 0.]);
        rejected(source, DiagnosticCode::InvalidValue);
        let mut source = fixture();
        text(&mut source).default_style.font_size = value;
        rejected(source, DiagnosticCode::InvalidValue);
    }
    let json = String::from_utf8(JSON.to_vec())
        .unwrap()
        .replacen("24.0", "1e400", 1);
    assert_eq!(
        parse_json(json.as_bytes(), &Limits::default())
            .unwrap_err()
            .diagnostics[0]
            .code,
        DiagnosticCode::InvalidJson
    );
}

#[test]
fn exact_equivalent_key_times_and_unsorted_keys_reject() {
    let mut source = fixture();
    let keys = &mut comp(&mut source).layers[0].properties[4].keys;
    keys[0].time = RationalTime {
        numerator: 1,
        denominator: 30,
    };
    keys[1].time = RationalTime {
        numerator: 2,
        denominator: 60,
    };
    rejected(source, DiagnosticCode::InvalidValue);
    let mut source = fixture();
    comp(&mut source).layers[0].properties[4].keys.reverse();
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn invalid_time_ratio_geometry_and_vector_reject() {
    let mut source = fixture();
    comp(&mut source).duration.denominator = 0;
    rejected(source, DiagnosticCode::InvalidValue);
    let mut source = fixture();
    comp(&mut source).frame_rate.numerator = 0;
    rejected(source, DiagnosticCode::InvalidValue);
    let mut source = fixture();
    comp(&mut source).width = 0;
    rejected(source, DiagnosticCode::InvalidValue);
    let mut source = fixture();
    comp(&mut source).layers[0].out_point.numerator = 0;
    rejected(source, DiagnosticCode::InvalidValue);
    let mut source = fixture();
    comp(&mut source).layers[0].properties[0].value = Some(vec![0.]);
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn missing_numeric_base_and_ease_remain_absent() {
    let mut source = fixture();
    comp(&mut source).layers[0].properties[4].value = None;
    let valid = validate_project(source, &Limits::default()).unwrap();
    let property = &valid.composition(101).unwrap().layers[0].properties[4];
    assert_eq!(property.value, None);
    assert_eq!(property.keys[0].in_ease, None);
}

#[test]
fn ease_is_explicit_preserved_and_bounded() {
    let mut source = fixture();
    let key = &mut comp(&mut source).layers[0].properties[4].keys[0];
    key.out_interpolation = Interpolation::Bezier;
    key.out_ease = Some(vec![KeyframeEase {
        speed: -12.5,
        influence: 33.333,
    }]);
    let valid = validate_project(source.clone(), &Limits::default()).unwrap();
    assert_eq!(
        valid.composition(101).unwrap().layers[0].properties[4].keys[0]
            .out_ease
            .as_ref()
            .unwrap()[0]
            .speed,
        -12.5
    );
    comp(&mut source).layers[0].properties[4].keys[0]
        .out_ease
        .as_mut()
        .unwrap()[0]
        .influence = 101.;
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn schema_unknown_and_duplicate_json_fields_reject() {
    let original = String::from_utf8(JSON.to_vec()).unwrap();
    for json in [
        original.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"future_semantics\": true,",
            1,
        ),
        original.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"schema_version\": 1,",
            1,
        ),
        original.replacen(
            "\"font_size\": 24.0,",
            "\"font_size\": 24.0, \"future_glyph_warp\": 2,",
            1,
        ),
        original.replacen(
            "\"kind\": \"null\",",
            "\"kind\": \"null\", \"unknown_null_behavior\": true,",
            1,
        ),
        original.replacen(
            "\"enabled\": true,",
            "\"enabled\": true, \"enabled\": false,",
            1,
        ),
    ] {
        assert_eq!(
            parse_json(json.as_bytes(), &Limits::default())
                .unwrap_err()
                .diagnostics[0]
                .code,
            DiagnosticCode::InvalidJson
        );
    }
    let mut source = fixture();
    source.schema_version = 2;
    rejected(source, DiagnosticCode::UnsupportedSchema);
}

#[test]
fn every_cumulative_resource_budget_is_enforced() {
    reject_limit(Limits {
        max_file_bytes: JSON.len() - 1,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_items: 1,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_layers: 3,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_properties: 20,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_links: 0,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_keys: 1,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_markers: 0,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_runs: 1,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_programs: 0,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_program_bytes: 4,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_total_program_bytes: 4,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_string_bytes: 8,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_total_string_bytes: 20,
        ..Limits::default()
    });
    reject_limit(Limits {
        max_dependency_depth: 1,
        ..Limits::default()
    });
}

#[test]
fn markers_retain_metadata_and_duplicate_times_reject() {
    let mut source = fixture();
    let marker = &mut comp(&mut source).layers[0].markers[0];
    marker.chapter = "Chapter".into();
    marker.parameters.push(MarkerParameter {
        name: "key".into(),
        value: "value".into(),
    });
    let valid = validate_project(source.clone(), &Limits::default()).unwrap();
    assert_eq!(
        valid.composition(101).unwrap().layers[0].markers[0].chapter,
        "Chapter"
    );
    let duplicate = comp(&mut source).layers[0].markers[0].clone();
    comp(&mut source).layers[0].markers.push(duplicate);
    rejected(source, DiagnosticCode::InvalidValue);
}

#[test]
fn footage_is_only_metadata_and_never_read_or_convertible() {
    let mut source = fixture();
    source.items.push(Item::Footage(Footage {
        id: 700,
        name: "Missing media".into(),
        source_path: "/no/such/file.mov".into(),
        width: 320,
        height: 180,
        pixel_aspect: FrameRate {
            numerator: 1,
            denominator: 1,
        },
        frame_rate: None,
        duration: None,
        unsupported: vec![],
    }));
    comp(&mut source).layers[2].source = LayerSource::Footage { item_id: 700 };
    let closure = validate_project(source, &Limits::default())
        .unwrap()
        .composition_closure(101)
        .unwrap();
    assert_eq!(closure.item_ids, [700, 101]);
    assert!(!closure.is_supported());
}

#[test]
fn numeric_time_comparison_handles_extremes_without_overflow() {
    let a = RationalTime {
        numerator: i64::MAX,
        denominator: u32::MAX,
    };
    let b = RationalTime {
        numerator: i64::MIN,
        denominator: u32::MAX,
    };
    assert!(a.compare(b).is_gt());
    assert!(
        RationalTime {
            numerator: 1,
            denominator: 2
        }
        .compare(RationalTime {
            numerator: 2,
            denominator: 4
        })
        .is_eq()
    );
}

// These are independent JSON field-presence checks, not Rust construction tests:
// constructing an Option::None in Rust is already an explicit producer choice.
const REQUIRED_NULLABLE_FIELDS: &[(&str, &str)] = &[
    ("/items/0/layers/0", "parent_id"),
    ("/items/0/layers/0/properties/4", "value"),
    ("/items/0/layers/0/properties/4", "expression"),
    ("/items/0/layers/0/properties/4/keys/0", "in_ease"),
    ("/items/0/layers/0/properties/4/keys/0", "out_ease"),
    (
        "/items/0/layers/0/source/document/default_style",
        "fill_rgb",
    ),
    (
        "/items/0/layers/0/source/document/default_style",
        "stroke_rgb",
    ),
    ("/items/0/layers/0/source/document/runs/0/style", "fill_rgb"),
    (
        "/items/0/layers/0/source/document/runs/0/style",
        "stroke_rgb",
    ),
    ("/items/0/layers/0/source/document/paragraph", "box_size"),
    ("/items/2", "frame_rate"),
    ("/items/2", "duration"),
];

fn nullable_fixture() -> serde_json::Value {
    let mut source: serde_json::Value = serde_json::from_slice(JSON).unwrap();
    source["items"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "kind": "footage",
            "id": 700,
            "name": "Synthetic nullable metadata",
            "source_path": "/not-opened/synthetic.mov",
            "width": 320,
            "height": 180,
            "pixel_aspect": {"numerator": 1, "denominator": 1},
            "frame_rate": {"numerator": 24, "denominator": 1},
            "duration": {"numerator": 10, "denominator": 1},
            "unsupported": []
        }));
    source
}

#[test]
fn omitted_semantic_nullable_field_matrix_rejects() {
    let original = nullable_fixture();
    parse_json(&serde_json::to_vec(&original).unwrap(), &Limits::default()).unwrap();
    for &(parent, field) in REQUIRED_NULLABLE_FIELDS {
        let mut source = original.clone();
        assert!(
            source
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field)
                .is_some()
        );
        let error = parse_json(&serde_json::to_vec(&source).unwrap(), &Limits::default())
            .expect_err(&format!("Omitted {parent}/{field} must reject"));
        assert_eq!(
            error.diagnostics[0].code,
            DiagnosticCode::InvalidJson,
            "{parent}/{field}: {error}"
        );
        assert!(
            error
                .to_string()
                .contains(&format!("missing field `{field}`")),
            "{parent}/{field}: {error}"
        );
    }
}

#[test]
fn explicit_null_semantic_field_matrix_is_accepted_and_preserved() {
    let original = nullable_fixture();
    for &(parent, field) in REQUIRED_NULLABLE_FIELDS {
        let mut source = original.clone();
        source.pointer_mut(parent).unwrap()[field] = serde_json::Value::Null;
        let validated = parse_json(&serde_json::to_vec(&source).unwrap(), &Limits::default())
            .unwrap_or_else(|error| {
                panic!("Explicit null at {parent}/{field} must be accepted: {error}")
            });
        let output = serde_json::to_value(validated.project()).unwrap();
        assert_eq!(
            output.pointer(&format!("{parent}/{field}")),
            Some(&serde_json::Value::Null),
            "{parent}/{field}"
        );
    }
}

#[test]
fn source_digest_is_the_only_omittable_option_metadata() {
    let mut source = nullable_fixture();
    source["provenance"]
        .as_object_mut()
        .unwrap()
        .remove("source_sha256");
    let validated = parse_json(&serde_json::to_vec(&source).unwrap(), &Limits::default()).unwrap();
    assert_eq!(validated.project().provenance.source_sha256, None);
}
