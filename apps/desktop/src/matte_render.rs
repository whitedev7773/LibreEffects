//! Matte coverage multiplies all premultiplied channels after source effects.
use crate::rendering::Renderer;
use libre_effects_core::{CompositionId, Layer, MatteMode, Project};
use resvg::tiny_skia::Pixmap;

pub(crate) fn apply_matte(source: &mut Pixmap, matte: &Pixmap, mode: MatteMode) {
    debug_assert_eq!(
        (source.width(), source.height()),
        (matte.width(), matte.height())
    );
    for (s, m) in source
        .data_mut()
        .chunks_exact_mut(4)
        .zip(matte.data().chunks_exact(4))
    {
        let coverage = match mode {
            MatteMode::Alpha | MatteMode::AlphaInverted => u32::from(m[3]),
            // Premultiplied sRGB channels already include source alpha. Transparent
            // white therefore contributes zero, not a fully opaque luminance matte.
            MatteMode::Luma | MatteMode::LumaInverted => {
                (2126 * u32::from(m[0]) + 7152 * u32::from(m[1]) + 722 * u32::from(m[2]) + 5000)
                    / 10000
            }
        };
        let weight = if matches!(mode, MatteMode::AlphaInverted | MatteMode::LumaInverted) {
            255 - coverage
        } else {
            coverage
        };
        for channel in s {
            *channel = ((u32::from(*channel) * weight + 127) / 255) as u8;
        }
    }
}
impl Renderer {
    pub(crate) fn matte_pixels(
        &self,
        project: &Project,
        composition: CompositionId,
        layer: &Layer,
        frame: u32,
        max_dimension: u32,
        prefix: &str,
        layer_count: &mut usize,
    ) -> Result<Option<(Pixmap, MatteMode)>, String> {
        let Some(matte) = layer.track_matte() else {
            return Ok(None);
        };
        let comp = project
            .composition_by_id(composition)
            .ok_or("Missing source composition")?;
        let source = self.isolated_layer_svg(
            project,
            composition,
            matte.source,
            frame,
            max_dimension,
            &format!("{prefix}-matte"),
            layer_count,
        )?;
        Ok(Some((
            self.raster_canvas(&source, comp.width(), comp.height(), max_dimension)?,
            matte.mode,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        BlendMode, Command, Content, Editor, EffectEdit, EffectKind, Effects, LayerSwitch, Mask,
        Property, TrackMatte,
    };
    fn value(e: &mut Editor, id: u64, property: Property, value: f64) {
        e.execute(Command::SetValue {
            id,
            property,
            frame: 0,
            value,
        })
        .unwrap();
    }
    fn matte(e: &mut Editor, id: u64, source: u64, mode: MatteMode) {
        e.execute(Command::SetTrackMatte {
            id,
            matte: Some(TrackMatte { source, mode }),
        })
        .unwrap();
    }
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::ConfigureComposition {
            name: "Matte QA".into(),
            width: 100,
            height: 100,
            fps: 30,
            duration: 30,
        })
        .unwrap();
        for (id, size) in [(1, 80.0), (2, 40.0)] {
            e.execute(Command::AddContent {
                content: Content::Solid,
                width: size,
                height: size,
                name: format!("Layer {id}"),
            })
            .unwrap();
            e.execute(Command::SetColor {
                id,
                color: 0xff0000,
            })
            .unwrap();
            value(&mut e, id, Property::Opacity, 50.0);
        }
        e
    }
    #[test]
    fn matte_reference_pixels_include_alpha_luma_inversion_masks_effects_and_timing() {
        let r = Renderer::new();
        for (mode, inside, outside) in [
            (MatteMode::Alpha, 64, 0),
            (MatteMode::AlphaInverted, 64, 128),
            (MatteMode::Luma, 14, 0),
            (MatteMode::LumaInverted, 114, 128),
        ] {
            let mut e = scene();
            matte(&mut e, 1, 2, mode);
            let p = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let image = r.render(&p, 0, 100).unwrap();
            assert_eq!(image.get_pixel(50, 50).0, [255, 0, 0, inside], "{mode:?}");
            assert_eq!(image.get_pixel(20, 50)[3], outside, "{mode:?}");
            assert_eq!(image.get_pixel(0, 0)[3], 0);
            assert_eq!(r.render_preview(&p, 0, 100).unwrap(), image);
            assert_eq!(
                r.render(&p, 0, 50).unwrap().get_pixel(25, 25).0,
                [255, 0, 0, inside]
            );
            e.execute(Command::SetLayerRange {
                id: 2,
                start: 5,
                end: 10,
            })
            .unwrap();
            assert_eq!(
                r.render(e.project(), 4, 100).unwrap().get_pixel(50, 50)[3],
                outside
            );
            assert_eq!(
                r.render(e.project(), 9, 100).unwrap().get_pixel(50, 50)[3],
                inside
            );
            assert_eq!(
                r.render(e.project(), 10, 100).unwrap().get_pixel(50, 50)[3],
                outside
            );
        }
        let mut e = scene();
        matte(&mut e, 1, 2, MatteMode::Alpha);
        e.execute(Command::SetMask {
            id: 2,
            mask: Some(Mask {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 40.0,
                inverted: false,
            }),
        })
        .unwrap();
        let image = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(40, 50)[3], 64);
        assert_eq!(image.get_pixel(60, 50)[3], 0);
        e.execute(Command::SetMask { id: 2, mask: None }).unwrap();
        matte(&mut e, 1, 2, MatteMode::Luma);
        e.execute(Command::SetColor {
            id: 2,
            color: 0x00ff00,
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50)[3],
            46
        );
        e.execute(Command::Effect {
            id: 2,
            edit: EffectEdit::Add(EffectKind::Fill),
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50)[3],
            64
        );
        matte(&mut e, 1, 2, MatteMode::Alpha);

        e.execute(Command::SetEffects {
            id: 2,
            effects: Effects {
                blur: 3.0,
                ..Default::default()
            },
        })
        .unwrap();
        let image = r.render(e.project(), 0, 100).unwrap();
        assert!(image.get_pixel(29, 50)[3] > 0 && image.get_pixel(29, 50)[3] < 64);
        e.execute(Command::SetEffects {
            id: 2,
            effects: Default::default(),
        })
        .unwrap();
        value(&mut e, 2, Property::Opacity, 0.0);
        e.execute(Command::ToggleKeyframe {
            id: 2,
            property: Property::Opacity,
            frame: 0,
        })
        .unwrap();
        e.execute(Command::ToggleKeyframe {
            id: 2,
            property: Property::Opacity,
            frame: 10,
        })
        .unwrap();
        e.execute(Command::SetValue {
            id: 2,
            property: Property::Opacity,
            frame: 10,
            value: 100.0,
        })
        .unwrap();
        assert_eq!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(50, 50)[3],
            0
        );
        assert_eq!(
            r.render(e.project(), 5, 100).unwrap().get_pixel(50, 50)[3],
            64
        );
        assert_eq!(
            r.render(e.project(), 10, 100).unwrap().get_pixel(50, 50)[3],
            128
        );
    }
    #[test]
    fn matte_chains_reuse_parent_transforms_and_precompose_preserve_pixels() {
        let r = Renderer::new();
        let mut e = scene();
        e.execute(Command::AddSolid).unwrap();
        value(&mut e, 3, Property::Opacity, 50.0);
        matte(&mut e, 2, 3, MatteMode::Alpha);
        matte(&mut e, 1, 2, MatteMode::Alpha);
        let original = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(original.get_pixel(50, 50).0, [255, 0, 0, 32]);
        e.execute(Command::SetBlendMode {
            id: 2,
            mode: BlendMode::Multiply,
        })
        .unwrap();
        e.execute(Command::MoveLayer { id: 2, index: 2 }).unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 2,
            switch: LayerSwitch::Guide,
            enabled: true,
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Solo,
            enabled: true,
        })
        .unwrap();
        assert_eq!(r.render(e.project(), 0, 100).unwrap(), original);
        assert_eq!(r.render_preview(e.project(), 0, 100).unwrap(), original);
        e.execute(Command::SetParent {
            id: 2,
            parent: Some(1),
            frame: 0,
        })
        .unwrap();
        value(&mut e, 1, Property::PositionX, 60.0);
        let moved = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(moved.get_pixel(35, 50)[3], 0);
        assert_eq!(moved.get_pixel(65, 50)[3], 32);
        e.execute(Command::SetLayerSwitch {
            id: 2,
            switch: LayerSwitch::Guide,
            enabled: false,
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Solo,
            enabled: false,
        })
        .unwrap();
        e.execute(Command::Precompose {
            layers: vec![1, 2, 3],
            name: "Nested matte".into(),
        })
        .unwrap();
        assert_eq!(r.render(e.project(), 0, 100).unwrap(), moved);
        e.undo();
        // Two independent consumers share one matte without changing its visibility.
        e.execute(Command::DuplicateLayer(1)).unwrap();
        let second = e.selected().unwrap();
        assert_eq!(e.selected_layer().unwrap().track_matte().unwrap().source, 2);
        value(&mut e, second, Property::PositionX, 20.0);
        assert!(!e.project().composition().layer(2).unwrap().visible());
        assert!(
            r.render(e.project(), 0, 100).unwrap().get_pixel(45, 50)[3]
                > moved.get_pixel(45, 50)[3]
        );
    }
    #[test]
    fn adjustment_track_matte_limits_effect_coverage_without_double_alpha() {
        let r = Renderer::new();
        let mut e = scene();
        e.execute(Command::AddAdjustment).unwrap();
        e.execute(Command::Effect {
            id: 3,
            edit: EffectEdit::Add(EffectKind::Grayscale),
        })
        .unwrap();
        matte(&mut e, 3, 2, MatteMode::Alpha);
        let image = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(image.get_pixel(20, 50).0, [255, 0, 0, 128]);
        assert_eq!(image.get_pixel(50, 50)[3], 128);
        assert!(image.get_pixel(50, 50)[0] < 255 && image.get_pixel(50, 50)[1] > 0);
        matte(&mut e, 3, 2, MatteMode::AlphaInverted);
        let inverted = r.render(e.project(), 0, 100).unwrap();
        assert_eq!(inverted.get_pixel(20, 50)[3], 128);
        assert_eq!(inverted.get_pixel(20, 50)[0], inverted.get_pixel(20, 50)[1]);
    }
    #[test]
    fn preflight_includes_hidden_guide_matte_sources_only_during_consumer_range() {
        let mut e = scene();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("offline.mp4");
        e.execute(Command::AddContent {
            content: Content::Video {
                audio: None,
                path: path.to_string_lossy().into_owned(),
                duration: 1.0,
                source_fps: 30.0,
                start_frame: 0,
                playback: Default::default(),
            },
            width: 100.0,
            height: 100.0,
            name: "Matte footage".into(),
        })
        .unwrap();
        matte(&mut e, 1, 3, MatteMode::Alpha);
        e.execute(Command::SetLayerSwitch {
            id: 3,
            switch: LayerSwitch::Guide,
            enabled: true,
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Solo,
            enabled: true,
        })
        .unwrap();
        e.execute(Command::SetLayerRange {
            id: 1,
            start: 10,
            end: 20,
        })
        .unwrap();
        let output = dir.path().join("out.mp4");
        crate::project_io::validate_render(e.project(), &output, &(0..10)).unwrap();
        assert!(
            crate::project_io::validate_render(e.project(), &output, &(10..20))
                .unwrap_err()
                .contains("offline")
        );
        crate::project_io::validate_render(e.project(), &output, &(20..30)).unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Solo,
            enabled: false,
        })
        .unwrap();
        e.execute(Command::SetLayerSwitch {
            id: 3,
            switch: LayerSwitch::Guide,
            enabled: false,
        })
        .unwrap();
        e.execute(Command::Precompose {
            layers: vec![1, 2, 3],
            name: "Nested missing matte".into(),
        })
        .unwrap();
        assert!(
            crate::project_io::validate_render(e.project(), &output, &(10..20))
                .unwrap_err()
                .contains("offline")
        );
    }
    #[test]
    #[ignore = "requires FFmpeg; compares every track matte mode against PNG with alpha and matte export"]
    fn track_mattes_roundtrip_through_mp4_and_alpha_mov() {
        use crate::video_export::{VideoPreset, export_video, ffmpeg_path};
        let dir = tempfile::tempdir().unwrap();
        for mode in MatteMode::ALL {
            let mut e = scene();
            matte(&mut e, 1, 2, mode);
            e.execute(Command::SetCompositionBackground(0x183048))
                .unwrap();
            let project = Project::from_json(&e.project().to_json().unwrap()).unwrap();
            let preview = Renderer::new().render_preview(&project, 0, 100).unwrap();
            let png = dir.path().join("matte.png");
            preview.save(&png).unwrap();
            assert_eq!(image::open(&png).unwrap().to_rgba8(), preview);
            let mut flattened = preview.clone();
            crate::rendering::composite_background(&mut flattened, 0x183048);
            for preset in [VideoPreset::H264, VideoPreset::ProResAlpha] {
                let output =
                    dir.path()
                        .join(format!("matte-{}.{}", mode.label(), preset.extension()));
                export_video(
                    &project,
                    0..3,
                    preset,
                    &output,
                    Default::default(),
                    Default::default(),
                )
                .unwrap();
                let mut cmd = std::process::Command::new(ffmpeg_path());
                #[cfg(target_os = "windows")]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                let decoded = cmd
                    .args(["-v", "error", "-i"])
                    .arg(&output)
                    .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(decoded.status.success());
                assert_eq!(decoded.stdout.len(), 100 * 100 * 4 * 3);
                let reference = if preset == VideoPreset::H264 {
                    &flattened
                } else {
                    &preview
                };
                for (x, y) in [(0, 0), (20, 50), (50, 50), (80, 50)] {
                    let pixel = &decoded.stdout[(y * 100 + x) * 4..][..4];
                    let expected = reference.get_pixel(x as u32, y as u32).0;
                    assert!(
                        (i32::from(pixel[3]) - i32::from(expected[3])).abs() <= 2,
                        "{mode:?}: {pixel:?} != {expected:?}"
                    );
                    if expected[3] > 0 {
                        for c in 0..3 {
                            assert!(
                                (i32::from(pixel[c]) - i32::from(expected[c])).abs() <= 8,
                                "{mode:?}: {pixel:?} != {expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
