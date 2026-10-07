//! Independently authored native template. No AEP payload or supplied program source.
use libre_effects_core::*;

pub const LYRIC_COMPOSITION: CompositionId = 1;
pub const NAME_ARTIST_COMPOSITION: CompositionId = 2;
pub const LYRIC_LAYER: LayerId = 1;
pub const LYRIC_MOTION: LayerId = 2;
pub const SONG: LayerId = 3;
pub const PARTNAME: LayerId = 4;
pub const LYRIC_FPS: u32 = 60;
pub const TRANSITION_FRAMES: f64 = 18.0;

#[derive(Clone, Copy, Debug)]
pub struct TemplateOptions {
    pub duration_seconds: u32,
    pub name_artist_fps: u32,
    pub lock_song: bool,
    pub lyric_expressions: bool,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            duration_seconds: 360,
            name_artist_fps: 60,
            lock_song: false,
            lyric_expressions: true,
        }
    }
}

fn set(editor: &mut Editor, id: LayerId, property: Property, value: f64) -> Result<(), String> {
    editor.execute(Command::SetValue {
        id,
        property,
        frame: 0,
        value,
    })
}

fn text(
    editor: &mut Editor,
    id: LayerId,
    name: &str,
    placeholder: &str,
    font_size: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    editor.execute(Command::AddContent {
        content: Content::Text {
            text: placeholder.into(),
            font_size,
        },
        width,
        height,
        name: name.into(),
    })?;
    if editor.selected() != Some(id) {
        return Err(format!("Unexpected native template layer ID for {name}"));
    }
    // Point text uses its top-left source origin. All other typography retains
    // the available native default: Wanted Sans, regular, uniform static text.
    for property in [Property::AnchorX, Property::AnchorY] {
        set(editor, id, property, 0.0)?;
    }
    Ok(())
}

