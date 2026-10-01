use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageEncoder;
use libre_effects_core::{AlphaInterpretation, FootageInterpretation};
use std::borrow::Cow;
#[cfg(test)]
#[path = "source_render_tests.rs"]
mod tests;

pub(crate) fn apply_alpha(pixels: &mut image::RgbaImage, interpretation: FootageInterpretation) {
    for pixel in pixels.pixels_mut() {
        let alpha = f64::from(pixel[3]) / 255.0;
        match interpretation.alpha {
            AlphaInterpretation::Straight => {}
            AlphaInterpretation::Ignore => {
                pixel[3] = 255;
                continue;
            }
            AlphaInterpretation::Premultiplied { matte } => {
                for (index, shift) in [16, 8, 0].into_iter().enumerate() {
                    let background = f64::from((matte >> shift) & 255);
                    pixel[index] = if alpha == 0.0 {
                        0
                    } else {
                        ((f64::from(pixel[index]) - background * (1.0 - alpha)) / alpha)
                            .round()
                            .clamp(0.0, 255.0) as u8
                    };
                }
            }
        }
        if interpretation.invert_alpha {
            pixel[3] = 255 - pixel[3];
        }
    }
}
pub(crate) fn alpha_png(
    png: &str,
    interpretation: FootageInterpretation,
) -> Result<Cow<'_, str>, String> {
    if interpretation.alpha == AlphaInterpretation::Straight && !interpretation.invert_alpha {
        return Ok(Cow::Borrowed(png));
    }
    let bytes = STANDARD.decode(png).map_err(|e| e.to_string())?;
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    reader.limits(crate::rendering::image_limits());
    let mut pixels = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    apply_alpha(&mut pixels, interpretation);
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(Cow::Owned(STANDARD.encode(bytes)))
}
