//! Independent numeric-expression/paragraph fixture and explicit frame references.
//! Usage: cargo run -p libre-effects-editor-model --example expression_fixture -- DIR
use libre_effects_core::*;
use std::path::Path;

fn set(e: &mut Editor, id: LayerId, p: Property, v: f64) {
    e.execute(Command::SetValue {
        id,
        property: p,
        frame: 0,
        value: v,
    })
    .unwrap();
}
fn program(e: &mut Editor, id: LayerId, target: ExpressionTarget, source: &str) {
    e.execute(Command::SetExpression {
        id,
        target,
        source: source.into(),
        enabled: true,
    })
    .unwrap();
}
fn save(p: &Project, path: &Path) {
    std::fs::write(path, project_file::encode(p, None).unwrap()).unwrap();
}
fn main() {
    let Some(directory) = std::env::args_os().nth(1) else {
        eprintln!("Usage: expression_fixture DIR");
        std::process::exit(2);
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut e = Editor::default();
    e.execute(Command::ConfigureCompositionRate {
        name: "Expression compatibility fixture".into(),
        width: 640,
        height: 360,
        fps: 60.into(),
        duration: 240,
        display_start: 0,
    })
    .unwrap();
    e.execute(Command::AddContent {
        content: Content::Text {
            text: "One\r二\r셋".into(),
            font_size: 32.0,
        },
        width: 180.0,
        height: 140.0,
        name: "Caption".into(),
    })
    .unwrap();
    for name in ["Gap", "Start", "Stay", "End"] {
        e.execute(Command::AddNull).unwrap();
        let id = e.selected().unwrap();
        e.execute(Command::RenameLayer {
            id,
            name: name.into(),
        })
        .unwrap();
        e.execute(Command::ToggleVisible(id)).unwrap();
    }
    set(&mut e, 2, Property::PositionY, 20.0);
    e.execute(Command::Effect {
        id: 2,
        edit: EffectEdit::Add(EffectKind::SliderControl),
    })
    .unwrap();
    e.execute(Command::Effect {
        id: 2,
        edit: EffectEdit::Rename {
            effect: 1,
            name: "Gap amount".into(),
        },
    })
    .unwrap();
    e.execute(Command::Effect {
        id: 2,
        edit: EffectEdit::SetValue {
            effect: 1,
            parameter: EffectParam::Amount,
            frame: 0,
            value: 20.0,
        },
    })
    .unwrap();
    for (i, (frame, name)) in [(0, "Show"), (60, "Focus"), (120, "Hide"), (180, "End")]
        .into_iter()
        .enumerate()
    {
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Add { frame },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Update {
                id: i as u64 + 1,
                frame,
                duration: 0,
                name: name.into(),
                color: 0,
            },
        })
        .unwrap();
    }
    program(
        &mut e,
        3,
        ExpressionTarget::Position,
        "[thisComp.width*0.25,thisComp.height*0.5+thisComp.layer('Gap').effect('Gap amount')(1)]",
    );
    program(
        &mut e,
        4,
        ExpressionTarget::Position,
        "[thisComp.width*0.5,thisComp.height*0.5]",
    );
    program(
        &mut e,
        5,
        ExpressionTarget::Position,
        "[thisComp.width*0.75,thisComp.height*0.5-thisComp.layer('Gap').effect('Gap amount')(1)]",
    );
    program(
        &mut e,
        1,
        ExpressionTarget::Position,
        "var states=[thisComp.layer('Start').transform.position,thisComp.layer('Stay').transform.position,thisComp.layer('End').transform.position];states[time<marker.key(2).time?0:time<marker.key(3).time?1:2]",
    );
    program(
        &mut e,
        1,
        ExpressionTarget::Scale,
        "var s=time<marker.key(2).time?50:time<marker.key(3).time?100:75;[s,s]",
    );
    program(
        &mut e,
        1,
        ExpressionTarget::Opacity,
        "time<marker.key(2).time?25:time<marker.key(3).time?100:50",
    );
    e.execute(Command::SetLayerLabel { id: 1, index: 4 })
        .unwrap();
    let authored = e.project().clone();
    save(&authored, &directory.join("expressions.lep"));
    for frame in [0, 59, 60, 119, 120, 180, 239] {
        let mut reference = Editor::default();
        reference.replace_project(authored.clone()).unwrap();
        for id in [1, 3, 4, 5] {
            for target in [
                ExpressionTarget::Position,
                ExpressionTarget::Scale,
                ExpressionTarget::Opacity,
            ] {
                reference
                    .execute(Command::RemoveExpression { id, target })
                    .unwrap();
            }
        }
        let (x, y, scale, opacity) = if frame < 60 {
            (160.0, 200.0, 50.0, 25.0)
        } else if frame < 120 {
            (320.0, 180.0, 100.0, 100.0)
        } else {
            (480.0, 160.0, 75.0, 50.0)
        };
        for (p, v) in [
            (Property::PositionX, x),
            (Property::PositionY, y),
            (Property::ScaleX, scale),
            (Property::ScaleY, scale),
            (Property::Opacity, opacity),
        ] {
            set(&mut reference, 1, p, v);
        }
        save(
            reference.project(),
            &directory.join(format!("reference-{frame}.lep")),
        );
    }
    let mut nested = Editor::default();
    nested.replace_project(authored.clone()).unwrap();
    nested.execute(Command::NewComposition).unwrap();
    nested
        .execute(Command::ConfigureCompositionRate {
            name: "Nested expression fixture".into(),
            width: 640,
            height: 360,
            fps: 60.into(),
            duration: 300,
            display_start: 0,
        })
        .unwrap();
    nested
        .execute(Command::AddCompositionLayer {
            composition: 1,
            frame: 30,
        })
        .unwrap();
    save(nested.project(), &directory.join("nested.lep"));
    let mut cycle = Editor::default();
    cycle.replace_project(authored.clone()).unwrap();
    program(
        &mut cycle,
        1,
        ExpressionTarget::Position,
        "thisComp.layer('Caption').transform.position",
    );
    save(cycle.project(), &directory.join("cycle.lep"));
    std::fs::write(directory.join("README.txt"),"Synthetic sources only. expressions.lep has six numeric programs, a named slider, four markers and three CR-separated lines. Render frames 0,59,60,119,120,180,239; each must equal reference-FRAME.lep at that frame. nested.lep at90 must equal reference-60.lep at60 (both640x360). cycle.lep must fail visibly and must not create/replace output. Authored references were set from explicit coordinates, not evaluator output.\n").unwrap();
    println!(
        "Created independent expression fixtures in {}",
        directory.display()
    );
}