fn expressions(editor: &mut Editor) -> Result<(), String> {
    // These deliberately small native programs establish useful marker motion,
    // rather than reproduce another application's expression or camera defaults.
    // Incomplete or unordered named markers leave the authored source untouched.
    let prefix = r#"var focus=null,hide=null,end=null;
for(var i=1;i<=marker.numKeys;i++){
  var m=marker.key(i);
  if(m.comment==='Focus')focus=m.time;
  if(m.comment==='Hide')hide=m.time;
  if(m.comment==='End')end=m.time;
}
var answer=value;
if(focus!==null&&hide!==null&&end!==null&&focus<=hide&&hide<end){
  var control=thisComp.layer('Lyric Motion');
  var span=Math.max(framesToTime(control.effect('Transition Frames')(1)),thisComp.frameDuration);
  var enter=Math.max(0,Math.min(1,(time-focus)/span));
  var leave=Math.max(0,Math.min(1,(time-hide)/(end-hide)));
"#;
    for (target, body) in [
        (
            ExpressionTarget::Position,
            "var p=control.transform.position;answer=[p[0],p[1]+24*(1-enter)+16*leave];",
        ),
        (
            ExpressionTarget::Scale,
            "var size=86+14*enter-8*leave;answer=[size,size];",
        ),
        (ExpressionTarget::Opacity, "answer=100*enter*(1-leave);"),
    ] {
        editor.execute(Command::SetExpression {
            id: LYRIC_LAYER,
            target,
            source: format!("{prefix}{body}\n}}\nanswer;"),
            enabled: true,
        })?;
    }
    Ok(())
}

/// Build the public 360-second setup or a bounded harness variant.
///
/// IDs are stable and exported above. The returned editor has Lyric active and
/// an empty Undo/Redo history. A 40-second duration, 30fps second composition or
/// locked Song can be selected without mutating the public baseline fixture.
pub fn build(options: TemplateOptions) -> Result<Editor, String> {
    if !(13..=3_600).contains(&options.duration_seconds) {
        return Err("Template duration must be between 13 and 3600 seconds".into());
    }
    if ![30, 60].contains(&options.name_artist_fps) {
        return Err("Name & Artist supports the 30fps discriminator or 60fps baseline".into());
    }
    let mut editor = Editor::default();
    editor.execute(Command::ConfigureCompositionRate {
        name: "Lyric".into(),
        width: 960,
        height: 540,
        fps: LYRIC_FPS.into(),
        duration: options.duration_seconds * LYRIC_FPS,
        display_start: 0,
    })?;
    editor.execute(Command::SetCompositionBackground(0x151a27))?;
    text(
        &mut editor,
        LYRIC_LAYER,
        "LyricLayer",
        "Your lyric line",
        48.0,
        800.0,
        120.0,
    )?;
    set(&mut editor, LYRIC_LAYER, Property::PositionX, 80.0)?;
    set(&mut editor, LYRIC_LAYER, Property::PositionY, 240.0)?;
    // Trim before shifting the origin. Moving an implicit composition-length
    // source would otherwise push its outPoint beyond the composition boundary.
    editor.execute(Command::SetLayerRange {
        id: LYRIC_LAYER,
        start: 0,
        end: 12 * LYRIC_FPS,
    })?;
    editor.execute(Command::SetLayerStart {
        id: LYRIC_LAYER,
        frame: 1,
    })?;
    editor.execute(Command::ToggleVisible(LYRIC_LAYER))?;

    editor.execute(Command::AddNull)?;
    if editor.selected() != Some(LYRIC_MOTION) {
        return Err("Unexpected native motion-control ID".into());
    }
    editor.execute(Command::RenameLayer {
        id: LYRIC_MOTION,
        name: "Lyric Motion".into(),
    })?;
    set(&mut editor, LYRIC_MOTION, Property::PositionX, 80.0)?;
    set(&mut editor, LYRIC_MOTION, Property::PositionY, 240.0)?;
    editor.execute(Command::SetLayerRange {
        id: LYRIC_MOTION,
        start: 0,
        end: 12 * LYRIC_FPS,
    })?;
    editor.execute(Command::ToggleVisible(LYRIC_MOTION))?;
    editor.execute(Command::Effect {
        id: LYRIC_MOTION,
        edit: EffectEdit::Add(EffectKind::SliderControl),
    })?;
    editor.execute(Command::Effect {
        id: LYRIC_MOTION,
        edit: EffectEdit::Rename {
            effect: 1,
            name: "Transition Frames".into(),
        },
    })?;
    editor.execute(Command::Effect {
        id: LYRIC_MOTION,
        edit: EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
            value: TRANSITION_FRAMES,
        },
    })?;
    if options.lyric_expressions {
        expressions(&mut editor)?;
    }

    editor.execute(Command::NewComposition)?;
    if editor.project().active_composition_id() != NAME_ARTIST_COMPOSITION {
        return Err("Unexpected native second-composition ID".into());
    }
    editor.execute(Command::ConfigureCompositionRate {
        name: "Name & Artist".into(),
        width: 960,
        height: 160,
        fps: options.name_artist_fps.into(),
        duration: options.duration_seconds * options.name_artist_fps,
        display_start: 0,
    })?;
    editor.execute(Command::SetCamera {
        camera: Some(Camera3 {
            position: [480.0, 80.0, -960.0],
            focal_distance: 960.0,
            principal_point: [480.0, 80.0],
            near_clip: 1.0,
        }),
    })?;
    text(
        &mut editor,
        SONG,
        "Song",
        "Song title / Artist",
        36.0,
        832.0,
        52.0,
    )?;
    text(
        &mut editor,
        PARTNAME,
        "Partname",
        "Section name",
        28.0,
        832.0,
        44.0,
    )?;
    for id in [SONG, PARTNAME] {
        // Establish identity local transforms before creating the hierarchy.
        set(&mut editor, id, Property::PositionX, 0.0)?;
        set(&mut editor, id, Property::PositionY, 0.0)?;
        editor.execute(Command::SetLayerRange {
            id,
            start: 0,
            end: if id == SONG {
                // The persistent visible parent receives keys throughout the
                // composition. Only the disabled Partname source is shifted.
                options.duration_seconds * options.name_artist_fps
            } else {
                6 * options.name_artist_fps
            },
        })?;
        editor.execute(Command::SetThreeD { id, enabled: true })?;
    }
    editor.execute(Command::SetSpatialParent {
        id: PARTNAME,
        parent: Some(SONG),
    })?;
    for (id, value) in [(SONG, [64.0, 36.0, 0.0]), (PARTNAME, [0.0, 64.0, 0.0])] {
        editor.execute(Command::SetSpatialPosition {
            id,
            edit: SpatialEdit::Value(value),
        })?;
    }
    editor.execute(Command::ToggleVisible(PARTNAME))?;
    if options.lock_song {
        editor.execute(Command::ToggleLocked(SONG))?;
    }
    editor.activate_composition(LYRIC_COMPOSITION)?;
    editor.select(LYRIC_LAYER);
    editor.clear_history();
    project_file::encode(editor.project(), None)?;
    editor.project().validate_spatial_animation()?;
    Ok(editor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::expression_runtime as ae;

    #[test]
    fn public_fixture_matches_builder_and_has_short_explicit_uniform_sources() {
        let editor = build(TemplateOptions::default()).unwrap();
        let project = editor.project();
        let bytes = project_file::encode(project, None).unwrap();
        // The public fixture predates the current struct-field serialization
        // order. JSON object order is not document content: compare the complete
        // decoded project, then verify the current encoder's deterministic bytes.
        let fixture = project_file::decode(include_bytes!(
            "../../../../examples/native-lyric-part-template.lep"
        ))
        .unwrap();
        assert_eq!(fixture.project, *project);
        assert_eq!(project_file::decode(&bytes).unwrap().project, *project);
        assert_eq!(project_file::encode(&fixture.project, None).unwrap(), bytes);
        assert_eq!(project.active_composition_id(), LYRIC_COMPOSITION);
        assert!(!editor.can_undo() && !editor.can_redo());
        let lyric = project.composition_by_id(LYRIC_COMPOSITION).unwrap();
        let names = project.composition_by_id(NAME_ARTIST_COMPOSITION).unwrap();
        for comp in [lyric, names] {
            assert_eq!(comp.fps(), 60.into());
            assert_eq!(comp.duration(), 21_600);
            for layer in comp.layers() {
                assert!(layer.markers().is_empty());
                assert_eq!(
                    layer.out_frame(comp.duration()) == comp.duration(),
                    layer.id() == SONG
                );
                assert!(layer.rich_text().is_none());
                assert!(layer.source_text_animation().is_default());
            }
        }
        let source = lyric.layer(LYRIC_LAYER).unwrap();
        assert_eq!(source.name(), "LyricLayer");
        assert!(!source.visible() && !source.locked());
        assert_eq!(source.start_frame(), 1);
        assert_eq!(source.in_frame(), 1);
        assert_eq!(source.out_frame(lyric.duration()), 721);
        let part = names.layer(PARTNAME).unwrap();
        assert_eq!(part.name(), "Partname");
        assert!(!part.visible() && !part.locked());
        assert_eq!(part.start_frame(), 0);
        assert_eq!(part.in_frame(), 0);
        assert_eq!(part.out_frame(names.duration()), 360);
        assert_eq!(part.parent(), Some(SONG));
        assert!(names.layers().iter().all(|layer| {
            layer.is_three_d() && layer.expressions().iter().all(|program| !program.enabled)
        }));
        assert!(names.layer(SONG).unwrap().visible());
        assert_eq!(
            names.layer(SONG).unwrap().out_frame(names.duration()),
            names.duration()
        );
        assert_eq!(
            names.projected_geometry(SONG, 0).unwrap().corners[0],
            [64.0, 36.0]
        );
        assert_eq!(
            names.projected_geometry(PARTNAME, 0).unwrap().corners[0],
            [64.0, 100.0]
        );
        assert_eq!(names.render_order(0, false).unwrap(), vec![SONG]);
        assert_eq!(names.render_order(1_200, false).unwrap(), vec![SONG]);
        let raw: serde_json::Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
        for layer in raw["composition"]["layers"].as_array().unwrap() {
            assert!(layer["out_frame"].is_number());
        }
        for layer in raw["other_compositions"]["2"]["layers"].as_array().unwrap() {
            assert!(layer["out_frame"].is_number());
            assert_eq!(
                layer["transform_offset"],
                serde_json::json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
            );
        }
    }

    fn sample(project: &Project, frame: Frame) -> [Vec<f64>; 3] {
        let properties = [
            ae::ExpressionProperty::Position,
            ae::ExpressionProperty::Scale,
            ae::ExpressionProperty::Opacity,
        ];
        let roots = properties.map(|property| ae::PropertyAddress {
            composition: ae::CompositionId(LYRIC_COMPOSITION),
            layer: ae::LayerId(LYRIC_LAYER),
            property,
        });
        let snapshot = project
            .expression_snapshot(LYRIC_COMPOSITION, frame)
            .unwrap();
        let values = ae::ExpressionEvaluator::default()
            .evaluate(&snapshot, &roots)
            .unwrap();
        roots.map(|address| match values.get(&address).unwrap() {
            ae::PropertyValue::Scalar(value) => vec![*value],
            ae::PropertyValue::Vector2(value) => value.to_vec(),
            ae::PropertyValue::Vector3(_) => {
                panic!("Lyric expressions must remain two-dimensional")
            }
            ae::PropertyValue::Text(_) | ae::PropertyValue::Path(_) => {
                panic!("Lyric transform expressions must return numeric values")
            }
        })
    }

    fn marker(editor: &mut Editor, frame: Frame, name: &str) {
        editor
            .execute(Command::Marker {
                target: MarkerTarget::Layer(LYRIC_LAYER),
                edit: MarkerEdit::Add { frame },
            })
            .unwrap();
        let id = editor
            .project()
            .composition()
            .layer(LYRIC_LAYER)
            .unwrap()
            .markers()
            .iter()
            .find(|marker| marker.frame() == frame)
            .unwrap()
            .id();
        editor
            .execute(Command::Marker {
                target: MarkerTarget::Layer(LYRIC_LAYER),
                edit: MarkerEdit::Update {
                    id,
                    frame,
                    duration: 0,
                    name: name.into(),
                    color: 0,
                },
            })
            .unwrap();
    }

    #[test]
    fn native_programs_require_named_markers_and_sample_independent_motion() {
        let mut editor = build(TemplateOptions {
            duration_seconds: 40,
            ..Default::default()
        })
        .unwrap();
        let authored = [vec![80.0, 240.0], vec![100.0, 100.0], vec![100.0]];
        assert_eq!(sample(editor.project(), 90), authored);
        marker(&mut editor, 60, "Focus");
        assert_eq!(sample(editor.project(), 90), authored);
        marker(&mut editor, 240, "Hide");
        assert_eq!(sample(editor.project(), 90), authored);
        marker(&mut editor, 300, "End");
        // Unrelated earlier/later markers discriminate named ownership from
        // merely indexing the first and last chronological markers.
        marker(&mut editor, 30, "Unrelated before");
        marker(&mut editor, 350, "Unrelated after");
        for (frame, y, scale, opacity) in [
            (60, 264.0, 86.0, 0.0),
            (69, 252.0, 93.0, 50.0),
            (78, 240.0, 100.0, 100.0),
            (270, 248.0, 96.0, 50.0),
            (300, 256.0, 92.0, 0.0),
        ] {
            let actual = sample(editor.project(), frame);
            let expected = [vec![80.0, y], vec![scale, scale], vec![opacity]];
            for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
                assert!(
                    (actual - expected).abs() < 1e-9,
                    "frame {frame}: {actual} != {expected}"
                );
            }
        }
    }

    #[test]
    fn short_mixed_fps_locked_variant_remains_distinct_from_public_baseline() {
        let mut editor = build(TemplateOptions {
            duration_seconds: 40,
            name_artist_fps: 30,
            lock_song: true,
            lyric_expressions: false,
        })
        .unwrap();
        assert_eq!(editor.project().composition().duration(), 2_400);
        assert!(
            editor
                .project()
                .composition()
                .layers()
                .iter()
                .all(|layer| layer.expressions().is_empty())
        );
        editor
            .activate_composition(NAME_ARTIST_COMPOSITION)
            .unwrap();
        let comp = editor.project().composition();
        assert_eq!(comp.fps(), 30.into());
        assert_eq!(comp.duration(), 1_200);
        assert_eq!(
            comp.layer(PARTNAME).unwrap().out_frame(comp.duration()),
            180
        );
        assert!(comp.layer(SONG).unwrap().locked());
        assert_eq!(comp.layer(SONG).unwrap().out_frame(comp.duration()), 1_200);
        let before = editor.project().clone();
        assert!(
            editor
                .execute(Command::SetSpatialPosition {
                    id: SONG,
                    edit: SpatialEdit::Value([1.0, 2.0, 3.0]),
                })
                .is_err()
        );
        assert_eq!(*editor.project(), before);
        assert!(!editor.can_undo());
        assert!(
            build(TemplateOptions {
                duration_seconds: 12,
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            build(TemplateOptions {
                name_artist_fps: 24,
                ..Default::default()
            })
            .is_err()
        );
    }
}
