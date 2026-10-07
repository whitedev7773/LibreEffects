//! Reuse composition pixels while refreshing only the displayed expression view.
//! The caller must obtain cached pixels from the current RAM cache context.
use libre_effects_core::Project;
use std::sync::Arc;

pub(crate) struct Frame {
    pub pixels: Arc<image::RgbaImage>,
    pub evaluated: Option<Arc<Project>>,
}

pub(crate) fn render(
    renderer: &crate::rendering::Renderer,
    project: &Project,
    frame: u32,
    dimension: u32,
    cached: Option<Arc<image::RgbaImage>>,
) -> Result<Frame, String> {
    if let Some(pixels) = cached {
        let evaluated = renderer.preview_view(project, frame)?;
        Ok(Frame { pixels, evaluated })
    } else {
        let rendered = renderer.render_preview_with_view(project, frame, dimension)?;
        Ok(Frame {
            pixels: Arc::new(rendered.pixels),
            evaluated: rendered.evaluated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_hit_skips_raster_work_without_copying_pixels_and_still_rejects_cancellation() {
        let editor = crate::rendering::trim_tests::partial_scene();
        let renderer = crate::rendering::Renderer::new();
        let fresh = render(&renderer, editor.project(), 0, 200, None).unwrap();
        let bounded = crate::rendering::with_test_contents_budget(
            libre_effects_core::ContentsRenderBudget {
                frame_work_limit: 0,
                ..Default::default()
            },
            crate::rendering::Renderer::new,
        );
        assert!(render(&bounded, editor.project(), 0, 200, None).is_err());
        let hit = render(
            &bounded,
            editor.project(),
            0,
            200,
            Some(fresh.pixels.clone()),
        )
        .unwrap();
        assert!(Arc::ptr_eq(&hit.pixels, &fresh.pixels));
        assert!(hit.evaluated.is_none());
        assert!(
            render(
                &bounded,
                editor.project(),
                editor.project().composition().duration(),
                200,
                Some(fresh.pixels.clone())
            )
            .is_err()
        );
        let canceled = crate::rendering::Renderer::with_cancel(Arc::new(
            std::sync::atomic::AtomicBool::new(true),
        ));
        let error = render(&canceled, editor.project(), 0, 200, Some(fresh.pixels))
            .err()
            .unwrap();
        assert!(error.to_lowercase().contains("cancel"), "{error}");
    }
}
