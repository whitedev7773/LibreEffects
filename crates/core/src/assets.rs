//! Persistent project items. Layers keep their own timing, transforms and effects.
use super::*;
#[cfg(test)]
#[path = "asset_tests.rs"]
mod tests;

pub type AssetId = u64;
pub type FolderId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectItem {
    Asset(AssetId),
    Folder(FolderId),
    Composition(CompositionId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    #[serde(default, skip_serializing_if = "FootageInterpretation::is_default")]
    pub(super) interpretation: FootageInterpretation,
    name: String,
    folder: Option<FolderId>,
    width: f64,
    height: f64,
    pub(super) content: Content,
}
impl MediaAsset {
    pub fn interpretation(&self) -> FootageInterpretation {
        self.interpretation
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn folder(&self) -> Option<FolderId> {
        self.folder
    }
    pub fn width(&self) -> f64 {
        self.width
    }
    pub fn height(&self) -> f64 {
        self.height
    }
    pub fn content(&self) -> &Content {
        &self.content
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFolder {
    name: String,
    parent: Option<FolderId>,
}
impl ProjectFolder {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn parent(&self) -> Option<FolderId> {
        self.parent
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetLibrary {
    next_id: u64,
    pub(super) assets: BTreeMap<AssetId, MediaAsset>,
    folders: BTreeMap<FolderId, ProjectFolder>,
    composition_folders: BTreeMap<CompositionId, FolderId>,
}
impl Default for AssetLibrary {
    fn default() -> Self {
        Self {
            next_id: 1,
            assets: BTreeMap::new(),
            folders: BTreeMap::new(),
            composition_folders: BTreeMap::new(),
        }
    }
}
impl AssetLibrary {
    pub(super) fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn assets(&self) -> &BTreeMap<AssetId, MediaAsset> {
        &self.assets
    }
    pub fn folders(&self) -> &BTreeMap<FolderId, ProjectFolder> {
        &self.folders
    }
    pub fn composition_folder(&self, id: CompositionId) -> Option<FolderId> {
        self.composition_folders.get(&id).copied()
    }
    fn allocate(&mut self) -> Result<u64, String> {
        if self.next_id >= u64::MAX - 1 {
            return Err("Project item ID limit reached".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        Ok(id)
    }
    fn import(
        &mut self,
        content: Content,
        width: f64,
        height: f64,
        name: String,
        folder: Option<FolderId>,
        interpretation: FootageInterpretation,
    ) -> Result<AssetId, String> {
        let content =
            source(&content).ok_or("Only image and video footage can be imported as assets")?;
        if folder.is_some_and(|id| !self.folders.contains_key(&id)) {
            return Err("Destination folder no longer exists".into());
        }
        if let Some((id, _)) = self.assets.iter().find(|(_, a)| {
            a.content == content
                && a.width == width
                && a.height == height
                && a.interpretation == interpretation
        }) {
            return Ok(*id);
        }
        let id = self.allocate()?;
        self.assets.insert(
            id,
            MediaAsset {
                interpretation,
                name: if name.trim().is_empty() {
                    "Untitled footage".into()
                } else {
                    name
                },
                folder,
                width,
                height,
                content,
            },
        );
        Ok(id)
    }
    pub(super) fn remove_composition(&mut self, id: CompositionId) {
        self.composition_folders.remove(&id);
    }
    pub(super) fn copy_composition(&mut self, from: CompositionId, to: CompositionId) {
        if let Some(folder) = self.composition_folder(from) {
            self.composition_folders.insert(to, folder);
        }
    }
}
pub(super) fn source(content: &Content) -> Option<Content> {
    Some(match content {
        Content::Image { .. } => content.clone(),
        Content::Video {
            path,
            duration,
            source_fps,
            ..
        } => Content::Video {
            path: path.clone(),
            duration: *duration,
            source_fps: *source_fps,
            start_frame: 0,
            playback: Default::default(),
        },
        Content::ImageSequence {
            frames,
            fps,
            missing,
            ..
        } => Content::ImageSequence {
            frames: frames.clone(),
            fps: *fps,
            missing: *missing,
            start_frame: 0,
            playback: Default::default(),
        },
        _ => return None,
    })
}
impl Layer {
    pub fn asset_id(&self) -> Option<AssetId> {
        self.asset
    }
}
impl Project {
    pub fn asset_library(&self) -> &AssetLibrary {
        &self.asset_library
    }
    pub fn asset_references(&self, id: AssetId) -> usize {
        self.compositions()
            .iter()
            .map(|(_, c)| c.layers.iter().filter(|l| l.asset == Some(id)).count())
            .sum()
    }
    pub(super) fn sync_assets(&mut self) -> Result<(), String> {
        let library = &mut self.asset_library;
        for layer in std::iter::once(&mut self.composition)
            .chain(self.other_compositions.values_mut())
            .flat_map(|c| &mut c.layers)
        {
            if layer.asset.is_none() && source(&layer.content).is_some() {
                let id = library.import(
                    layer.content.clone(),
                    layer.width,
                    layer.height,
                    layer.name.clone(),
                    None,
                    layer.footage_interpretation,
                )?;
                layer.asset = Some(id);
                if let Content::Image { png } = &library.assets[&id].content {
                    layer.content = Content::Image { png: png.clone() };
                }
            }
        }
        if !library.is_default() {
            self.version = 22;
        }
        if library
            .assets
            .values()
            .any(|a| !a.interpretation.is_default())
        {
            self.version = 23;
        }
        if library
            .assets
            .values()
            .any(|a| matches!(a.content, Content::ImageSequence { .. }))
        {
            self.version = 24;
        }
        Ok(())
    }
}
pub(super) fn validate(project: &Project) -> Result<(), String> {
    let library = &project.asset_library;
    if project.version < 22
        && (!library.is_default()
            || project
                .compositions()
                .iter()
                .any(|(_, c)| c.layers.iter().any(|l| l.asset.is_some())))
    {
        return Err("Project assets require version 22".into());
    }
    if library.assets.len() > 1000
        || library.folders.len() > 1000
        || library.next_id == 0
        || library.next_id == u64::MAX
    {
        return Err("Project supports at most 1000 media assets and 1000 folders".into());
    }
    let valid_folder = |id: Option<FolderId>| id.is_none_or(|id| library.folders.contains_key(&id));
    let valid_id = |id: u64| id > 0 && id < library.next_id;
    for (id, asset) in &library.assets {
        if project.version < 24 && matches!(asset.content, Content::ImageSequence { .. }) {
            return Err("Image sequences require version 24".into());
        }
        asset.interpretation.validate(&asset.content)?;
        if project.version < 23 && !asset.interpretation.is_default() {
            return Err("Footage interpretation requires version 23".into());
        }
        if !valid_id(*id)
            || library.folders.contains_key(id)
            || asset.name.trim().is_empty()
            || asset.name.len() > 1024
            || !valid_folder(asset.folder)
            || ![asset.width, asset.height]
                .iter()
                .all(|n| n.is_finite() && (1.0..=16384.0).contains(n))
            || source(&asset.content).as_ref() != Some(&asset.content)
        {
            return Err("Invalid media asset".into());
        }
        editing::validate_content(&asset.content, Effects::default(), None)?;
    }
    for (id, folder) in &library.folders {
        if !valid_id(*id)
            || folder.name.trim().is_empty()
            || folder.name.len() > 1024
            || !valid_folder(folder.parent)
        {
            return Err("Invalid project folder".into());
        }
        let mut seen = BTreeSet::from([*id]);
        let mut parent = folder.parent;
        while let Some(id) = parent {
            if !seen.insert(id) || seen.len() > 32 {
                return Err("Folders cannot contain cycles or exceed 32 levels".into());
            }
            parent = library
                .folders
                .get(&id)
                .ok_or("Missing parent folder")?
                .parent;
        }
    }
    for (id, folder) in &library.composition_folders {
        if project.composition_by_id(*id).is_none() || !valid_folder(Some(*folder)) {
            return Err("Invalid composition folder".into());
        }
    }
    for (_, comp) in project.compositions() {
        for layer in &comp.layers {
            if project.version < 24 && matches!(layer.content, Content::ImageSequence { .. }) {
                return Err("Image sequences require version 24".into());
            }
            layer.footage_interpretation.validate(&layer.content)?;
            if project.version < 23 && !layer.footage_interpretation.is_default() {
                return Err("Footage interpretation requires version 23".into());
            }
            if let Some(id) = layer.asset {
                let a = library.assets.get(&id).ok_or("Missing layer asset")?;
                if source(&layer.content).as_ref() != Some(&a.content)
                    || layer.width != a.width
                    || layer.height != a.height
                    || layer.footage_interpretation != a.interpretation
                {
                    return Err("Layer source does not match its shared asset".into());
                }
            } else if project.version >= 22 && source(&layer.content).is_some() {
                return Err("Media layer is missing its asset ID".into());
            }
        }
    }
    Ok(())
}
pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    if !matches!(
        command,
        Command::ImportAsset { .. }
            | Command::AddAssetLayer { .. }
            | Command::NewProjectFolder { .. }
            | Command::RenameProjectItem { .. }
            | Command::MoveProjectItem { .. }
            | Command::DeleteProjectItem(_)
    ) {
        return None;
    }
    Some((|| {
        match command {
            Command::ImportAsset {
                content,
                width,
                height,
                name,
                folder,
                frame,
            } => {
                let id = state.project.asset_library.import(
                    content.clone(),
                    *width,
                    *height,
                    name.clone(),
                    *folder,
                    Default::default(),
                )?;
                if let Some(frame) = frame {
                    add_layer(state, id, *frame)?;
                }
            }
            Command::AddAssetLayer { asset, frame } => add_layer(state, *asset, *frame)?,
            Command::NewProjectFolder { name, parent } => {
                let library = &mut state.project.asset_library;
                let id = library.allocate()?;
                library.folders.insert(
                    id,
                    ProjectFolder {
                        name: name.clone(),
                        parent: *parent,
                    },
                );
            }
            Command::RenameProjectItem { item, name } => {
                if name.trim().is_empty() || name.len() > 1024 {
                    return Err("Enter a name between 1 and 1024 bytes".into());
                }
                let project = &mut state.project;
                match item {
                    ProjectItem::Asset(id) => {
                        project
                            .asset_library
                            .assets
                            .get_mut(id)
                            .ok_or("Asset not found")?
                            .name = name.clone()
                    }
                    ProjectItem::Folder(id) => {
                        project
                            .asset_library
                            .folders
                            .get_mut(id)
                            .ok_or("Folder not found")?
                            .name = name.clone()
                    }
                    ProjectItem::Composition(id) => {
                        let comp = if *id == project.composition_id {
                            &mut project.composition
                        } else {
                            project
                                .other_compositions
                                .get_mut(id)
                                .ok_or("Composition not found")?
                        };
                        comp.name = name.clone();
                    }
                }
            }
            Command::MoveProjectItem { item, folder } => {
                let project = &mut state.project;
                if folder.is_some_and(|id| !project.asset_library.folders.contains_key(&id)) {
                    return Err("Folder not found".into());
                }
                match item {
                    ProjectItem::Asset(id) => {
                        project
                            .asset_library
                            .assets
                            .get_mut(id)
                            .ok_or("Asset not found")?
                            .folder = *folder
                    }
                    ProjectItem::Folder(id) => {
                        project
                            .asset_library
                            .folders
                            .get_mut(id)
                            .ok_or("Folder not found")?
                            .parent = *folder
                    }
                    ProjectItem::Composition(id) => {
                        if project.composition_by_id(*id).is_none() {
                            return Err("Composition not found".into());
                        }
                        if let Some(folder) = folder {
                            project
                                .asset_library
                                .composition_folders
                                .insert(*id, *folder);
                        } else {
                            project.asset_library.composition_folders.remove(id);
                        }
                    }
                }
            }
            Command::DeleteProjectItem(item) => {
                let project = &mut state.project;
                match *item {
                    ProjectItem::Asset(id) => {
                        if project.asset_references(id) != 0 {
                            return Err("Remove this asset's layers from all compositions before deleting it".into());
                        }
                        project
                            .asset_library
                            .assets
                            .remove(&id)
                            .ok_or("Asset not found")?;
                    }
                    ProjectItem::Folder(id) => {
                        let library = &mut project.asset_library;
                        if library.assets.values().any(|a| a.folder == Some(id))
                            || library.folders.values().any(|f| f.parent == Some(id))
                            || library.composition_folders.values().any(|f| *f == id)
                        {
                            return Err("Move or delete the folder's contents first".into());
                        }
                        library.folders.remove(&id).ok_or("Folder not found")?;
                    }
                    ProjectItem::Composition(_) => {
                        return Err("Open the composition and use Delete composition".into());
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}
fn add_layer(state: &mut Snapshot, asset: AssetId, frame: Frame) -> Result<(), String> {
    if frame >= state.project.composition.duration {
        return Err("Playhead is outside the composition".into());
    }
    let a = state
        .project
        .asset_library
        .assets
        .get(&asset)
        .ok_or("Asset no longer exists")?
        .clone();
    let mut content = a.content;
    let interpreted_duration = a.interpretation.duration(&content);
    if let Content::Video { start_frame, .. } | Content::ImageSequence { start_frame, .. } =
        &mut content
    {
        *start_frame = i64::from(frame);
    }
    super::apply(
        state,
        Command::AddContent {
            content,
            width: a.width,
            height: a.height,
            name: a.name,
        },
    )?;
    let layer = state
        .project
        .composition
        .layers
        .iter_mut()
        .find(|l| Some(l.id) == state.selected)
        .unwrap();
    layer.asset = Some(asset);
    layer.footage_interpretation = a.interpretation;
    layer.in_frame = frame;
    if let Some(seconds) = interpreted_duration {
        let comp = &state.project.composition;
        let end = (u64::from(frame) + (seconds * comp.fps.as_f64() - 1e-7).ceil().max(1.0) as u64)
            .min(u64::from(comp.duration)) as Frame;
        state
            .project
            .composition
            .layers
            .iter_mut()
            .find(|l| Some(l.id) == state.selected)
            .unwrap()
            .out_frame = Some(end);
    }
    Ok(())
}
