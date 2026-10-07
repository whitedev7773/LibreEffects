use libre_effects_core::{Content, Project, ProjectItem};
#[cfg(test)]
#[path = "project_browser_tests.rs"]
mod tests;

pub(crate) use libre_effects_editor_model::project_browser::{ItemType, Row, folder_path, rows};

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
                Content::Image { png } => {
                    crate::source_render::alpha_png(png, asset.interpretation())?.into_owned()
                }
                Content::ImageSequence { .. } => crate::image_sequence::frame_png(
                    asset.content(),
                    0,
                    asset.width() as u32,
                    asset.height() as u32,
                    asset.interpretation(),
                )?
                .ok_or("First sequence frame is transparent")?,
                Content::Video { path, .. } => crate::footage::interpreted_frame_png(
                    path,
                    0.0,
                    asset.width() as u32,
                    asset.height() as u32,
                    160,
                    asset.interpretation(),
                )?,
                Content::Audio { path, audio, .. } => {
                    let peaks = crate::audio::decode_chunk(path, audio, 0)?;
                    let mut image =
                        image::RgbaImage::from_pixel(160, 100, image::Rgba([20, 20, 20, 255]));
                    for x in 0..160 {
                        let from = x as usize * peaks.len() / 160;
                        let to = ((x + 1) as usize * peaks.len() / 160)
                            .max(from + 1)
                            .min(peaks.len());
                        let (mut low, mut high) = (0.0f32, 0.0f32);
                        for p in &peaks[from..to] {
                            low = low.min(p.min);
                            high = high.max(p.max);
                        }
                        let top = (50.0 - high.clamp(0.0, 1.0) * 45.0) as u32;
                        let bottom = (50.0 - low.clamp(-1.0, 0.0) * 45.0) as u32;
                        for y in top..=bottom {
                            image.put_pixel(x, y, image::Rgba([190, 165, 100, 255]));
                        }
                    }
                    return Ok(image);
                }
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
