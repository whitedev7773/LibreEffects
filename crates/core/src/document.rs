//! Portable JSON: image payloads occur once, layer references share immutable memory.
use super::*;
use std::sync::Arc;
pub(super) const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn encode(project: &Project) -> Result<String, String> {
    let mut compact = project.clone();
    let mut assets: BTreeMap<String, Arc<str>> = BTreeMap::new();
    let mut ids = BTreeMap::new();
    for layer in compact
        .compositions_mut()
        .flat_map(|comp| comp.layers.iter_mut())
    {
        if let Content::Image { png } = &mut layer.content {
            let pointer = png.as_ptr() as usize;
            let id = ids.entry(pointer).or_insert_with(|| {
                let id = format!("image-{}", assets.len() + 1);
                assets.insert(id.clone(), png.clone());
                id
            });
            *png = Arc::from(id.as_str());
        }
    }
    let mut value = serde_json::to_value(&compact).map_err(|e| e.to_string())?;
    if !assets.is_empty() {
        value["version"] = project.version.max(7).into();
        each_layer(&mut value, |layer| {
            if let Some(image) = layer
                .get_mut("content")
                .and_then(|v| v.get_mut("Image"))
                .and_then(|v| v.as_object_mut())
            {
                let id = image.remove("png").unwrap();
                image.insert("asset".into(), id);
            }
            Ok(())
        })?;
    }
    if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > MAX_METADATA_BYTES {
        return Err("Project metadata exceeds 16 MiB".into());
    }
    if !assets.is_empty() {
        value["image_assets"] = serde_json::to_value(assets).map_err(|e| e.to_string())?;
    }
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

pub(super) fn decode(json: &str) -> Result<Project, String> {
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let assets_value = value
        .as_object_mut()
        .ok_or("Project must be an object")?
        .remove("image_assets");
    if assets_value.is_some() && !matches!(value["version"].as_u64(), Some(7..=12)) {
        return Err("Image assets require project version 7".into());
    }
    let assets: BTreeMap<String, Arc<str>> = assets_value
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    if assets.len() > 1000 || assets.values().map(|s| s.len()).sum::<usize>() > MAX_IMAGE_BYTES {
        return Err("Embedded images exceed 128 MiB".into());
    }
    let mut refs = BTreeMap::new();
    each_layer(&mut value, |layer| {
        let layer_id = layer["id"].as_u64().ok_or("Invalid layer id")?;
        if let Some(image) = layer
            .get_mut("content")
            .and_then(|v| v.get_mut("Image"))
            .and_then(|v| v.as_object_mut())
        {
            if let Some(reference) = image.remove("asset") {
                let id = reference
                    .as_str()
                    .ok_or("Invalid image asset reference")?
                    .to_owned();
                if image.contains_key("png") || !assets.contains_key(&id) {
                    return Err("Missing or ambiguous image asset".into());
                }
                refs.insert(layer_id, id);
                image.insert("png".into(), "".into());
            }
        }
        Ok(())
    })?;
    let mut project: Project = serde_json::from_value(value).map_err(|e| e.to_string())?;
    // Legacy inline images are accepted and interned as well.
    let mut intern: BTreeMap<Arc<str>, Arc<str>> = BTreeMap::new();
    for layer in project
        .compositions_mut()
        .flat_map(|comp| comp.layers.iter_mut())
    {
        if let Content::Image { png } = &mut layer.content {
            if let Some(id) = refs.get(&layer.id) {
                *png = assets[id].clone();
            }
            *png = intern
                .entry(png.clone())
                .or_insert_with(|| png.clone())
                .clone();
        }
    }
    if !intern.is_empty() && (1..=12).contains(&project.version) {
        project.version = project.version.max(7);
    }
    Ok(project)
}

fn each_layer(
    value: &mut serde_json::Value,
    mut visit: impl FnMut(&mut serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(layers) = value
        .get_mut("composition")
        .and_then(|c| c.get_mut("layers"))
        .and_then(|l| l.as_array_mut())
    {
        for layer in layers {
            visit(layer)?;
        }
    }
    if let Some(comps) = value
        .get_mut("other_compositions")
        .and_then(|c| c.as_object_mut())
    {
        for comp in comps.values_mut() {
            if let Some(layers) = comp.get_mut("layers").and_then(|l| l.as_array_mut()) {
                for layer in layers {
                    visit(layer)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn images_are_shared_across_duplicates_history_and_file_roundtrip() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Image { png: "YWJj".into() },
            width: 10.0,
            height: 10.0,
            name: "Image".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateLayers(vec![1])).unwrap();
        let json = e.project().to_json().unwrap();
        assert_eq!(json.matches("YWJj").count(), 1);
        let loaded = Project::from_json(&json).unwrap();
        assert_eq!(&loaded, e.project());
        let Content::Image { png: a } = loaded.composition.layers[0].content() else {
            panic!()
        };
        let Content::Image { png: b } = loaded.composition.layers[1].content() else {
            panic!()
        };
        assert!(Arc::ptr_eq(a, b));
        let Content::Image { png: a } = &e.current.project.composition.layers[0].content else {
            panic!()
        };
        let Content::Image { png: b } =
            &e.undo.last().unwrap().project.composition.layers[0].content
        else {
            panic!()
        };
        assert!(Arc::ptr_eq(a, b));
        let broken = json.replace("\"asset\": \"image-1\"", "\"asset\": \"missing\"");
        assert!(Project::from_json(&broken).is_err());
        let legacy = serde_json::to_string(e.project())
            .unwrap()
            .replace("\"version\":7", "\"version\":3");
        assert_eq!(Project::from_json(&legacy).unwrap(), loaded);
    }
    #[test]
    fn duplicated_large_images_no_longer_expand_the_document() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Image {
                png: Arc::from("A".repeat(9 * 1024 * 1024)),
            },
            width: 10.0,
            height: 10.0,
            name: "Image".into(),
        })
        .unwrap();
        e.execute(Command::DuplicateLayers(vec![1])).unwrap();
        let json = e.project().to_json().unwrap();
        assert!(json.len() < 10 * 1024 * 1024);
        assert_eq!(Project::from_json(&json).unwrap(), *e.project());
    }
}
