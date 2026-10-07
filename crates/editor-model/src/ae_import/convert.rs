use libre_effects_ae_project as ae;
use libre_effects_core as core;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn value<T: serde::Serialize>(x: T) -> Result<Value, String> {
    serde_json::to_value(x).map_err(|e| e.to_string())
}
fn fail<T>(path: &str, message: &str) -> Result<T, String> {
    Err(format!("{path}: {message}"))
}
fn frame(time: ae::RationalTime, rate: ae::FrameRate, path: &str) -> Result<i64, String> {
    let n = i128::from(time.numerator) * i128::from(rate.numerator);
    let d = i128::from(time.denominator) * i128::from(rate.denominator);
    if d == 0 || n % d != 0 {
        return fail(
            path,
            "Time must lie exactly on this composition's frame grid; snapping is not implicit",
        );
    }
    i64::try_from(n / d).map_err(|_| format!("{path}: frame time overflow"))
}
fn unsigned_frame(time: ae::RationalTime, rate: ae::FrameRate, path: &str) -> Result<u32, String> {
    u32::try_from(frame(time, rate, path)?)
        .map_err(|_| format!("{path}: negative or oversized frame is not supported"))
}
fn rgb(c: [f64; 3]) -> Result<u32, String> {
    let mut result = 0;
    for channel in c {
        let n = channel * 255.;
        if !n.is_finite() || !(0.0..=255.0).contains(&n) || channel != n.round() / 255.0 {
            return Err("RGB must be exactly representable by native 8-bit channels".into());
        }
        result = (result << 8) | n.round() as u32;
    }
    Ok(result)
}
fn markers(
    markers: &[ae::Marker],
    rate: ae::FrameRate,
    duration: u32,
    path: &str,
) -> Result<Value, String> {
    let mut times = BTreeSet::new();
    let mut items = vec![];
    for (index, m) in markers.iter().enumerate() {
        if !m.chapter.is_empty()
            || !m.url.is_empty()
            || !m.frame_target.is_empty()
            || !m.cue_point_name.is_empty()
            || m.event_cue_point
            || m.protected_region
            || m.label != 0
            || !m.parameters.is_empty()
        {
            return fail(
                path,
                "Marker chapter, URL, cue, protected-region, label and parameter metadata is not supported",
            );
        }
        let at = unsigned_frame(m.time, rate, path)?;
        let length = unsigned_frame(m.duration, rate, path)?;
        if at >= duration || length > duration - at || !times.insert(at) {
            return fail(
                path,
                "Marker times must be unique and inside the composition",
            );
        }
        items.push(
            json!({"id":index+1,"frame":at,"duration":length,"name":m.comment,"color":0x808080}),
        );
    }
    Ok(json!({"next_id":items.len()+1,"items":items}))
}
fn character(s: &ae::CharacterStyle) -> Result<core::TextCharacterStyle, String> {
    if s.baseline_shift != 0. || s.horizontal_scale != 100. || s.vertical_scale != 100. {
        return Err("Text baseline shifts and character scaling are not supported".into());
    }
    // The human-readable font style label is provenance, not a substitute for
    // exact PostScript identity/weight/slant resolution.
    Ok(core::TextCharacterStyle {
        font_family: s.font.family.clone(),
        font_face: s.font.postscript_name.clone(),
        weight: s.font.weight,
        italic: s.font.italic,
        font_size: s.font_size,
        leading: None,
        tracking: s.tracking,
        fill_color: s.fill_rgb.map(rgb).transpose()?.unwrap_or(0),
        fill_enabled: s.fill_rgb.is_some(),
        stroke_color: s.stroke_rgb.map(rgb).transpose()?.unwrap_or(0),
        stroke_enabled: s.stroke_rgb.is_some(),
        stroke_width: s.stroke_width,
        stroke_over_fill: s.stroke_over_fill,
        stroke_join: match s.stroke_join {
            ae::StrokeJoin::Miter => core::TextStrokeJoin::Miter,
            ae::StrokeJoin::Round => core::TextStrokeJoin::Round,
            ae::StrokeJoin::Bevel => core::TextStrokeJoin::Bevel,
        },
    })
}
fn text(d: &ae::TextDocument) -> Result<(core::TextStyle, core::RichText), String> {
    if d.origin != ae::TextOrigin::NativeTopLeft {
        return Err("AE point-text baseline conversion remains unverified; explicit native-normalized coordinates are required".into());
    }
    if d.paragraph.box_size.is_some()
        || d.paragraph.left_indent != 0.
        || d.paragraph.right_indent != 0.
        || d.paragraph.first_line_indent != 0.
        || d.paragraph.space_before != 0.
        || d.paragraph.space_after != 0.
    {
        return Err("Rich paragraph boxes and paragraph spacing are not supported".into());
    }
    let default = character(&d.default_style)?;
    let mut style = core::TextStyle::default();
    default.apply_to_text_style(&mut style);
    style.leading = d.paragraph.leading;
    style.align = match d.paragraph.alignment {
        ae::ParagraphAlignment::Left => core::TextAlign::Left,
        ae::ParagraphAlignment::Center => core::TextAlign::Center,
        ae::ParagraphAlignment::Right => core::TextAlign::Right,
        ae::ParagraphAlignment::Justify => return Err("Justified text is not supported".into()),
    };
    let runs = if d.runs.is_empty() {
        if d.text.is_empty() {
            vec![]
        } else {
            vec![core::TextStyleRun {
                start: 0,
                end: d.text.encode_utf16().count(),
                style: default.clone(),
            }]
        }
    } else {
        d.runs
            .iter()
            .map(|run| {
                Ok(core::TextStyleRun {
                    start: run.start_utf16 as usize,
                    end: run.end_utf16 as usize,
                    style: character(&run.style)?,
                })
            })
            .collect::<Result<_, String>>()?
    };
    Ok((
        style,
        core::RichText::from_utf16_runs(&d.text, default, runs)?,
    ))
}
fn track(
    p: &ae::NumericProperty,
    component: usize,
    rate: ae::FrameRate,
    duration: u32,
    path: &str,
) -> Result<Value, String> {
    let base = p
        .value
        .as_ref()
        .and_then(|v| v.get(component))
        .ok_or_else(|| format!("{path}: explicit authored base is required"))?;
    let mut keys = BTreeMap::new();
    for k in &p.keys {
        if k.in_ease.is_some()
            || k.out_ease.is_some()
            || k.in_interpolation != k.out_interpolation
            || k.out_interpolation == ae::Interpolation::Bezier
        {
            return fail(
                path,
                "Per-side Bezier/ease metadata is not supported by this scalar import profile",
            );
        }
        let at = unsigned_frame(k.time, rate, path)?;
        if at >= duration {
            return fail(path, "Key lies outside the composition");
        }
        let v = *k
            .value
            .get(component)
            .ok_or_else(|| format!("{path}: key dimensions differ"))?;
        let mode = match k.out_interpolation {
            ae::Interpolation::Linear => "Linear",
            ae::Interpolation::Hold => "Hold",
            ae::Interpolation::Bezier => unreachable!(),
        };
        if keys
            .insert(at.to_string(), json!({"value":v,"interpolation":mode}))
            .is_some()
        {
            return fail(path, "Key times collide on the native frame grid");
        }
    }
    Ok(json!({"value":base,"keys":keys}))
}
fn expression(
    p: &ae::NumericProperty,
    target: Option<core::ExpressionTarget>,
    out: &mut Vec<core::NumericExpression>,
) -> Result<(), String> {
    if let Some(e) = &p.expression {
        let target = target.ok_or("Expressions on this property are unsupported")?;
        out.push(core::NumericExpression {
            target,
            source: e.source.clone(),
            enabled: e.enabled,
            local_bindings: vec![],
        });
    }
    Ok(())
}
fn layer(
    source: &ae::Layer,
    comp: &ae::Composition,
    document: &ae::ValidatedProject,
) -> Result<Value, String> {
    let path = format!("composition {} layer {}", comp.id, source.id);
    if source.three_d {
        return fail(
            &path,
            "True 3D transforms require the spatial model, which is not present at this recovered checkpoint",
        );
    }
    let duration = unsigned_frame(comp.duration, comp.frame_rate, &path)?;
    let start = frame(source.start_time, comp.frame_rate, &path)?;
    let inside = unsigned_frame(source.in_point, comp.frame_rate, &path)?;
    let outside = unsigned_frame(source.out_point, comp.frame_rate, &path)?;
    if inside >= outside || outside > duration {
        return fail(
            &path,
            "Layer trim must be a nonempty interval inside the composition",
        );
    }
    let (content, width, height, color, rich) = match &source.source {
        ae::LayerSource::Null { width, height } => (core::Content::Null, *width, *height, 0, None),
        ae::LayerSource::Solid {
            width,
            height,
            color,
        } => (core::Content::Solid, *width, *height, rgb(*color)?, None),
        ae::LayerSource::Text {
            width,
            height,
            document,
            keys,
        } => {
            if !keys.is_empty() {
                return fail(&path, "Animated rich Source Text is unsupported");
            };
            let (style, rich) = text(document)?;
            (
                core::Content::Text {
                    text: document.text.clone(),
                    font_size: document.default_style.font_size,
                },
                *width,
                *height,
                rich.default_style.fill_color,
                Some((style, rich)),
            )
        }
        ae::LayerSource::Composition { item_id } => {
            let c = document
                .composition(*item_id)
                .ok_or("Missing composition source")?;
            (
                core::Content::Composition {
                    composition: *item_id,
                    start_frame: start,
                },
                c.width,
                c.height,
                0,
                None,
            )
        }
        _ => {
            return fail(
                &path,
                "Footage and undecoded layer kinds cannot be converted",
            );
        }
    };
    // Obtain unchanged native defaults from a real admitted empty layer, then
    // assemble explicit authored data. Final Project validation is mandatory.
    // This avoids pose-compensating UI reparent/retime commands during ingestion.
    let mut editor = core::Editor::default();
    editor.execute(core::Command::AddContent {
        content: core::Content::Null,
        width: width as f64,
        height: height as f64,
        name: source.name.clone(),
    })?;
    let mut layer = value(
        editor
            .project()
            .composition()
            .layers()
            .first()
            .ok_or("Native layer creation failed")?,
    )?;
    layer["id"] = json!(source.id);
    layer["name"] = json!(source.name);
    layer["content"] = value(content)?;
    layer["width"] = json!(width);
    layer["height"] = json!(height);
    layer["color"] = json!(color);
    layer["visible"] = json!(source.enabled);
    layer["start_frame"] = json!(start);
    layer["in_frame"] = json!(inside);
    layer["out_frame"] = json!(outside);
    layer["label_index"] = json!(source.label);
    layer["parent"] = json!(source.parent_id);
    layer["markers"] = markers(&source.markers, comp.frame_rate, duration, &path)?;
    if let Some((style, rich)) = rich {
        layer["text_style"] = value(style)?;
        layer["rich_text"] = value(rich)?;
    }
    let mut seen = BTreeSet::new();
    let mut expressions = vec![];
    for p in &source.properties {
        let (names, target): (&[&str], Option<core::ExpressionTarget>) = match p
            .match_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["ADBE Transform Group", "ADBE Anchor Point"] => (&["AnchorX", "AnchorY"], None),
            ["ADBE Transform Group", "ADBE Position"] => (
                &["PositionX", "PositionY"],
                Some(core::ExpressionTarget::Position),
            ),
            ["ADBE Transform Group", "ADBE Scale"] => {
                (&["ScaleX", "ScaleY"], Some(core::ExpressionTarget::Scale))
            }
            ["ADBE Transform Group", "ADBE Rotate Z"] => (&["Rotation"], None),
            ["ADBE Transform Group", "ADBE Opacity"] => {
                (&["Opacity"], Some(core::ExpressionTarget::Opacity))
            }
            _ => return fail(&path, "Unknown numeric property path cannot be discarded"),
        };
        if p.dimensions as usize != names.len() {
            return fail(
                &path,
                "Property dimensions are not a supported 2D transform",
            );
        }
        for (i, name) in names.iter().enumerate() {
            if !seen.insert(*name) {
                return fail(&path, "Duplicate transform property");
            };
            layer["properties"][*name] = track(p, i, comp.frame_rate, duration, &path)?;
        }
        expression(p, target, &mut expressions)?;
    }
    if seen.len() != 8 {
        return fail(
            &path,
            "Explicit Anchor, Position, Scale, Rotation and Opacity are required; defaults are never inferred",
        );
    }
    let mut effects = vec![];
    let mut next = 1;
    for s in &source.sliders {
        if s.property.match_names != ["ADBE Slider Control", "ADBE Slider Control-0001"]
            || s.property.dimensions != 1
        {
            return fail(&path, "Unknown Slider property path/dimensions");
        }
        next = next.max(s.id.checked_add(1).ok_or("Slider ID overflow")?);
        effects.push(json!({"id":s.id,"kind":"SliderControl","name":s.name,"bypassed":false,"color_space":"Srgb","parameters":{"Amount":track(&s.property,0,comp.frame_rate,duration,&path)?}}));
        expression(
            &s.property,
            Some(core::ExpressionTarget::Slider(s.id)),
            &mut expressions,
        )?;
    }
    if !effects.is_empty() {
        layer["effect_stack"] = json!(effects);
        layer["next_effect_id"] = json!(next);
    }
    if !expressions.is_empty() {
        layer["expressions"] = value(expressions)?;
    }
    Ok(layer)
}
pub(super) fn convert(document: &ae::ValidatedProject, root: u64) -> Result<core::Project, String> {
    let closure = document
        .composition_closure(root)
        .map_err(|e| e.to_string())?;
    if !closure.diagnostics.is_empty() {
        return Err(closure
            .diagnostics
            .iter()
            .map(|d| format!("{}: {}", d.path, d.message))
            .collect::<Vec<_>>()
            .join("; "));
    }
    if closure.composition_ids.len() > 100 {
        return Err("Native projects support at most 100 compositions".into());
    }
    let mut project = value(core::Project::default())?;
    let mut others = serde_json::Map::new();
    let mut max_layer = 0;
    let mut max_comp = 0;
    let mut rich = false;
    for id in &closure.composition_ids {
        let c = document.composition(*id).ok_or("Missing composition")?;
        if c.pixel_aspect.numerator != c.pixel_aspect.denominator {
            return Err(format!(
                "Composition {id}: non-square pixels are unsupported"
            ));
        }
        let rate = core::FrameRate::new(c.frame_rate.numerator, c.frame_rate.denominator)?;
        let duration = unsigned_frame(c.duration, c.frame_rate, "composition duration")?;
        let mut composition = value(core::Project::default().composition())?;
        composition["name"] = json!(c.name);
        composition["width"] = json!(c.width);
        composition["height"] = json!(c.height);
        composition["fps"] = value(rate)?;
        composition["duration"] = json!(duration);
        composition["markers"] =
            markers(&c.markers, c.frame_rate, duration, "composition markers")?;
        let mut layers = vec![];
        for l in &c.layers {
            let native = layer(l, c, document)?;
            rich |= native.get("rich_text").is_some();
            layers.push(native);
            max_layer = max_layer.max(l.id);
        }
        composition["layers"] = json!(layers);
        max_comp = max_comp.max(*id);
        if *id == root {
            project["composition"] = composition;
        } else {
            others.insert(id.to_string(), composition);
        }
    }
    project["version"] = json!(if rich { 71 } else { 65 });
    project["composition_id"] = json!(root);
    project["next_layer_id"] = json!(max_layer.checked_add(1).ok_or("Layer ID overflow")?);
    project["next_composition_id"] =
        json!(max_comp.checked_add(1).ok_or("Composition ID overflow")?);
    project["other_compositions"] = Value::Object(others);
    let result: core::Project = serde_json::from_value(project).map_err(|e| e.to_string())?;
    result.validate_automation_project()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::rgb;
    #[test]
    fn exact_rgb_accepts_every_canonical_channel_and_rejects_perturbations() {
        for channel in 0..=255u32 {
            let value = channel as f64 / 255.0;
            assert_eq!(rgb([value, value, value]).unwrap(), channel * 0x010101);
        }
        assert!(rgb([1e-13, 0.0, 1.0]).is_err());
        assert!(rgb([128.0 / 255.0 + 1e-13, 0.0, 1.0]).is_err());
    }
}
