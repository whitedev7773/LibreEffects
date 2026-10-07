//! Convert an inspected AE scripting capture to an editable development LEP.
//! This is an offline reference adapter, not an AEP importer or a parity claim.
//! Unknown content, animation and missing resources reject rather than disappear.
//! Usage: reference_snapshot_project SNAPSHOT.json RESOURCES.json ROOT_ID NEW.lep [--all]
use libre_effects_core::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("Reference exceeds byte budget".into());
    }
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn array(v: &Value) -> Result<&Vec<Value>> {
    v.as_array().ok_or("Expected array".into())
}
fn num(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|n| n.is_finite())
        .ok_or("Missing finite number".into())
}
fn id(v: &Value) -> Result<u64> {
    v.as_u64()
        .filter(|n| *n > 0)
        .ok_or("Missing positive identity".into())
}
fn string(v: &Value) -> Result<&str> {
    v.as_str().ok_or("Missing string".into())
}
fn children(v: &Value) -> Result<&Vec<Value>> {
    array(&v["children"])
}
fn find<'a>(ps: &'a [Value], name: &str) -> Result<&'a Value> {
    ps.iter()
        .find(|p| p["matchName"] == name)
        .ok_or_else(|| format!("Missing property {name}").into())
}
fn value<'a>(ps: &'a [Value], name: &str) -> Result<&'a Value> {
    Ok(&find(ps, name)?["authoredValue"])
}
fn scalar(ps: &[Value], name: &str) -> Result<f64> {
    num(value(ps, name)?)
}
// Optional measured mappings are explicit development calibration, not AE parity.
// Once supplied, an unmeasured source amount must not silently use another unit.
fn spectrum_profile(resources: &Value) -> Result<&str> {
    match resources.get("audio_spectrum_profile") {
        None => Ok("NativeV1"),
        Some(Value::String(value)) if matches!(value.as_str(), "NativeV1" | "HammingV1") => {
            Ok(value)
        }
        _ => Err("Unknown explicit Audio Spectrum profile".into()),
    }
}
fn effect_sigma(resources: &Value, parameter: &str, amount: f64) -> Result<f64> {
    let Some(mapping) = resources.get("effect_sigma") else {
        return Ok(amount);
    };
    let sigma = num(&mapping[parameter][amount.to_string()])?;
    if sigma < 0.0 || (amount == 0.0 && sigma != 0.0) {
        return Err("Effect sigma calibration must preserve zero and be nonnegative".into());
    }
    Ok(sigma)
}
fn compositing_profile(resources: &Value) -> Result<CompositingProfile> {
    match resources.get("compositing_profile") {
        None => Ok(CompositingProfile::NativeV1),
        Some(Value::String(value)) if value == "NativeV1" => Ok(CompositingProfile::NativeV1),
        Some(Value::String(value)) if value == "OpaqueOpacityByte257V1" => {
            Ok(CompositingProfile::OpaqueOpacityByte257V1)
        }
        _ => Err("Unknown explicit compositing profile".into()),
    }
}
fn mask_feather_kernel(resources: &Value, amount: f64) -> Result<MaskFeatherKernel> {
    let radii = resources.get("mask_feather_box3_radii");
    let profiles = resources.get("mask_feather_profiles");
    if radii.is_some() && profiles.is_some() {
        return Err("Supply one explicit mask calibration map".into());
    }
    let Some(mapping) = profiles.or(radii) else {
        return Ok(MaskFeatherKernel::GaussianV1);
    };
    if !mapping.is_object() {
        return Err("Mask box3 calibration must be an explicit amount-to-radius map".into());
    }
    let entry = &mapping[amount.to_string()];
    let radius = if profiles.is_some() {
        if entry == "GaussianV1" {
            return Ok(MaskFeatherKernel::GaussianV1);
        }
        let object = entry
            .as_object()
            .ok_or("Mask profile requires GaussianV1 or an explicit box3_radius")?;
        if object.len() != 1 || !object.contains_key("box3_radius") {
            return Err("Unknown explicit mask feather profile".into());
        }
        num(&entry["box3_radius"])?
    } else {
        num(entry)?
    };
    if amount == 0.0 {
        if radius != 0.0 {
            return Err("Mask box3 calibration must preserve zero".into());
        }
        return Ok(MaskFeatherKernel::GaussianV1);
    }
    if amount < 3.0 || radius < 0.5 {
        return Err("Captured box3 calibration requires Feather >=3 and radius >=0.5".into());
    }
    let kernel = MaskFeatherKernel::FractionalBox3V1 {
        radius_per_unit: (radius - 0.5) / amount,
    };
    if !kernel.valid() {
        return Err("Mask box3 calibration exceeds native coefficient bounds".into());
    }
    Ok(kernel)
}
fn xy(v: &Value) -> Result<[f64; 2]> {
    Ok([num(&v[0])?, num(&v[1])?])
}
fn frame(seconds: f64, fps: f64) -> Result<u32> {
    let n = seconds * fps;
    if n < 0.0 || n > f64::from(u32::MAX) || (n - n.round()).abs() > 1e-7 {
        return Err("Authored key/time is not on the owning composition frame grid".into());
    }
    Ok(n.round() as u32)
}
fn endpoint(seconds: f64, fps: f64) -> f64 {
    let n = seconds * fps;
    if (n - n.round()).abs() < 1e-7 {
        n.round()
    } else {
        n
    }
}
fn track(v: f64) -> Value {
    json!({"value":v,"keys":{}})
}
fn rgb(v: &Value) -> Result<u32> {
    let mut color = 0;
    if array(v)?.len() < 3 {
        return Err("Color requires three channels".into());
    }
    for channel in array(v)?.iter().take(3) {
        let n = num(channel)? * 255.0;
        if !(0.0..=255.0).contains(&n) || (n - n.round()).abs() > 1e-6 {
            return Err("Color requires an explicit non-8-bit conversion policy".into());
        }
        color = (color << 8) | n.round() as u32;
    }
    Ok(color)
}
fn path(v: &Value) -> Result<VectorPath> {
    if v["type"] != "shape" {
        return Err("Expected captured Shape".into());
    }
    let a = &v["attributes"];
    let vertices = array(&a["vertices"])?;
    if array(&a["inTangents"])?.len() != vertices.len()
        || array(&a["outTangents"])?.len() != vertices.len()
    {
        return Err("Shape tangent counts differ".into());
    }
    Ok(VectorPath {
        closed: a["closed"].as_bool().ok_or("Missing closed flag")?,
        vertices: vertices
            .iter()
            .enumerate()
            .map(|(i, p)| {
                Ok(PathVertex {
                    position: xy(p)?,
                    incoming: xy(&a["inTangents"][i])?,
                    outgoing: xy(&a["outTangents"][i])?,
                })
            })
            .collect::<Result<_>>()?,
    })
}
fn interpolation(v: &Value) -> Result<&'static str> {
    match v.as_u64() {
        Some(6612) => Ok("Linear"),
        Some(6613) => Ok("Bezier"),
        Some(6614) => Ok("Hold"),
        _ => Err("Unknown interpolation enum".into()),
    }
}
fn timing(k: &Value) -> Result<Value> {
    Ok(
        json!({"in_interpolation":interpolation(&k["inInterpolation"])?,
        "out_interpolation":interpolation(&k["outInterpolation"])?,
        "in_ease":k["inEase"][0],"out_ease":k["outEase"][0],
        "temporal_continuous":k["temporalContinuous"],"temporal_auto_bezier":k["temporalAutoBezier"]}),
    )
}
fn expression(p: &Value, target: Value, locals: &Value) -> Result<Option<Value>> {
    let a = &p["attributes"];
    let source = a["expression"].as_str().unwrap_or("");
    if source.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        json!({"target":target,"source":source,"enabled":a["expressionEnabled"],
        "local_bindings":locals.as_array().cloned().unwrap_or_default()}),
    ))
}
fn ensure_static(p: &Value) -> Result<()> {
    if p["keys"].as_array().is_some_and(|k| !k.is_empty())
        || p["attributes"]["expressionEnabled"] == true
    {
        return Err(format!("Unmapped animation: {}", p["matchName"]).into());
    }
    for child in p["children"].as_array().into_iter().flatten() {
        ensure_static(child)?;
    }
    Ok(())
}
fn node(p: &Value) -> Result<ContentsNode> {
    ensure_static(p)?;
    let ps = children(p)?;
    let m = string(&p["matchName"])?;
    let kind = match m {
        "ADBE Vector Group" => ContentsKind::Group(
            children(find(ps, "ADBE Vectors Group")?)?
                .iter()
                .map(node)
                .collect::<Result<_>>()?,
        ),
        "ADBE Vector Shape - Rect" => ContentsKind::Parametric(ShapeKind::RoundedRectangle),
        "ADBE Vector Shape - Ellipse" => ContentsKind::Parametric(ShapeKind::Ellipse),
        "ADBE Vector Shape - Group" => ContentsKind::Path {
            path: path(value(ps, "ADBE Vector Shape")?)?,
            animation: Default::default(),
        },
        "ADBE Vector Graphic - Fill" => ContentsKind::Fill {
            even_odd: scalar(ps, "ADBE Vector Fill Rule")? == 2.0,
        },
        "ADBE Vector Graphic - Stroke" => ContentsKind::Stroke(ShapeStroke {
            cap: match scalar(ps, "ADBE Vector Stroke Line Cap")? as u32 {
                1 => StrokeCap::Butt,
                2 => StrokeCap::Round,
                3 => StrokeCap::Square,
                _ => return Err("Unknown stroke cap".into()),
            },
            join: match scalar(ps, "ADBE Vector Stroke Line Join")? as u32 {
                1 => StrokeJoin::Miter,
                2 => StrokeJoin::Round,
                3 => StrokeJoin::Bevel,
                _ => return Err("Unknown stroke join".into()),
            },
            miter_limit: scalar(ps, "ADBE Vector Stroke Miter Limit")?,
            ..Default::default()
        }),
        _ => return Err(format!("Unsupported Shape Contents: {m}").into()),
    };
    let mut n = ContentsNode::with_defaults(kind);
    n.centered = matches!(n.kind, ContentsKind::Parametric(_));
    n.name = string(&p["name"])?.into();
    n.enabled = p["enabled"] == true;
    let mut values = Vec::new();
    if m == "ADBE Vector Group" {
        let t = children(find(ps, "ADBE Vector Transform Group")?)?;
        for (match_name, x, y) in [
            (
                "ADBE Vector Position",
                Property::PositionX,
                Property::PositionY,
            ),
            ("ADBE Vector Anchor", Property::AnchorX, Property::AnchorY),
            ("ADBE Vector Scale", Property::ScaleX, Property::ScaleY),
        ] {
            let v = xy(value(t, match_name)?)?;
            values.extend([
                (ContentsParam::Transform(x), v[0]),
                (ContentsParam::Transform(y), v[1]),
            ]);
        }
        values.extend([
            (
                ContentsParam::Transform(Property::Rotation),
                scalar(t, "ADBE Vector Rotation")?,
            ),
            (
                ContentsParam::Transform(Property::Opacity),
                scalar(t, "ADBE Vector Group Opacity")?,
            ),
            (ContentsParam::Skew, scalar(t, "ADBE Vector Skew")?),
            (ContentsParam::SkewAxis, scalar(t, "ADBE Vector Skew Axis")?),
        ]);
    } else if m == "ADBE Vector Shape - Rect" || m == "ADBE Vector Shape - Ellipse" {
        let prefix = if m.ends_with("Rect") {
            "ADBE Vector Rect"
        } else {
            "ADBE Vector Ellipse"
        };
        let size = xy(value(ps, &format!("{prefix} Size"))?)?;
        let pos = xy(value(ps, &format!("{prefix} Position"))?)?;
        values.extend([
            (ContentsParam::Width, size[0]),
            (ContentsParam::Height, size[1]),
            (ContentsParam::Transform(Property::PositionX), pos[0]),
            (ContentsParam::Transform(Property::PositionY), pos[1]),
        ]);
        if m.ends_with("Rect") {
            values.push((
                ContentsParam::Shape(ShapeParam::Roundness),
                scalar(ps, "ADBE Vector Rect Roundness")?,
            ));
        }
    } else if m.ends_with("Fill") || m.ends_with("Stroke") {
        let fill = m.ends_with("Fill");
        let prefix = if fill {
            "ADBE Vector Fill"
        } else {
            "ADBE Vector Stroke"
        };
        let c = rgb(value(ps, &format!("{prefix} Color"))?)?;
        let colors = if fill {
            [
                ShapeParam::FillRed,
                ShapeParam::FillGreen,
                ShapeParam::FillBlue,
            ]
        } else {
            [
                ShapeParam::StrokeRed,
                ShapeParam::StrokeGreen,
                ShapeParam::StrokeBlue,
            ]
        };
        for (parameter, shift) in colors.into_iter().zip([16, 8, 0]) {
            values.push((
                ContentsParam::Shape(parameter),
                f64::from((c >> shift) & 255),
            ));
        }
        values.push((
            ContentsParam::Shape(if fill {
                ShapeParam::FillOpacity
            } else {
                ShapeParam::StrokeOpacity
            }),
            scalar(ps, &format!("{prefix} Opacity"))?,
        ));
        if !fill {
            values.push((
                ContentsParam::Shape(ShapeParam::StrokeWidth),
                scalar(ps, "ADBE Vector Stroke Width")?,
            ));
        }
    }
    for (parameter, v) in values {
        n.set_static_value(parameter, v)?;
    }
    Ok(n)
}

