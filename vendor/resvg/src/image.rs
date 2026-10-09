// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub fn render(
    image: &usvg::Image,
    ctx: &crate::render::Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    if !image.is_visible() {
        return;
    }

    render_inner(image.kind(), ctx, transform, image.rendering_mode(), pixmap);
}

pub fn render_inner(
    image_kind: &usvg::ImageKind,
    ctx: &crate::render::Context,
    transform: tiny_skia::Transform,
    #[allow(unused_variables)] rendering_mode: usvg::ImageRendering,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    let _profile = crate::profile::time("Image");
    match image_kind {
        usvg::ImageKind::SVG(ref tree) => {
            render_vector(tree, ctx, transform, pixmap);
        }
        #[cfg(feature = "raster-images")]
        _ => {
            raster_images::render_raster(image_kind, transform, rendering_mode, pixmap);
        }
        #[cfg(not(feature = "raster-images"))]
        _ => {
            log::warn!("Images decoding was disabled by a build feature.");
        }
    }
}

fn render_vector(
    tree: &usvg::Tree,
    ctx: &crate::render::Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    let _live;
    let mut sub_pixmap = if let Some(state) = ctx.checked {
        let allocation = (|| {
            let bytes =
                crate::checked::image_bytes(pixmap.width(), pixmap.height(), state.options.limits)?;
            let guard = state.reserve(bytes)?;
            let image =
                crate::checked::pixmap(pixmap.width(), pixmap.height(), state.options.limits)?;
            Ok::<_, crate::RenderErrorKind>((image, guard))
        })();
        match allocation {
            Ok((image, guard)) => {
                _live = Some(guard);
                image
            }
            Err(kind) => {
                state.fail(crate::RenderError::new(kind));
                return None;
            }
        }
    } else {
        _live = None;
        tiny_skia::Pixmap::new(pixmap.width(), pixmap.height()).unwrap()
    };
    if ctx.checked.is_some() {
        crate::render::render_nodes(tree.root(), ctx, transform, &mut sub_pixmap.as_mut());
    } else {
        crate::render(tree, transform, &mut sub_pixmap.as_mut());
    }
    pixmap.draw_pixmap(
        0,
        0,
        sub_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::default(),
        None,
    );

    Some(())
}

#[cfg(feature = "raster-images")]
mod raster_images {
    use crate::OptionLog;
    use usvg::ImageRendering;

    fn decode_raster(image: &usvg::ImageKind) -> Option<tiny_skia::Pixmap> {
        match image {
            usvg::ImageKind::SVG(_) => None,
            usvg::ImageKind::JPEG(ref data) => {
                decode_jpeg(data).log_none(|| log::warn!("Failed to decode a JPEG image."))
            }
            usvg::ImageKind::PNG(ref data) => {
                decode_png(data).log_none(|| log::warn!("Failed to decode a PNG image."))
            }
            usvg::ImageKind::GIF(ref data) => {
                decode_gif(data).log_none(|| log::warn!("Failed to decode a GIF image."))
            }
            usvg::ImageKind::WEBP(ref data) => {
                decode_webp(data).log_none(|| log::warn!("Failed to decode a WebP image."))
            }
        }
    }

    fn decode_png(data: &[u8]) -> Option<tiny_skia::Pixmap> {
        tiny_skia::Pixmap::decode_png(data).ok()
    }

