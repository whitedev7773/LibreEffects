//! Shared portable metadata and asset resolution for JSON and native projects.
use super::*;
use std::sync::Arc;
pub(super) const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;

// The desktop file/recovery envelope has the same cap. Leave room for its
// optional view state when deciding whether a large document needs compact JSON.
const MAX_PROJECT_BYTES: usize = 256 * 1024 * 1024;
const MAX_PRETTY_BYTES: usize = MAX_PROJECT_BYTES - MAX_METADATA_BYTES;
const METADATA_LIMIT_ERROR: &str = "Project metadata exceeds 16 MiB";

struct Prepared {
    project: Project,
    images: BTreeMap<String, Arc<str>>,
    sequences: BTreeMap<String, Arc<Vec<String>>>,
    image_references: usize,
}

#[derive(Serialize)]
struct Metadata<'a> {
    #[serde(flatten)]
    project: &'a Project,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    sequence_assets: &'a BTreeMap<String, Arc<Vec<String>>>,
}

impl Prepared {
    fn new(project: &Project) -> Self {
        let mut project = project.clone();
        let sequences = super::image_sequence::compact(&mut project);
        let mut images: BTreeMap<String, Arc<str>> = BTreeMap::new();
        let mut ids = BTreeMap::new();
        let mut image_references = 0;
        let mut visit = |content: &mut Content| {
            if let Content::Image { png } = content {
                image_references += 1;
                let id = ids.entry(png.as_ptr() as usize).or_insert_with(|| {
                    let id = format!("image-{}", images.len() + 1);
                    images.insert(id.clone(), png.clone());
                    id
                });
                *png = Arc::from(id.as_str());
            }
        };
        for asset in project.asset_library.assets.values_mut() {
            visit(&mut asset.content);
        }
        for layer in project.compositions_mut().flat_map(|comp| &mut comp.layers) {
            visit(&mut layer.content);
        }
        if !images.is_empty() {
            project.version = project.version.max(7);
        }
        Self {
            project,
            images,
            sequences,
            image_references,
        }
    }

    fn metadata(&self) -> Metadata<'_> {
        Metadata {
            project: &self.project,
            sequence_assets: &self.sequences,
        }
    }

    fn validate_budget(&self) -> Result<(), String> {
        if self.images.len() > 1000
            || self
                .images
                .values()
                .fold(0usize, |sum, image| sum.saturating_add(image.len()))
                > MAX_IMAGE_BYTES
        {
            return Err("Embedded images exceed 128 MiB".into());
        }
        if self.sequences.len() > 1000 {
            return Err("Sequence manifests exceed project metadata limits".into());
        }
        // The on-disk image key is `asset` instead of `png` (+2 bytes per
        // reference). For sequences, `frames:["id"]` and `manifest:"id"`
        // have identical compact lengths. Count the actual escaped JSON and
        // shared manifests once, without allocating JSON or copying image data.
        count_json(
            &self.metadata(),
            false,
            self.image_references * 2,
            MAX_METADATA_BYTES,
            METADATA_LIMIT_ERROR,
        )?;
        Ok(())
    }
}

/// Check a structurally validated candidate before it can enter editor history.
/// The 16 MiB metadata + 128 MiB image budgets leave compact output comfortably
/// below the 256 MiB file/recovery limit, even including asset-table overhead.
pub(super) fn validate_budget(project: &Project) -> Result<(), String> {
    Prepared::new(project).validate_budget()
}

/// A bounded sink: serialization stops at the limit and never stores JSON bytes.
struct CountingWriter {
    bytes: usize,
    limit: usize,
    message: &'static str,
}
impl std::io::Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|size| *size <= self.limit)
            .ok_or_else(|| std::io::Error::other(self.message))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count_json(
    value: &impl Serialize,
    pretty: bool,
    initial_bytes: usize,
    limit: usize,
    message: &'static str,
) -> Result<usize, String> {
    if initial_bytes > limit {
        return Err(message.into());
    }
    let mut writer = CountingWriter {
        bytes: initial_bytes,
        limit,
        message,
    };
    if pretty {
        serde_json::to_writer_pretty(&mut writer, value)
    } else {
        serde_json::to_writer(&mut writer, value)
    }
    .map_err(|error| error.to_string())?;
    Ok(writer.bytes)
}

