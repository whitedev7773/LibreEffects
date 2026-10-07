//! GPUI-free authored-spacing qualification. Compile as a binary, not --test.
//! Synthetic bundled-font fixtures exercise production paint/export and carets.
#![allow(dead_code)]

#[path = "../../src/adjustment_render.rs"]
mod adjustment_render;
#[path = "../../src/audio_spectrum_render.rs"]
mod audio_spectrum_render;
#[path = "../../src/blend_render.rs"]
mod blend_render;
#[path = "../../src/effect_render.rs"]
mod effect_render;
#[path = "../../src/fonts.rs"]
mod fonts;
#[path = "../../src/image_sequence.rs"]
mod image_sequence;
#[path = "../../src/matte_render.rs"]
mod matte_render;
#[path = "../../src/path_mask_render.rs"]
mod path_mask_render;
#[path = "../../src/rendering.rs"]
mod rendering;
#[path = "../../src/rich_text_render.rs"]
mod rich_text_render;
#[path = "../../src/source_render.rs"]
mod source_render;
#[path = "../../src/text_animator.rs"]
mod text_animator;
#[path = "../../src/text_animator_render.rs"]
mod text_animator_render;
#[path = "../../src/text_edit.rs"]
mod text_edit;
#[path = "../../src/text_flow.rs"]
mod text_flow;

// Only external media decoding, media-import path rewriting, and expression
// workers are excluded. Fail immediately if a fixture unexpectedly invokes one.
mod video_decoder {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Default)]
    pub(crate) struct Pool;
    impl Pool {
        pub fn clear(&mut self) {}
        pub fn frame_png(
            &mut self,
            _: &str,
            _: f64,
            _: f64,
            _: u32,
            _: u32,
            _: u32,
            _: &AtomicBool,
        ) -> Result<Arc<str>, String> {
            Err("Authored-spacing probe excludes video decoding".into())
        }
    }
    pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            Err("Authored-spacing probe canceled".into())
        } else {
            Ok(())
        }
    }
}
mod media_io {
    pub(crate) fn path_string(_: &std::path::Path) -> Result<String, String> {
        Err("Authored-spacing probe excludes media import and relocation".into())
    }
}
mod automation_process {
    use libre_effects_ae_expressions as ae;
    pub(crate) fn evaluate_expressions(
        _: &ae::CompositionSnapshot,
        _: &[ae::PropertyAddress],
        _: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<ae::EvaluatedProperties, ae::EvaluationError> {
        panic!("Authored-spacing probe must not invoke an expression worker")
    }
}
mod audio_analysis {
    #[derive(Default)]
    pub(crate) struct FrameAnalysis;
    #[derive(Default)]
    pub(crate) struct AudioAnalysis;
    impl AudioAnalysis {
        pub(crate) fn analyze(
            &mut self,
            _: &libre_effects_core::Project,
            _: libre_effects_core::CompositionId,
            _: libre_effects_core::CompositionSample,
            _: &libre_effects_core::AudioSpectrumSettings,
            _: &mut FrameAnalysis,
            _: &std::sync::atomic::AtomicBool,
        ) -> Result<Option<std::sync::Arc<libre_effects_audio_spectrum::SpectrumFrame>>, String>
        {
            Err("Authored-spacing probe excludes audio analysis".into())
        }
    }
}
// The production text module also exposes a UI selection-target check. Its
// unused state type needs no GPUI shell for this standalone renderer probe.
mod editor {
    pub(crate) struct EditorState {
        pub text_session: Option<crate::text_edit::Session>,
        pub editor: libre_effects_core::Editor,
        pub document_revision: u64,
        pub frame: libre_effects_core::Frame,
    }
}

mod checks;
fn main() {
    let output = std::env::args().nth(1).map(std::path::PathBuf::from);
    checks::literal_pixels_and_carets();
    checks::legacy_and_rejections();
    checks::paint_overhang_keeps_independent_caret();
    checks::production_roundtrip(output.as_deref());
    println!("PASS authored-spacing80 synthetic qualification");
}