    fn decode_jpeg(data: &[u8]) -> Option<tiny_skia::Pixmap> {
        use zune_jpeg::zune_core::colorspace::ColorSpace;
        use zune_jpeg::zune_core::options::DecoderOptions;

        let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
        let mut decoder = zune_jpeg::JpegDecoder::new_with_options(data, options);
        decoder.decode_headers().ok()?;
        let output_cs = decoder.get_output_colorspace()?;

        let img_data = {
            let data = decoder.decode().ok()?;
            match output_cs {
                ColorSpace::RGBA => data,
                // `set_output_color_space` is not guaranteed to actually always set the output space
                // to RGBA (its docs say "we do not guarantee the decoder can convert to all colorspaces").
                // In particular, it seems like it doesn't work for luma JPEGs,
                // so we convert them manually.
                ColorSpace::Luma => data
                    .into_iter()
                    .flat_map(|p| [p, p, p, 255])
                    .collect::<Vec<_>>(),
                _ => return None,
            }
        };

        let info = decoder.info()?;

        let size = tiny_skia::IntSize::from_wh(info.width as u32, info.height as u32)?;
        tiny_skia::Pixmap::from_vec(img_data, size)
    }

    fn decode_gif(data: &[u8]) -> Option<tiny_skia::Pixmap> {
        let mut decoder = gif::DecodeOptions::new();
        decoder.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = decoder.read_info(data).ok()?;
        let first_frame = decoder.read_next_frame().ok()??;

        let size = tiny_skia::IntSize::from_wh(
            u32::from(first_frame.width),
            u32::from(first_frame.height),
        )?;

        let (w, h) = size.dimensions();
        let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
        rgba_to_pixmap(&first_frame.buffer, &mut pixmap);
        Some(pixmap)
    }

    fn decode_webp(data: &[u8]) -> Option<tiny_skia::Pixmap> {
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(data)).ok()?;
        let mut first_frame = vec![0; decoder.output_buffer_size()?];
        decoder.read_image(&mut first_frame).ok()?;

        let (w, h) = decoder.dimensions();
        let mut pixmap = tiny_skia::Pixmap::new(w, h)?;

        if decoder.has_alpha() {
            rgba_to_pixmap(&first_frame, &mut pixmap);
        } else {
            rgb_to_pixmap(&first_frame, &mut pixmap);
        }