impl Prepared {
    fn metadata_value(&self) -> Result<serde_json::Value, String> {
        let mut value = serde_json::to_value(self.metadata()).map_err(|e| e.to_string())?;
        if !self.images.is_empty() {
            each_source(&mut value, |_, layer| {
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
        if !self.sequences.is_empty() {
            each_source(&mut value, |_, source| {
                if let Some(sequence) = source
                    .get_mut("content")
                    .and_then(|c| c.get_mut("ImageSequence"))
                    .and_then(|s| s.as_object_mut())
                {
                    let frames = sequence
                        .remove("frames")
                        .ok_or("Missing sequence manifest")?;
                    sequence.insert("manifest".into(), frames[0].clone());
                }
                Ok(())
            })?;
        }
        Ok(value)
    }
}

pub(super) struct NativeMetadata {
    pub metadata: Vec<u8>,
    pub images: BTreeMap<String, Arc<str>>,
}

/// No image payload enters a JSON value or serialized metadata buffer.
pub(super) fn prepare_native(project: &Project) -> Result<NativeMetadata, String> {
    let prepared = Prepared::new(project);
    prepared.validate_budget()?;
    let metadata = serde_json::to_vec(&prepared.metadata_value()?).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(METADATA_LIMIT_ERROR.into());
    }
    Ok(NativeMetadata {
        metadata,
        images: prepared.images,
    })
}

pub(super) fn encode(project: &Project) -> Result<String, String> {
    let prepared = Prepared::new(project);
    prepared.validate_budget()?;
    let mut value = prepared.metadata_value()?;
    if !prepared.images.is_empty() {
        value["image_assets"] = serde_json::to_value(prepared.images).map_err(|e| e.to_string())?;
    }
    encode_value(&value, MAX_PRETTY_BYTES)
}

fn encode_value(value: &serde_json::Value, pretty_limit: usize) -> Result<String, String> {
    // Deeply nested arrays can expand far beyond the compact metadata budget.
    // Fall back before allocating that oversized pretty representation.
    let json = if count_json(value, true, 0, pretty_limit, "Project needs compact JSON").is_ok() {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
    .map_err(|e| e.to_string())?;
    if json.len() > MAX_PROJECT_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    Ok(json)
}

pub(super) fn decode(json: &str) -> Result<Project, String> {
    if json.len() > MAX_PROJECT_BYTES {
        return Err("Project exceeds 256 MiB".into());
    }
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let assets_value = value
        .as_object_mut()
        .ok_or("Project must be an object")?
        .remove("image_assets");
    if assets_value.is_some()
        && !value["version"]
            .as_u64()
            .is_some_and(|v| (7..=u64::from(PROJECT_VERSION)).contains(&v))
    {
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
    resolve(value, assets, false)
}

pub(super) fn decode_native(
    value: serde_json::Value,
    images: BTreeMap<String, Arc<str>>,
) -> Result<Project, String> {
    let object = value.as_object().ok_or("Project must be an object")?;
    if object.contains_key("image_assets") {
        return Err("Native metadata must not contain inline image_assets".into());
    }
    if !images.is_empty()
        && !value["version"]
            .as_u64()
            .is_some_and(|v| (7..=u64::from(PROJECT_VERSION)).contains(&v))
    {
        return Err("Image assets require project version 7".into());
    }
    finish(resolve(value, images, true)?)
}

/// Both formats use the same migrations and validate the final resident model.
pub(super) fn finish(mut project: Project) -> Result<Project, String> {
    project.validate()?;
    mask_animation::migrate(&mut project);
    shape_contents::migrate(&mut project);
    project.sync_assets()?;
    project.validate()?;
    validate_budget(&project)?;
    Ok(project)
}

fn resolve(
    mut value: serde_json::Value,
    assets: BTreeMap<String, Arc<str>>,
    native: bool,
) -> Result<Project, String> {
    let sequences_value = value.as_object_mut().unwrap().remove("sequence_assets");
    if sequences_value.is_some()
        && !value["version"]
            .as_u64()
            .is_some_and(|v| (24..=u64::from(PROJECT_VERSION)).contains(&v))
    {
        return Err("Sequence manifests require version 24".into());
    }
    let sequences: BTreeMap<String, Arc<Vec<String>>> = sequences_value
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    if sequences.len() > 1000
        || sequences
            .values()
            .map(|f| f.iter().map(String::len).sum::<usize>())
            .sum::<usize>()
            > MAX_METADATA_BYTES
    {
        return Err("Sequence manifests exceed project metadata limits".into());
    }
    let mut sequence_refs = BTreeMap::new();
    let mut refs = BTreeMap::new();
    each_source(&mut value, |source_id, layer| {
        if let Some(sequence) = layer
            .get_mut("content")
            .and_then(|c| c.get_mut("ImageSequence"))
            .and_then(|s| s.as_object_mut())
        {
            if let Some(reference) = sequence.remove("manifest") {
                let id = reference
                    .as_str()
                    .ok_or("Invalid sequence manifest reference")?
                    .to_owned();
                if sequence.contains_key("frames") || !sequences.contains_key(&id) {
                    return Err("Missing or ambiguous sequence manifest".into());
                }
                sequence_refs.insert(source_id.clone(), id);
                sequence.insert("frames".into(), serde_json::json!([]));
            }
        }
        if let Some(image) = layer
            .get_mut("content")
            .and_then(|v| v.get_mut("Image"))
            .and_then(|v| v.as_object_mut())
        {
            if native && image.contains_key("png") {
                return Err("Native metadata must not contain inline image png".into());
            }
            if let Some(reference) = image.remove("asset") {
                let id = reference
                    .as_str()
                    .ok_or("Invalid image asset reference")?
                    .to_owned();
                if image.contains_key("png") || !assets.contains_key(&id) {
                    return Err("Missing or ambiguous image asset".into());
                }
                refs.insert(source_id, id);
                image.insert("png".into(), "".into());
            }
        }
        Ok(())
    })?;
    if native {
        let used_sequences: BTreeSet<_> = sequence_refs.values().collect();
        if sequences.keys().any(|id| !used_sequences.contains(id)) {
            return Err("Unreferenced native sequence manifest".into());
        }
        let used: BTreeSet<_> = refs.values().collect();
        if assets.keys().any(|id| !used.contains(id)) {
            return Err("Unreferenced native image chunk".into());
        }
    }
    let mut project: Project = serde_json::from_value(value).map_err(|e| e.to_string())?;
    // Legacy inline images are accepted and interned as well.
    let mut intern: BTreeMap<Arc<str>, Arc<str>> = BTreeMap::new();
    for (id, asset) in &mut project.asset_library.assets {
        if let Content::ImageSequence { frames, .. } = &mut asset.content {
            if let Some(id) = sequence_refs.get(&format!("asset-{id}")) {
                *frames = sequences[id].clone();
            }
        }
        if let Content::Image { png } = &mut asset.content {
            if let Some(id) = refs.get(&format!("asset-{id}")) {
                *png = assets[id].clone();
            }
            *png = intern
                .entry(png.clone())
                .or_insert_with(|| png.clone())
                .clone();
        }
    }
    for layer in project
        .compositions_mut()
        .flat_map(|comp| comp.layers.iter_mut())
    {
        if let Content::ImageSequence { frames, .. } = &mut layer.content {
            if let Some(id) = sequence_refs.get(&format!("layer-{}", layer.id)) {
                *frames = sequences[id].clone();
            }
        }
        if let Content::Image { png } = &mut layer.content {
            if let Some(id) = refs.get(&format!("layer-{}", layer.id)) {
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

fn each_source(
    value: &mut serde_json::Value,
    mut visit: impl FnMut(String, &mut serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(assets) = value
        .get_mut("asset_library")
        .and_then(|v| v.get_mut("assets"))
        .and_then(|v| v.as_object_mut())
    {
        for (id, asset) in assets {
            visit(format!("asset-{id}"), asset)?;
        }
    }
    if let Some(layers) = value
        .get_mut("composition")
        .and_then(|c| c.get_mut("layers"))
        .and_then(|l| l.as_array_mut())
    {
        for layer in layers {
            let id = layer["id"].as_u64().ok_or("Invalid layer id")?;
            visit(format!("layer-{id}"), layer)?;
        }
    }
    if let Some(comps) = value
        .get_mut("other_compositions")
        .and_then(|c| c.as_object_mut())
    {
        for comp in comps.values_mut() {
            if let Some(layers) = comp.get_mut("layers").and_then(|l| l.as_array_mut()) {
                for layer in layers {
                    let id = layer["id"].as_u64().ok_or("Invalid layer id")?;
                    visit(format!("layer-{id}"), layer)?;
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
        let mut legacy = serde_json::to_value(e.project()).unwrap();
        legacy["version"] = 3.into();
        legacy.as_object_mut().unwrap().remove("asset_library");
        for layer in legacy["composition"]["layers"].as_array_mut().unwrap() {
            layer.as_object_mut().unwrap().remove("asset");
        }
        let legacy = Project::from_json(&legacy.to_string()).unwrap();
        assert_eq!(legacy.asset_library.assets.len(), 1);
        assert_eq!(
            legacy.composition.layers[0].content,
            loaded.composition.layers[0].content
        );
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

#[cfg(test)]
#[path = "document_budget_tests.rs"]
mod budget_tests;