struct Adapter {
    resources: Value,
    fonts: fontdb::Database,
    assets: BTreeMap<u64, Value>,
    issues: Vec<Value>,
    font_evidence: BTreeMap<String, Value>,
    template: Value,
}
impl Adapter {
    fn text_style(&mut self, a: &Value, base: &TextStyle) -> Result<TextCharacterStyle> {
        let face_name = string(&a["font"])?;
        let face = self
            .fonts
            .faces()
            .find(|f| f.post_script_name == face_name)
            .ok_or_else(|| format!("Exact PostScript font unavailable: {face_name}"))?;
        let family = face
            .families
            .first()
            .ok_or("Missing font family")?
            .0
            .clone();
        self.font_evidence.insert(face_name.into(),json!({"family":family,"weight":face.weight.0,"source":format!("{:?}",face.source),"face_index":face.index}));
        let mut style =
            TextCharacterStyle::from_style(base, num(&a["fontSize"])?, rgb(&a["fillColor"])?);
        style.font_family = family;
        style.font_face = face_name.into();
        style.weight = face.weight.0;
        style.italic = a["fauxItalic"] == true || face.style != fontdb::Style::Normal;
        style.tracking = num(&a["tracking"])?;
        style.fill_enabled = a["applyFill"] == true;
        style.stroke_enabled = a["applyStroke"] == true;
        style.stroke_color = if a["strokeColor"].is_null() {
            0
        } else {
            rgb(&a["strokeColor"])?
        };
        style.stroke_width = num(&a["strokeWidth"])?;
        style.stroke_over_fill = a["strokeOverFill"] == true;
        style.leading = Some(if a["autoLeading"] == true {
            TextLeading::Auto(1.2)
        } else {
            TextLeading::Fixed(num(&a["leading"])?)
        });
        if num(&a["baselineShift"])? != 0.0
            || num(&a["horizontalScale"])? != 1.0
            || num(&a["verticalScale"])? != 1.0
            || a["fauxBold"] == true
        {
            return Err("Typography needs baseline/scale/faux-bold support".into());
        }
        Ok(style)
    }
    fn media(&mut self, item: &Value) -> Result<Value> {
        let key = id(&item["id"])?;
        if let Some(asset) = self.assets.get(&key) {
            return Ok(asset["content"].clone());
        }
        let file = string(&item["file"])?;
        if item["footageMissing"] == true || item["useProxy"] == true {
            return Err("Missing/proxied footage requires reviewed relinking".into());
        }
        let bytes = read(Path::new(file), 512 * 1024 * 1024)?;
        let reviewed = &self.resources["media"][key.to_string()];
        if reviewed["sha256"] != hash(&bytes) {
            return Err(format!("Media checksum differs for {key}").into());
        }
        let content = if item["source"]["isStill"] == true {
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err("Still adapter currently requires original PNG bytes".into());
            }
            json!({"Image":{"png":base64(&bytes)}})
        } else if item["hasVideo"] == true {
            json!({"Video":{"path":file,"audio":reviewed["audio"],"duration":item["duration"],"source_fps":item["frameRate"],"start_frame":0}})
        } else if item["hasAudio"] == true {
            if reviewed["audio"].is_null() {
                return Err("Missing probed audio metadata".into());
            }
            json!({"Audio":{"path":file,"audio":reviewed["audio"],"start_frame":0}})
        } else {
            return Err("Unclassified source file".into());
        };
        self.assets.insert(key,json!({"name":item["name"],"folder":null,"width":num(&item["width"])?.max(1.0),"height":num(&item["height"])?.max(1.0),"content":content}));
        Ok(content)
    }
    fn layer(&mut self, l: &Value, c: &Value, items: &BTreeMap<u64, &Value>) -> Result<Value> {
        let layer_id = id(&l["id"])?;
        let comp_id = id(&c["id"])?;
        let fps = num(&c["frameRate"])?;
        if l["threeDLayer"] == true || num(&l["stretch"])? != 100.0 || l["timeRemapEnabled"] == true
        {
            return Err("Layer needs 3D/stretch/remap adapter".into());
        }
        let ps = array(&l["properties"])?;
        let mut n = self.template.clone();
        n["id"] = json!(layer_id);
        n["name"] = l["name"].clone();
        for (native, source) in [
            ("visible", "enabled"),
            ("locked", "locked"),
            ("solo", "solo"),
            ("shy", "shy"),
            ("guide", "guideLayer"),
        ] {
            n[native] = l[source].clone();
        }
        n["label_index"] = l["label"].clone();
        n["parent"] = l["parentId"].clone();
        let origin = endpoint(num(&l["startTime"])?, fps);
        if origin.fract() != 0.0 {
            return Err("Fractional layer origin requires source-clock support".into());
        }
        n["start_frame"] = json!(origin as i64);
        let a = endpoint(num(&l["inPoint"])?, fps);
        let b = endpoint(num(&l["outPoint"])?, fps);
        let duration = frame(num(&c["duration"])?, fps)?;
        n["in_frame"] = json!(a.max(0.0).ceil() as u32);
        n["out_frame"] = json!(b.min(f64::from(duration)).ceil() as u32);
        if a < 0.0 || a.fract() != 0.0 || b.fract() != 0.0 || b > f64::from(duration) {
            n["precise_range"] = json!([a, b]);
        }
        let source = l["sourceId"]
            .as_u64()
            .and_then(|id| items.get(&id).copied());
        n["width"] = json!(
            source
                .and_then(|s| s["width"].as_f64())
                .filter(|v| *v > 0.0)
                .unwrap_or(num(&c["width"])?)
        );
        n["height"] = json!(
            source
                .and_then(|s| s["height"].as_f64())
                .filter(|v| *v > 0.0)
                .unwrap_or(num(&c["height"])?)
        );
        let mut expressions = Vec::new();
        n["content"] = if l["nullLayer"] == true {
            json!("Null")
        } else if l["adjustmentLayer"] == true {
            json!("Adjustment")
        } else if l["matchName"] == "ADBE Vector Layer" {
            let nodes = children(find(ps, "ADBE Root Vectors Group")?)?
                .iter()
                .map(node)
                .collect::<Result<Vec<_>>>()?;
            serde_json::to_value(Content::ShapeContents(ShapeContents::from_nodes(nodes)?))?
        } else if l["matchName"] == "ADBE Text Layer" {
            let p = find(
                children(find(ps, "ADBE Text Properties")?)?,
                "ADBE Text Document",
            )?;
            if !array(&p["keys"])?.is_empty() {
                return Err("Keyed Source Text requires document animation adapter".into());
            }
            let doc = &p["authoredValue"];
            let a = &doc["attributes"];
            let text = string(&a["text"])?;
            let mut base = TextStyle::default();
            base.align = match a["justification"].as_u64() {
                Some(7413) => TextAlign::Left,
                Some(7415) => TextAlign::Center,
                Some(7414) => TextAlign::Right,
                _ => return Err("Unsupported justification".into()),
            };
            if a["pointText"] != true {
                return Err("Paragraph text requires box geometry adapter".into());
            }
            let default = self.text_style(a, &base)?;
            default.apply_to_text_style(&mut base);
            if let Some(TextLeading::Fixed(v)) = default.leading {
                base.leading = v / default.font_size;
            }
            let mut runs = Vec::new();
            for run in array(&doc["characterRuns"])? {
                runs.push(TextStyleRun {
                    start: run["startUTF16"].as_u64().ok_or("Missing UTF16 start")? as usize,
                    end: run["endUTF16"].as_u64().ok_or("Missing UTF16 end")? as usize,
                    style: self.text_style(&run["attributes"], &base)?,
                });
            }
            let mut rich = RichText::from_utf16_runs(text, default, runs)?;
            rich.point_origin = true;
            rich.proportional_metrics = a["autoKernType"] == 11413;
            n["text_style"] = serde_json::to_value(base)?;
            n["rich_text"] = serde_json::to_value(&rich)?;
            n["color"] = json!(rich.default_style.fill_color);
            let locals = &self.resources["locals"][format!("{comp_id}:{layer_id}")];
            if let Some(e) = expression(p, json!("SourceText"), locals)? {
                expressions.push(e);
            }
            json!({"Text":{"text":text,"font_size":num(&a["fontSize"])?}})
        } else if let Some(s) = source {
            if s["kind"] == "composition" {
                json!({"Composition":{"composition":s["id"],"start_frame":origin as i64}})
            } else if s["file"].as_str().is_some() {
                let mut content = self.media(s)?;
                if let Some(audio) = content.get_mut("Audio") {
                    audio["start_frame"] = json!(origin as i64);
                    n["width"] = json!(1.0);
                    n["height"] = json!(1.0);
                }
                if let Some(video) = content.get_mut("Video") {
                    video["start_frame"] = json!(origin as i64);
                }
                n["asset"] = s["id"].clone();
                content
            } else if s["source"]["color"].is_array() {
                n["color"] = json!(rgb(&s["source"]["color"])?);
                json!("Solid")
            } else {
                return Err("Source has neither file nor exposed solid color".into());
            }
        } else {
            return Err("Unsupported layer content".into());
        };
        let ts = children(find(ps, "ADBE Transform Group")?)?;
        for (m, props, target) in [
            (
                "ADBE Anchor Point",
                vec![Property::AnchorX, Property::AnchorY],
                None,
            ),
            (
                "ADBE Position",
                vec![Property::PositionX, Property::PositionY],
                Some("Position"),
            ),
            (
                "ADBE Scale",
                vec![Property::ScaleX, Property::ScaleY],
                Some("Scale"),
            ),
            ("ADBE Rotate Z", vec![Property::Rotation], None),
            ("ADBE Opacity", vec![Property::Opacity], Some("Opacity")),
        ] {
            let p = find(ts, m)?;
            let v = &p["authoredValue"];
            for (i, property) in props.iter().enumerate() {
                n["properties"][format!("{property:?}")] = track(if props.len() > 1 {
                    num(&v[i])?
                } else {
                    num(v)?
                });
            }
            if let Some(target) = target {
                if let Some(e) = expression(p, json!(target), &Value::Null)? {
                    expressions.push(e);
                }
            }
            let keys = array(&p["keys"])?;
            if !keys.is_empty() {
                if m == "ADBE Position" {
                    let mut map = BTreeMap::new();
                    for k in keys {
                        if k["roving"] == true {
                            return Err("Roving Position key needs effective-time evidence".into());
                        }
                        let mut key = timing(k)?;
                        key["value"] = json!(xy(&k["value"])?);
                        key["in_tangent"] = json!(xy(&k["inSpatialTangent"])?);
                        key["out_tangent"] = json!(xy(&k["outSpatialTangent"])?);
                        key["spatial_continuous"] = k["spatialContinuous"].clone();
                        key["spatial_auto_bezier"] = k["spatialAutoBezier"].clone();
                        if map.insert(frame(num(&k["time"])?, fps)?, key).is_some() {
                            return Err("Duplicate key time".into());
                        }
                    }
                    n["planar_position"] = json!({"keys":map});
                    n["properties"]
                        .as_object_mut()
                        .ok_or("Missing property map")?
                        .remove("PositionX");
                    n["properties"]
                        .as_object_mut()
                        .ok_or("Missing property map")?
                        .remove("PositionY");
                } else if m == "ADBE Opacity" {
                    let mut metadata = BTreeMap::new();
                    let mut values = BTreeMap::new();
                    for k in keys {
                        let f = frame(num(&k["time"])?, fps)?;
                        metadata.insert(f, timing(k)?);
                        values.insert(f, json!({"value":k["value"],"interpolation":"Linear"}));
                    }
                    n["properties"]["Opacity"]["keys"] = json!(values);
                    n["opacity_timing"] = json!({"keys":metadata});
                } else {
                    return Err(format!("Unsupported transform animation {m}").into());
                }
            }
        }
        let markers = array(&find(ps, "ADBE Marker")?["keys"])?;
        n["markers"] = json!({"next_id":markers.len()+1,"items":markers.iter().enumerate().map(|(i,k)| Ok(json!({"id":i+1,"frame":frame(num(&k["time"])?,fps)?,"duration":frame(num(&k["value"]["attributes"]["duration"])?,fps)?,"name":k["value"]["attributes"]["comment"],"color":0x808080}))).collect::<Result<Vec<_>>>()?});
        let mut masks = Vec::new();
        if let Ok(group) = find(ps, "ADBE Mask Parade") {
            for (i, m) in children(group)?.iter().enumerate() {
                let props = children(m)?;
                let p = find(props, "ADBE Mask Shape")?;
                if !array(&p["keys"])?.is_empty() {
                    return Err("Authored mask animation requires adapter".into());
                }
                let mut mask = PathMask {
                    id: i as u64 + 1,
                    path: path(&p["authoredValue"])?,
                    inverted: m["maskAttributes"]["inverted"] == true,
                    ..Default::default()
                };
                mask.mode = match m["maskAttributes"]["maskMode"].as_u64() {
                    Some(6812) => PathMaskMode::None,
                    Some(6813) => PathMaskMode::Add,
                    Some(6814) => PathMaskMode::Subtract,
                    Some(6815) => PathMaskMode::Intersect,
                    _ => return Err("Unsupported mask mode".into()),
                };
                let feather = xy(value(props, "ADBE Mask Feather")?)?;
                if feather[0] != feather[1] {
                    return Err("Anisotropic mask feather requires native support".into());
                }
                for parameter in ["ADBE Mask Feather", "ADBE Mask Opacity", "ADBE Mask Offset"] {
                    ensure_static(find(props, parameter)?)?;
                }
                mask.feather_kernel = mask_feather_kernel(&self.resources, feather[0])?;
                let mut mv = serde_json::to_value(mask)?;
                for (parameter, v) in [
                    ("Opacity", scalar(props, "ADBE Mask Opacity")?),
                    ("Expansion", scalar(props, "ADBE Mask Offset")?),
                    ("Feather", feather[0]),
                ] {
                    mv["parameters"][parameter] = track(v);
                }
                if let Some(e) = expression(p, json!({"MaskPath":i+1}), &Value::Null)? {
                    expressions.push(e);
                }
                masks.push(mv);
            }
        }
        n["next_mask_id"] = json!(masks.len() + 1);
        n["path_masks"] = json!(masks);
        if l["hasTrackMatte"] == true {
            n["track_matte"] = json!({"source":l["trackMatteLayerId"],"mode":match l["trackMatteType"].as_u64(){Some(5013)=>"Alpha",Some(5014)=>"AlphaInverted",Some(5015)=>"Luma",Some(5016)=>"LumaInverted",_=>return Err("Unknown matte mode".into())}});
        }
        n["blend_mode"] = json!(match l["blendingMode"].as_u64() {
            Some(5212) => "Normal",
            Some(5233) => "Difference",
            _ => return Err("Unmapped layer blend mode".into()),
        });
        if let Ok(effects) = find(ps, "ADBE Effect Parade") {
            let mut stack = Vec::new();
            for (i, e) in children(effects)?.iter().enumerate() {
                stack.push(self.effect(e, i as u64 + 1, c, layer_id)?);
            }
            n["next_effect_id"] = json!(stack.len() + 1);
            n["effect_stack"] = json!(stack);
        }
        n["expressions"] = json!(expressions);
        if n["content"].get("Audio").is_some()
            || n["content"].get("Video").is_some()
            || n["content"].get("Composition").is_some()
        {
            n["audio_controls"] = json!({"enabled":l["audioEnabled"],"parameters":{"LeftLevel":track(0.0),"RightLevel":track(0.0),"Pan":track(0.0),"Fade":track(100.0)}});
        }
        Ok(n)
    }
    fn effect(&mut self, e: &Value, id: u64, c: &Value, layer: u64) -> Result<Value> {
        ensure_static(e)?;
        let m = string(&e["matchName"])?;
        let ps = children(e)?;
        let kind = match m {
            "ADBE Slider Control" => EffectKind::SliderControl,
            "ADBE Fill" => EffectKind::Fill,
            "ADBE Drop Shadow" => EffectKind::DropShadow,
            "ADBE Gaussian Blur 2" => EffectKind::GaussianBlur,
            "ADBE AudSpect" => EffectKind::AudioSpectrum,
            _ => return Err(format!("Unsupported effect {m}").into()),
        };
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle)?;
        editor.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(kind),
        })?;
        let mut n =
            serde_json::to_value(&editor.project().composition().layers()[0].effect_stack()[0])?;
        n["id"] = json!(id);
        n["name"] = e["name"].clone();
        n["bypassed"] = json!(e["enabled"] != true);
        let mut vals = Vec::new();
        match kind {
            EffectKind::SliderControl => {
                vals.push(("Amount", scalar(ps, "ADBE Slider Control-0001")?))
            }
            EffectKind::GaussianBlur => {
                if scalar(ps, "ADBE Gaussian Blur 2-0002")? != 1.0 {
                    return Err("Directional Gaussian Blur requires an axis adapter".into());
                }
                n["gaussian_edge_mode"] = json!(match scalar(ps, "ADBE Gaussian Blur 2-0003")? {
                    0.0 => "Transparent",
                    1.0 => "Repeat",
                    _ => return Err("Unknown Gaussian boundary option".into()),
                });
                vals.push((
                    "Radius",
                    effect_sigma(
                        &self.resources,
                        "ADBE Gaussian Blur 2-0001",
                        scalar(ps, "ADBE Gaussian Blur 2-0001")?,
                    )?,
                ));
                self.issues.push(json!({"layer":layer,"effect":id,"requirement":"AE Gaussian kernel/dimensions/edge parity","source":e}));
            }
            EffectKind::Fill => {
                let color = rgb(value(ps, "ADBE Fill-0002")?)?;
                for (p, shift) in [("Red", 16), ("Green", 8), ("Blue", 0)] {
                    vals.push((p, f64::from((color >> shift) & 255)));
                }
                vals.push(("Opacity", scalar(ps, "ADBE Fill-0005")? * 100.0));
            }
            EffectKind::DropShadow => {
                if scalar(ps, "ADBE Drop Shadow-0006")? != 0.0 {
                    return Err("Shadow-only paint requires a generator adapter".into());
                }
                let color = rgb(value(ps, "ADBE Drop Shadow-0001")?)?;
                for (p, shift) in [("Red", 16), ("Green", 8), ("Blue", 0)] {
                    vals.push((p, f64::from((color >> shift) & 255)));
                }
                let direction = scalar(ps, "ADBE Drop Shadow-0003")?.to_radians();
                let distance = scalar(ps, "ADBE Drop Shadow-0004")?;
                vals.extend([
                    (
                        "Opacity",
                        scalar(ps, "ADBE Drop Shadow-0002")? * 100.0 / 255.0,
                    ),
                    ("OffsetX", direction.sin() * distance),
                    ("OffsetY", -direction.cos() * distance),
                    (
                        "Radius",
                        effect_sigma(
                            &self.resources,
                            "ADBE Drop Shadow-0005",
                            scalar(ps, "ADBE Drop Shadow-0005")?,
                        )?,
                    ),
                ]);
                self.issues.push(json!({"layer":layer,"effect":id,"requirement":"AE Drop Shadow softness/raster parity","source":e}));
            }
            EffectKind::AudioSpectrum => {
                let selected = scalar(ps, "ADBE AudSpect-0001")? as usize;
                let selected_layer = c["layers"]
                    [selected.checked_sub(1).ok_or("Spectrum layer index")?]["id"]
                    .clone();
                let display = match scalar(ps, "ADBE AudSpect-0020")? {
                    1.0 => "Bars",
                    2.0 => "Line",
                    3.0 => "Points",
                    _ => return Err("Unknown Audio Spectrum display option".into()),
                };
                let side = match scalar(ps, "ADBE AudSpect-0021")? {
                    1.0 => "Above",
                    2.0 => "Below",
                    3.0 => "Both",
                    _ => return Err("Unknown Audio Spectrum side option".into()),
                };
                n["audio_spectrum"] = json!({"profile":spectrum_profile(&self.resources)?,"input_scope":"SelectedLayerOutput","source":{"layer":selected_layer},"bands":scalar(ps,"ADBE AudSpect-0008")? as u16,"start_hz":scalar(ps,"ADBE AudSpect-0006")?,"end_hz":scalar(ps,"ADBE AudSpect-0007")?,"duration_ms":scalar(ps,"ADBE AudSpect-0010")?,"offset_ms":scalar(ps,"ADBE AudSpect-0011")?,"start":xy(value(ps,"ADBE AudSpect-0002")?)?,"end":xy(value(ps,"ADBE AudSpect-0003")?)?,"maximum_height":scalar(ps,"ADBE AudSpect-0009")?,"thickness":scalar(ps,"ADBE AudSpect-0012")?,"color":[255,255,255,255],"display":display,"side":side,"composite_original":scalar(ps,"ADBE AudSpect-0023")?==1.0});
                self.issues.push(json!({"layer":layer,"effect":id,"requirement":"AE Audio Spectrum profile, paint, softness and source parity","source":e}));
            }
            _ => unreachable!(),
        }
        for (p, v) in vals {
            n["parameters"][p] = track(v);
        }
        Ok(n)
    }
}
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        s.push(A[((n >> 18) & 63) as usize] as char);
        s.push(A[((n >> 12) & 63) as usize] as char);
        s.push(if c.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    s
}
fn visit(
    id: u64,
    items: &BTreeMap<u64, &Value>,
    pending: &mut BTreeSet<u64>,
    done: &mut BTreeSet<u64>,
    order: &mut Vec<u64>,
) -> Result<()> {
    if done.contains(&id) {
        return Ok(());
    }
    if pending.len() >= 16 || !pending.insert(id) {
        return Err("Composition dependency cycle/depth".into());
    }
    let c = items.get(&id).ok_or("Missing composition")?;
    if c["kind"] != "composition" {
        return Err("Root is not a composition".into());
    }
    for layer in array(&c["layers"])? {
        if let Some(source) = layer["sourceId"].as_u64() {
            if items
                .get(&source)
                .is_some_and(|s| s["kind"] == "composition")
            {
                visit(source, items, pending, done, order)?;
            }
        }
    }
    pending.remove(&id);
    done.insert(id);
    order.push(id);
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(args.len() == 4 || (args.len() == 5 && args[4] == "--all")) {
        return Err(
            "Usage: reference_snapshot_project SNAPSHOT.json RESOURCES.json ROOT_ID NEW.lep [--all]".into(),
        );
    }
    let output = Path::new(&args[3]);
    if output.exists()
        || output
            .extension()
            .is_none_or(|s| !s.eq_ignore_ascii_case("lep"))
    {
        return Err("Output must be a new .lep".into());
    }
    let receipt = output.with_extension("conversion.json");
    if receipt.exists() {
        return Err("Receipt already exists".into());
    }
    let bytes = read(Path::new(&args[0]), 64 * 1024 * 1024)?;
    let s: Value = serde_json::from_slice(&bytes)?;
    if s["version"] != 1 || !array(&s["errors"])?.is_empty() {
        return Err("Unsupported or incomplete capture".into());
    }
    let resource_bytes = read(Path::new(&args[1]), 1024 * 1024)?;
    let resources: Value = serde_json::from_slice(&resource_bytes)?;
    let root = args[2].to_str().ok_or("Root encoding")?.parse::<u64>()?;
    let mut items = BTreeMap::new();
    for item in array(&s["items"])? {
        if items.insert(id(&item["id"])?, item).is_some() {
            return Err("Duplicate item identity".into());
        }
    }
    let mut order = Vec::new();
    let mut done = BTreeSet::new();
    visit(root, &items, &mut BTreeSet::new(), &mut done, &mut order)?;
    if args.len() == 5 {
        for (cid, c) in &items {
            if c["kind"] == "composition" {
                visit(*cid, &items, &mut BTreeSet::new(), &mut done, &mut order)?;
            }
        }
    }
    let mut fonts = fontdb::Database::new();
    fonts.load_system_fonts();
    let mut editor = Editor::default();
    editor.execute(Command::AddRectangle)?;
    let mut adapter = Adapter {
        resources,
        fonts,
        assets: BTreeMap::new(),
        issues: Vec::new(),
        font_evidence: BTreeMap::new(),
        template: serde_json::to_value(&editor.project().composition().layers()[0])?,
    };
    let mut comps = BTreeMap::new();
    let mut layer_ids = BTreeSet::new();
    for cid in &order {
        let c = items[cid];
        let fps = num(&c["frameRate"])?;
        if fps.fract() != 0.0 || !(1.0..=240.0).contains(&fps) || num(&c["pixelAspect"])? != 1.0 {
            return Err("Non-integer FPS or non-square pixels need an explicit adapter".into());
        }
        let mut layers = Vec::new();
        for l in array(&c["layers"])? {
            if !layer_ids.insert(id(&l["id"])?) {
                return Err("Duplicate layer identity".into());
            }
            layers.push(
                adapter
                    .layer(l, c, &items)
                    .map_err(|e| format!("Composition {cid}, layer {}: {e}", l["id"]))?,
            );
        }
        let settings = &c["settings"];
        comps.insert(*cid,json!({"name":c["name"],"width":c["width"],"height":c["height"],"fps":fps as u32,"duration":frame(num(&c["duration"])?,fps)?,"display_start":frame(num(&settings["displayStartTime"])?,fps)?,"background_color":rgb(&settings["bgColor"])?,"preserve_nested_frame_rate":settings["preserveNestedFrameRate"],"work_area":[frame(num(&settings["workAreaStart"])?,fps)?,frame(num(&settings["workAreaStart"])?+num(&settings["workAreaDuration"])?,fps)?],"layers":layers}));
    }
    let mut folders = BTreeMap::new();
    for (fid, f) in &items {
        if f["kind"] == "folder" {
            folders.insert(
                *fid,
                json!({"name":f["name"],"parent":f["parentFolderId"].as_u64().filter(|id|*id>0)}),
            );
        }
    }
    let mut composition_folders = BTreeMap::new();
    for cid in &order {
        if let Some(fid) = items[cid]["parentFolderId"]
            .as_u64()
            .filter(|id| folders.contains_key(id))
        {
            composition_folders.insert(*cid, fid);
        }
    }
    for (aid, a) in &mut adapter.assets {
        if let Some(fid) = items[aid]["parentFolderId"]
            .as_u64()
            .filter(|id| folders.contains_key(id))
        {
            a["folder"] = json!(fid);
        }
    }
    let next_asset = adapter
        .assets
        .keys()
        .chain(folders.keys())
        .max()
        .copied()
        .unwrap_or(0)
        + 1;
    let profile = compositing_profile(&adapter.resources)?;
    if profile != CompositingProfile::NativeV1 {
        for comp in comps.values_mut() {
            comp["compositing_profile"] = serde_json::to_value(profile)?;
        }
    }
    let version = if profile != CompositingProfile::NativeV1 {
        84
    } else if adapter.resources.get("mask_feather_box3_radii").is_some()
        || adapter.resources.get("mask_feather_profiles").is_some()
    {
        83
    } else if spectrum_profile(&adapter.resources)? == "HammingV1" {
        82
    } else {
        81
    };
    let native = json!({"version":version,"next_layer_id":layer_ids.last().ok_or("No layers")?+1,"composition_id":root,"next_composition_id":order.iter().max().ok_or("No compositions")?+1,"composition":comps.remove(&root).unwrap(),"other_compositions":comps,"asset_library":{"next_id":next_asset,"assets":adapter.assets,"folders":folders,"composition_folders":composition_folders}});
    let project = Project::from_json(&serde_json::to_string(&native)?)?;
    let lep = project_file::encode(&project, None)?;
    let reopened = project_file::decode(&lep)?;
    if reopened.project != project || project_file::encode(&reopened.project, None)? != lep {
        return Err("Native reopen differs".into());
    }
    let report = json!({"compositing_profile":profile,"compositing_profile_evidence":adapter.resources.get("compositing_profile_evidence"),"capture_sha256":hash(&bytes),"resources_sha256":hash(&resource_bytes),"effect_sigma_calibration":adapter.resources.get("effect_sigma"),"effect_sigma_evidence":adapter.resources.get("effect_sigma_evidence"),"mask_feather_box3_radii":adapter.resources.get("mask_feather_box3_radii"),"mask_feather_box3_evidence":adapter.resources.get("mask_feather_box3_evidence"),"mask_feather_profiles":adapter.resources.get("mask_feather_profiles"),"mask_feather_profiles_evidence":adapter.resources.get("mask_feather_profiles_evidence"),"audio_spectrum_profile":spectrum_profile(&adapter.resources)?,"audio_spectrum_profile_evidence":adapter.resources.get("audio_spectrum_profile_evidence"),"root_source_id":root,"compositions":order,"layers":layer_ids.len(),"expression_bindings":project.compositions().iter().flat_map(|(_,c)|c.layers()).map(|l|l.expressions().len()).sum::<usize>(),"font_evidence":adapter.font_evidence,"development_project":true,"pixel_and_audio_parity_verified":false,"native_roundtrip_equal":true,"remaining_requirements":adapter.issues,"source_metadata_preserved_in_capture":args[0].to_string_lossy()});
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&receipt)?
        .write_all(&serde_json::to_vec_pretty(&report)?)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?
        .write_all(&lep)?;
    println!(
        "Wrote {} compositions, {} layers, {} bytes; native reopen verified; output parity unverified",
        order.len(),
        layer_ids.len(),
        lep.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compositing_profile_is_explicit_and_bounded() {
        assert_eq!(
            compositing_profile(&json!({})).unwrap(),
            CompositingProfile::NativeV1
        );
        assert_eq!(
            compositing_profile(&json!({"compositing_profile":"OpaqueOpacityByte257V1"})).unwrap(),
            CompositingProfile::OpaqueOpacityByte257V1
        );
        for value in [
            json!(null),
            json!(true),
            json!("auto"),
            json!({"profile":"OpaqueOpacityByte257V1"}),
        ] {
            assert!(compositing_profile(&json!({"compositing_profile":value})).is_err());
        }
    }
    #[test]
    fn explicit_mixed_mask_profiles_retain_gaussian_and_reject_ambiguous_maps() {
        let resources = json!({"mask_feather_profiles":{"0":"GaussianV1","20":{"box3_radius":7.871},"50":{"box3_radius":18.93},"677":"GaussianV1"}});
        for amount in [0.0, 677.0] {
            assert_eq!(
                mask_feather_kernel(&resources, amount).unwrap(),
                MaskFeatherKernel::GaussianV1
            );
        }
        for (amount, radius) in [(20.0, 7.871), (50.0, 18.93)] {
            assert!(
                (mask_feather_kernel(&resources, amount)
                    .unwrap()
                    .box_radius(amount)
                    .unwrap()
                    - radius)
                    .abs()
                    < 1e-12
            );
        }
        assert!(mask_feather_kernel(&resources, 40.0).is_err());
        for value in [
            json!(null),
            json!(20),
            json!("auto"),
            json!({"box3_radius":7.871,"extra":true}),
            json!({"box3_radius":0.4}),
        ] {
            assert!(
                mask_feather_kernel(&json!({"mask_feather_profiles":{"20":value}}), 20.0).is_err()
            );
        }
        assert!(mask_feather_kernel(&json!({"mask_feather_profiles":{"20":"GaussianV1"},"mask_feather_box3_radii":{"20":7.871}}),20.0).is_err());
    }
    #[test]
    fn explicit_mask_box3_calibration_preserves_units_and_rejects_unmeasured_or_invalid_amounts() {
        let resources =
            json!({"mask_feather_box3_radii":{"0":0,"20":7.871,"50":18.93,"677":250.323}});
        assert_eq!(
            mask_feather_kernel(&json!({}), 20.0).unwrap(),
            MaskFeatherKernel::GaussianV1
        );
        assert_eq!(
            mask_feather_kernel(&resources, 0.0).unwrap(),
            MaskFeatherKernel::GaussianV1
        );
        for (amount, radius) in [(20.0, 7.871), (50.0, 18.93), (677.0, 250.323)] {
            let kernel = mask_feather_kernel(&resources, amount).unwrap();
            assert!((kernel.box_radius(amount).unwrap() - radius).abs() < 1e-12);
            assert_eq!(kernel.box_radius(0.0), None);
        }
        assert!(mask_feather_kernel(&resources, 40.0).is_err());
        for (amount, mapping) in [
            (0.0, json!({"0":1})),
            (2.0, json!({"2":1.0})),
            (20.0, json!({"20":0.4})),
            (20.0, json!({"20":30.0})),
            (20.0, json!([])),
        ] {
            assert!(
                mask_feather_kernel(&json!({"mask_feather_box3_radii":mapping}), amount).is_err()
            );
        }
    }
    #[test]
    fn analysis_profile_is_explicit_and_unknown_profiles_reject() {
        assert_eq!(spectrum_profile(&json!({})).unwrap(), "NativeV1");
        assert_eq!(
            spectrum_profile(&json!({"audio_spectrum_profile":"HammingV1"})).unwrap(),
            "HammingV1"
        );
        assert!(spectrum_profile(&json!({"audio_spectrum_profile":"AE"})).is_err());
        assert!(spectrum_profile(&json!({"audio_spectrum_profile":1})).is_err());
    }
    #[test]
    fn explicit_effect_unit_calibration_rejects_missing_samples_and_changed_zero() {
        let resources = json!({"effect_sigma":{"blur":{"0":0,"40":11.2}}});
        assert_eq!(effect_sigma(&resources, "blur", 40.0).unwrap(), 11.2);
        assert_eq!(effect_sigma(&resources, "blur", 0.0).unwrap(), 0.0);
        assert!(effect_sigma(&resources, "blur", 50.0).is_err());
        assert!(effect_sigma(&resources, "shadow", 40.0).is_err());
        assert!(effect_sigma(&json!({"effect_sigma":{"blur":{"0":1}}}), "blur", 0.0).is_err());
        assert!(effect_sigma(&json!({"effect_sigma":{"blur":{"40":-1}}}), "blur", 40.0).is_err());
        assert_eq!(effect_sigma(&json!({}), "blur", 40.0).unwrap(), 40.0);
    }
    #[test]
    fn timing_does_not_quantize_fractional_keys_or_signed_trim_endpoints() {
        assert_eq!(frame(1.0 / 60.0, 60.0).unwrap(), 1);
        assert!(frame(1.125, 60.0).is_err());
        assert!(frame(-1.0, 60.0).is_err());
        assert_eq!(endpoint(-1.0, 60.0), -60.0);
        assert_eq!(endpoint(2.125, 60.0), 127.5);
        assert_eq!(endpoint(1.0 / 60.0, 60.0), 1.0);
    }
    #[test]
    fn graph_cycle_and_non_composition_roots_reject_before_writing() {
        let a = json!({"id":1,"kind":"composition","layers":[{"sourceId":2}]});
        let b = json!({"id":2,"kind":"composition","layers":[{"sourceId":1}]});
        let items = [(1, &a), (2, &b)].into();
        assert!(
            visit(
                1,
                &items,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                &mut Vec::new()
            )
            .is_err()
        );
        let f = json!({"id":3,"kind":"footage"});
        assert!(
            visit(
                3,
                &[(3, &f)].into(),
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                &mut Vec::new()
            )
            .is_err()
        );
    }
    #[test]
    fn exact_rgb_and_png_payload_encoding_do_not_invent_channels() {
        assert_eq!(rgb(&json!([1.0, 0.0, 1.0, 1.0])).unwrap(), 0xff00ff);
        assert!(rgb(&json!([1.0, 0.0])).is_err());
        assert!(rgb(&json!([0.5, 0.0, 1.0])).is_err());
        assert_eq!(base64(b"\0\x01\xfe\xff"), "AAH+/w==");
    }
}
