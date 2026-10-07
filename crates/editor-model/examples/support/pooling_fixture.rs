//! Independently authored native fixture; no supplied JSX/expression text.
use libre_effects_core::{expression_runtime as ae, *};

pub const SOURCE_BYTES: usize = 3176;
pub fn programs() -> [String; 6] {
    [
        "[thisLayer.index*3+effect('Amount')(1)+thisLayer.opacity,time*10+startTime];",
        "[thisLayer.index*5-effect('Amount')(1)+thisLayer.opacity,time*20-startTime];",
        "[50+thisLayer.index,60+marker.key(1).time];",
        "[70+thisLayer.index,80+marker.key(1).time];",
        "value-thisLayer.index+time;",
        "value-thisLayer.index+time+effect('Amount')(1)/100;",
    ]
    .map(|body| format!("{body}\n/*{}*/", "p".repeat(SOURCE_BYTES - body.len() - 5)))
}
pub fn scene() -> Editor {
    let mut editor = Editor::default();
    editor
        .execute(Command::ConfigureCompositionRate {
            name: "Independent pooled native scene".into(),
            width: 640,
            height: 360,
            fps: 30.into(),
            duration: 300,
            display_start: 0,
        })
        .unwrap();
    let sources = programs();
    for ordinal in 0..64 {
        editor.execute(Command::AddRectangle).unwrap();
        let id = editor.selected().unwrap();
        editor
            .execute(Command::Batch(vec![
                Command::RenameLayer {
                    id,
                    name: format!("Pool layer {ordinal:02}"),
                },
                Command::SetLayerRange {
                    id,
                    start: 100,
                    end: 250,
                },
                Command::SetLayerStart {
                    id,
                    frame: -(ordinal as i64),
                },
                Command::SetValue {
                    id,
                    property: Property::Opacity,
                    frame: 0,
                    value: 80.,
                },
                Command::Effect {
                    id,
                    edit: EffectEdit::Add(EffectKind::SliderControl),
                },
            ]))
            .unwrap();
        let effect = editor
            .project()
            .composition()
            .layer(id)
            .unwrap()
            .effect_stack()[0]
            .id();
        editor
            .execute(Command::Batch(vec![
                Command::Effect {
                    id,
                    edit: EffectEdit::Rename {
                        effect,
                        name: "Amount".into(),
                    },
                },
                Command::Effect {
                    id,
                    edit: EffectEdit::SetValue {
                        effect,
                        parameter: EffectParam::Amount,
                        frame: 0,
                        value: f64::from(ordinal + 10),
                    },
                },
                Command::Marker {
                    target: MarkerTarget::Layer(id),
                    edit: MarkerEdit::Add { frame: ordinal + 1 },
                },
                Command::SetExpression {
                    id,
                    target: ExpressionTarget::Position,
                    source: sources[(ordinal % 2) as usize].clone(),
                    enabled: true,
                },
                Command::SetExpression {
                    id,
                    target: ExpressionTarget::Scale,
                    source: sources[2 + (ordinal % 2) as usize].clone(),
                    enabled: true,
                },
                Command::SetExpression {
                    id,
                    target: ExpressionTarget::Opacity,
                    source: sources[4 + (ordinal % 2) as usize].clone(),
                    enabled: true,
                },
            ]))
            .unwrap();
        editor.clear_history();
    }
    editor
}

pub fn assert_values(project: &Project, frame: Frame, values: &ae::EvaluatedProperties) {
    assert_eq!(values.expression_evaluations, 192);
    let time = f64::from(frame) / 30.;
    for (index, layer) in project.composition().layers().iter().enumerate() {
        let ordinal: u32 = layer
            .name()
            .strip_prefix("Pool layer ")
            .unwrap()
            .parse()
            .unwrap();
        let i = (index + 1) as f64;
        let amount = f64::from(ordinal + 10);
        let origin = -f64::from(ordinal) / 30.;
        let marker = f64::from(ordinal + 1) / 30.;
        let odd = ordinal % 2 == 1;
        let opacity = 80. - i + time + if odd { amount / 100. } else { 0. };
        let position = if odd {
            [i * 5. - amount + opacity, time * 20. - origin]
        } else {
            [i * 3. + amount + opacity, time * 10. + origin]
        };
        let scale = if odd {
            [70. + i, 80. + marker]
        } else {
            [50. + i, 60. + marker]
        };
        for (property, expected) in [
            (
                ae::ExpressionProperty::Position,
                ae::PropertyValue::Vector2(position),
            ),
            (
                ae::ExpressionProperty::Scale,
                ae::PropertyValue::Vector2(scale),
            ),
            (
                ae::ExpressionProperty::Opacity,
                ae::PropertyValue::Scalar(opacity),
            ),
        ] {
            let address = ae::PropertyAddress {
                composition: ae::CompositionId(1),
                layer: ae::LayerId(layer.id()),
                property,
            };
            assert_eq!(values.get(&address), Some(&expected));
        }
    }
}
