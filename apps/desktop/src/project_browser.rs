use libre_effects_core::{Content, FolderId, Project, ProjectItem};
#[cfg(test)]
#[path = "project_browser_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) struct Row {
    pub item: ProjectItem,
    pub name: String,
    pub kind: &'static str,
    pub folder: Option<FolderId>,
    pub depth: usize,
}
pub(crate) fn rows(
    project: &Project,
    query: &str,
    by_type: bool,
    descending: bool,
    collapsed: &std::collections::BTreeSet<FolderId>,
) -> Vec<Row> {
    let library = project.asset_library();
    let mut items: Vec<_> = library
        .folders()
        .iter()
        .map(|(id, f)| Row {
            item: ProjectItem::Folder(*id),
            name: f.name().into(),
            kind: "Folder",
            folder: f.parent(),
            depth: 0,
        })
        .chain(library.assets().iter().map(|(id, a)| Row {
            item: ProjectItem::Asset(*id),
            name: a.name().into(),
            kind: if matches!(a.content(), Content::Video { .. }) {
                "Video"
            } else {
                "Image"
            },
            folder: a.folder(),
            depth: 0,
        }))
        .chain(project.compositions().into_iter().map(|(id, c)| Row {
            item: ProjectItem::Composition(id),
            name: c.name().into(),
            kind: "Comp",
            folder: library.composition_folder(id),
            depth: 0,
        }))
        .collect();
    items.sort_by(|a, b| {
        let order = (if by_type {
            a.kind.cmp(b.kind)
        } else {
            std::cmp::Ordering::Equal
        })
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.item.cmp(&b.item));
        if descending { order.reverse() } else { order }
    });
    if !query.trim().is_empty() {
        let query = query.trim().to_lowercase();
        return items
            .into_iter()
            .filter(|r| {
                r.name.to_lowercase().contains(&query) || r.kind.to_lowercase().contains(&query)
            })
            .collect();
    }
    fn visit(
        items: &[Row],
        parent: Option<FolderId>,
        depth: usize,
        collapsed: &std::collections::BTreeSet<FolderId>,
        output: &mut Vec<Row>,
    ) {
        for row in items.iter().filter(|r| r.folder == parent) {
            let mut row = row.clone();
            row.depth = depth;
            let item = row.item;
            output.push(row);
            if let ProjectItem::Folder(id) = item {
                if !collapsed.contains(&id) {
                    visit(items, Some(id), depth + 1, collapsed, output);
                }
            }
        }
    }
    let mut output = Vec::new();
    visit(&items, None, 0, collapsed, &mut output);
    output
}
pub(crate) fn folder_path(project: &Project, mut folder: Option<FolderId>) -> String {
    let mut parts = Vec::new();
    for _ in 0..32 {
        let Some(f) = folder.and_then(|id| project.asset_library().folders().get(&id)) else {
            break;
        };
        parts.push(f.name().to_string());
        folder = f.parent();
    }
    parts.reverse();
    if parts.is_empty() {
        "Project".into()
    } else {
        parts.join(" / ")
    }
}
pub(crate) fn thumbnail(project: &Project, item: ProjectItem) -> Result<image::RgbaImage, String> {
    use base64::Engine;
    let mut pixels = match item {
        ProjectItem::Asset(id) => {
            let asset = project
                .asset_library()
                .assets()
                .get(&id)
                .ok_or("Asset no longer exists")?;
            let png = match asset.content() {
                Content::Image { png } => png.to_string(),
                Content::Video { path, .. } => crate::footage::frame_png(
                    path,
                    0.0,
                    asset.width() as u32,
                    asset.height() as u32,
                    160,
                )?,
                _ => return Err("Unsupported source".into()),
            };
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(png)
                .map_err(|e| e.to_string())?;
            image::load_from_memory(&bytes)
                .map_err(|e| e.to_string())?
                .thumbnail(160, 100)
                .to_rgba8()
        }
        ProjectItem::Composition(id) => {
            let mut copy = project.clone();
            copy.activate_composition(id)?;
            crate::rendering::Renderer::new().render_preview(&copy, 0, 160)?
        }
        ProjectItem::Folder(_) => return Err("Folder".into()),
    };
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(pixels)
}