        Some(pixmap)
    }

    fn rgb_to_pixmap(data: &[u8], pixmap: &mut tiny_skia::Pixmap) {
        use rgb::FromSlice;

        let mut i = 0;
        let dst = pixmap.data_mut();
        for p in data.as_rgb() {
            dst[i + 0] = p.r;
            dst[i + 1] = p.g;
            dst[i + 2] = p.b;
            dst[i + 3] = 255;

            i += tiny_skia::BYTES_PER_PIXEL;
        }
    }

    fn rgba_to_pixmap(data: &[u8], pixmap: &mut tiny_skia::Pixmap) {
        use rgb::FromSlice;

        let mut i = 0;
        let dst = pixmap.data_mut();
        for p in data.as_rgba() {
            let a = p.a as f64 / 255.0;
            dst[i + 0] = (p.r as f64 * a + 0.5) as u8;
            dst[i + 1] = (p.g as f64 * a + 0.5) as u8;
            dst[i + 2] = (p.b as f64 * a + 0.5) as u8;
            dst[i + 3] = p.a;

            i += tiny_skia::BYTES_PER_PIXEL;
        }
    }

    pub(crate) fn render_raster(
        image: &usvg::ImageKind,
        transform: tiny_skia::Transform,
        rendering_mode: usvg::ImageRendering,
        pixmap: &mut tiny_skia::PixmapMut,
    ) -> Option<()> {
        let mut raster = if let Some(pixels) = crate::raster_cache::get(image, &[]) {
            pixels
        } else {
            let pixels = std::sync::Arc::new(decode_raster(image)?);
            crate::raster_cache::insert(image, &[], pixels.clone());
            pixels
        };

        let rect = tiny_skia::Size::from_wh(raster.width() as f32, raster.height() as f32)?
            .to_rect(0.0, 0.0)?;

        let quality = match rendering_mode {
            ImageRendering::OptimizeQuality => tiny_skia::FilterQuality::Bicubic,
            ImageRendering::OptimizeSpeed => tiny_skia::FilterQuality::Nearest,
            ImageRendering::Smooth => tiny_skia::FilterQuality::Bilinear,
            ImageRendering::HighQuality => tiny_skia::FilterQuality::Bicubic,
            ImageRendering::CrispEdges => tiny_skia::FilterQuality::Nearest,
            ImageRendering::Pixelated => tiny_skia::FilterQuality::Nearest,
        };

        // A fixed bicubic footprint aliases when many source pixels map to one
        // output pixel. Average premultiplied coverage before sampling; keep the
        // asset's original coordinate extent and pixel-art behavior intact.
        if matches!(
            rendering_mode,
            ImageRendering::OptimizeQuality | ImageRendering::HighQuality | ImageRendering::Smooth
        ) {
            let scale_x = transform.sx.hypot(transform.ky);
            let scale_y = transform.kx.hypot(transform.sy);
            let mut reductions = Vec::new();
            loop {
                let x = scale_x * rect.width() / raster.width() as f32;
                let y = scale_y * rect.height() / raster.height() as f32;
                let width = if x < 0.5 && raster.width() > 1 {
                    raster.width().div_ceil(2)
                } else {
                    raster.width()
                };
                let height = if y < 0.5 && raster.height() > 1 {
                    raster.height().div_ceil(2)
                } else {
                    raster.height()
                };
                if width == raster.width() && height == raster.height() {
                    break;
                }
                // Every reduction rounds premultiplied coverage. Different
                // anisotropic paths to the same dimensions are distinct keys.
                reductions.push((width, height));
                raster = if let Some(pixels) = crate::raster_cache::get(image, &reductions) {
                    pixels
                } else {
                    let pixels = std::sync::Arc::new(area_reduce(&raster, width, height)?);
                    crate::raster_cache::insert(image, &reductions, pixels.clone());
                    pixels
                };
            }
        }

        let pattern = tiny_skia::Pattern::new(
            raster.as_ref().as_ref(),
            tiny_skia::SpreadMode::Pad,
            quality,
            1.0,
            tiny_skia::Transform::from_scale(
                rect.width() / raster.width() as f32,
                rect.height() / raster.height() as f32,
            ),
        );
        let mut paint = tiny_skia::Paint::default();
        paint.shader = pattern;

        pixmap.fill_rect(rect, &paint, transform, None);

        Some(())
    }

    /// Area coverage on premultiplied RGBA, including fractional odd-size edges.
    fn area_reduce(
        source: &tiny_skia::Pixmap,
        width: u32,
        height: u32,
    ) -> Option<tiny_skia::Pixmap> {
        let mut result = tiny_skia::Pixmap::new(width, height)?;
        let sx = source.width() as f64 / width as f64;
        let sy = source.height() as f64 / height as f64;
        for y in 0..height {
            for x in 0..width {
                let (left, top) = (x as f64 * sx, y as f64 * sy);
                let (right, bottom) = ((x + 1) as f64 * sx, (y + 1) as f64 * sy);
                let mut channels = [0.0; 4];
                for iy in top.floor() as u32..(bottom.ceil() as u32).min(source.height()) {
                    let wy = bottom.min((iy + 1) as f64) - top.max(iy as f64);
                    for ix in left.floor() as u32..(right.ceil() as u32).min(source.width()) {
                        let weight = wy * (right.min((ix + 1) as f64) - left.max(ix as f64));
                        let offset = (iy as usize * source.width() as usize + ix as usize) * 4;
                        for channel in 0..4 {
                            channels[channel] += source.data()[offset + channel] as f64 * weight;
                        }
                    }
                }
                let offset = (y as usize * width as usize + x as usize) * 4;
                for channel in 0..4 {
                    result.data_mut()[offset + channel] =
                        (channels[channel] / (sx * sy)).round() as u8;
                }
            }
        }
        Some(result)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reused_decodes_and_anisotropic_reductions_match_uncached_pixels() {
            use std::sync::{Arc, Mutex};
            let mut source = tiny_skia::Pixmap::new(65, 49).unwrap();
            for (index, pixel) in source.pixels_mut().iter_mut().enumerate() {
                *pixel = tiny_skia::Color::from_rgba8(
                    (index * 31) as u8,
                    (index * 13) as u8,
                    (index * 7) as u8,
                    (index * 19) as u8,
                )
                .to_color_u8()
                .premultiply();
            }
            let kind = usvg::ImageKind::PNG(Arc::new(source.encode_png().unwrap()));
            let cache = Arc::new(Mutex::new(crate::RasterImageCache::new(1024 * 1024)));
            for (sx, sy) in [
                (0.07, 0.07),
                (0.03, 0.3),
                (0.3, 0.03),
                (0.07, 0.07),
                (0.3, 0.3),
            ] {
                for quality in [
                    ImageRendering::HighQuality,
                    ImageRendering::Smooth,
                    ImageRendering::Pixelated,
                ] {
                    let transform = tiny_skia::Transform::from_scale(sx, sy);
                    let mut expected = tiny_skia::Pixmap::new(24, 24).unwrap();
                    {
                        let _scope = crate::install_raster_image_cache(None);
                        render_raster(&kind, transform, quality, &mut expected.as_mut()).unwrap();
                    }
                    let mut actual = tiny_skia::Pixmap::new(24, 24).unwrap();
                    {
                        let _scope = crate::install_raster_image_cache(Some(cache.clone()));
                        render_raster(&kind, transform, quality, &mut actual.as_mut()).unwrap();
                        assert!(crate::raster_cache::get(&kind, &[]).is_some());
                    }
                    assert_eq!(actual.data(), expected.data(), "{sx},{sy}: {quality:?}");
                }
            }
        }

        #[test]
        fn minified_checkerboard_preserves_coverage_and_pixel_art_stays_discrete() {
            let mut source = tiny_skia::Pixmap::new(32, 32).unwrap();
            for y in 0..32 {
                for x in 0..32 {
                    let color = if (x + y) % 2 == 0 {
                        tiny_skia::Color::WHITE
                    } else {
                        tiny_skia::Color::BLACK
                    };
                    source.pixels_mut()[y * 32 + x] = color.to_color_u8().premultiply();
                }
            }
            let kind = usvg::ImageKind::PNG(std::sync::Arc::new(source.encode_png().unwrap()));
            let mut quality = tiny_skia::Pixmap::new(4, 4).unwrap();
            render_raster(
                &kind,
                tiny_skia::Transform::from_scale(0.125, 0.125),
                ImageRendering::OptimizeQuality,
                &mut quality.as_mut(),
            )
            .unwrap();
            assert!(quality.pixels().iter().all(|p| p.alpha() == 255
                && (127..=128).contains(&p.red())
                && p.red() == p.green()
                && p.red() == p.blue()));
            let mut pixelated = tiny_skia::Pixmap::new(4, 4).unwrap();
            render_raster(
                &kind,
                tiny_skia::Transform::from_scale(0.125, 0.125),
                ImageRendering::Pixelated,
                &mut pixelated.as_mut(),
            )
            .unwrap();
            assert!(pixelated
                .pixels()
                .iter()
                .all(|p| matches!(p.red(), 0 | 255)));
        }

        #[test]
        fn odd_transparent_source_keeps_premultiplied_color_and_complete_extent() {
            let mut source = tiny_skia::Pixmap::new(3, 1).unwrap();
            source.pixels_mut()[0] = tiny_skia::Color::from_rgba8(255, 0, 0, 255)
                .to_color_u8()
                .premultiply();
            source.pixels_mut()[2] = source.pixels()[0];
            let reduced = area_reduce(&source, 2, 1).unwrap();
            assert!(reduced
                .pixels()
                .iter()
                .all(|p| p.red() == 170 && p.alpha() == 170 && p.green() == 0 && p.blue() == 0));
        }
    }
}
