use crate::ae_import::*;
use libre_effects_core::{self as core, Property};
use serde_json::{Value, json};
fn source() -> Value {
    let mut p: Value = serde_json::from_slice(include_bytes!(
        "../../ae-project/tests/fixtures/synthetic-project.json"
    ))
    .unwrap();
    p["root_composition_ids"] = json!([101, 202]);
    for comp in p["items"].as_array_mut().unwrap() {
        comp["frame_rate"] = json!({"numerator":30,"denominator":1});
        for layer in comp["layers"].as_array_mut().unwrap() {
            layer["start_time"] = json!({"numerator":1,"denominator":30});
            layer["in_point"] = json!({"numerator":1,"denominator":30});
            for prop in layer["properties"].as_array_mut().unwrap() {
                for key in prop["keys"].as_array_mut().unwrap() {
                    if key["time"]["numerator"] != 0 {
                        key["time"] = json!({"numerator":2,"denominator":1});
                    }
                }
            }
        }
    }
    p
}
fn read(p: &Value) -> ImportDocument {
    read_document(&serde_json::to_vec(p).unwrap()).unwrap()
}
#[test]
fn typed_closure_converts_real_rich_sources_origins_parents_sliders_and_roundtrips() {
    let p = source();
    let input = serde_json::to_vec(&p).unwrap();
    let doc = read_document(&input).unwrap();
    let before = serde_json::to_vec(doc.source()).unwrap();
    let project = doc.convert(101).unwrap();
    assert_eq!(project.active_composition_id(), 101);
    assert_eq!(project.compositions().len(), 2);
    let comp = project.composition();
    assert_eq!(comp.duration(), 300);
    assert_eq!(
        comp.layers().iter().map(|l| l.id()).collect::<Vec<_>>(),
        [1001, 1002, 1003]
    );
    let l = comp.layer(1001).unwrap();
    assert_eq!(l.start_frame(), 1);
    assert_eq!(l.in_frame(), 1);
    assert_eq!(l.out_frame(300), 300);
    assert_eq!(l.parent(), Some(1002));
    assert_eq!(l.label_index(), 4);
    assert_eq!(l.source_text_at(0), Some("A🙂\r\nB"));
    let rich = l.rich_text().unwrap();
    assert_eq!(rich.runs[1].start, 5);
    assert_eq!(rich.runs[1].style.fill_color, 0xff0000);
    assert_eq!(l.effect_stack()[0].name(), "Synthetic amount");
    assert_eq!(
        l.effect_stack()[0].value_at(core::EffectParam::Amount, 0),
        40.
    );
    assert_eq!(
        l.property(Property::Opacity)
            .expect("known scalar fixture property")
            .value_at(30),
        50.
    );
    assert_eq!(l.markers()[0].frame(), 30);
    assert_eq!(l.markers()[0].name(), "Synthetic cue");
    assert!(!comp.layer(1002).unwrap().visible());
    assert_eq!(serde_json::to_vec(doc.source()).unwrap(), before);
    assert_eq!(serde_json::to_vec(&p).unwrap(), input);
    let saved = core::project_file::encode(&project, None).unwrap();
    let decoded = core::project_file::decode(&saved).unwrap();
    assert_eq!(decoded.project, project);
    assert_eq!(
        core::project_file::encode(&decoded.project, decoded.view).unwrap(),
        saved
    );
}
#[test]
fn selected_closure_excludes_unrelated_blockers_and_preserves_ids() {
    let mut p = source();
    p["items"][0]["layers"][0]["three_d"] = json!(true);
    let d = read(&p);
    assert!(d.convert(101).unwrap_err().contains("Three-dimensional"));
    let child = d.convert(202).unwrap();
    assert_eq!(child.compositions().len(), 1);
    assert_eq!(child.composition().layers()[0].id(), 2001);
    assert_eq!(child.composition().layers()[0].color(), 0x0000ff);
    let roots = d.roots();
    assert!(roots[0].blocker.is_some());
    assert!(roots[1].blocker.is_none());
}
#[test]
fn nonrepresentable_timing_colors_and_unknown_metadata_fail_closed() {
    for (edit, expected) in [
        (0, "frame grid"),
        (1, "RGB"),
        (2, "required"),
        (3, "Marker"),
        (4, "baseline"),
        (5, "Bezier"),
    ] {
        let mut p = source();
        match edit {
            0 => p["items"][0]["layers"][0]["start_time"] = json!({"numerator":1,"denominator":31}),
            1 => p["items"][1]["layers"][0]["source"]["color"] = json!([0.1, 0., 1.]),
            2 => p["items"][0]["layers"][0]["properties"][0]["value"] = Value::Null,
            3 => {
                p["items"][0]["layers"][0]["markers"][0]["url"] = json!("https://example.invalid/")
            }
            4 => p["items"][0]["layers"][0]["source"]["document"]["origin"] = json!("ae_baseline"),
            _ => {
                p["items"][0]["layers"][0]["properties"][4]["keys"][0]["in_ease"] =
                    json!([{"speed":1.,"influence":25.}])
            }
        }
        let error = read(&p).convert(101).unwrap_err();
        assert!(error.contains(expected), "{edit}: {error}");
    }
}
#[test]
fn absent_transform_and_unsupported_expression_target_never_use_defaults() {
    let mut p = source();
    p["items"][0]["layers"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(
        read(&p)
            .convert(101)
            .unwrap_err()
            .contains("Explicit Anchor")
    );
    let mut p = source();
    p["items"][0]["layers"][0]["properties"][3]["expression"] =
        json!({"source":"value","enabled":false});
    assert!(
        read(&p)
            .convert(101)
            .unwrap_err()
            .contains("Expressions on this property")
    );
}
#[test]
fn binary_container_never_returns_a_convertible_document() {
    let binary = include_bytes!("../../ae-project/tests/fixtures/synthetic-inventory.rifx");
    assert!(
        read_document(binary)
            .err()
            .unwrap()
            .contains("binary payload schema is not verified")
    );
}

#[test]
fn native_preflight_rejects_ready_roots_before_install_and_never_runs_programs() {
    let mut p = source();
    p["items"][0]["layers"][0]["properties"][4]["expression"] =
        json!({"source":"while (true) {}", "enabled":true});
    let document = read(&p);
    let calls = std::cell::Cell::new(0);
    let roots = document.roots_with_preflight(|project| {
        calls.set(calls.get() + 1);
        if project.active_composition_id() == 101 {
            Err("Synthetic missing exact font".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(calls.get(), 2);
    assert_eq!(
        roots[0].blocker.as_deref(),
        Some("Synthetic missing exact font")
    );
    assert!(roots[1].blocker.is_none());
    assert!(
        document
            .convert(101)
            .unwrap()
            .composition()
            .layer(1001)
            .unwrap()
            .expression(core::ExpressionTarget::Opacity)
            .unwrap()
            .enabled
    );
}

#[test]
fn rich_grapheme_split_is_blocked_before_native_admission_but_equal_styles_merge() {
    let mut p = source();
    let d = &mut p["items"][0]["layers"][0]["source"]["document"];
    d["text"] = json!("e\u{301}");
    d["runs"][0]["start_utf16"] = json!(0);
    d["runs"][0]["end_utf16"] = json!(1);
    d["runs"][1]["start_utf16"] = json!(1);
    d["runs"][1]["end_utf16"] = json!(2);
    let document = read(&p);
    assert!(document.convert(101).unwrap_err().contains("grapheme"));
    assert!(document.roots()[0].blocker.is_some());
    let style = p["items"][0]["layers"][0]["source"]["document"]["runs"][0]["style"].clone();
    p["items"][0]["layers"][0]["source"]["document"]["runs"][1]["style"] = style;
    assert_eq!(
        read(&p)
            .convert(101)
            .unwrap()
            .composition()
            .layer(1001)
            .unwrap()
            .rich_text()
            .unwrap()
            .runs
            .len(),
        1
    );
}
