mod audio_controls;
mod audio_waveform;
mod authored_spacing;
mod blend;
mod browser;
mod character;
mod character_range;
mod color_curve;
mod contents;
mod effects;
mod footage_interpretation;
mod graph;
mod image_retirement;
mod inspector;
mod key_easing;
mod key_glyph;
mod markers;
mod mask_values;
mod matte;
mod native_opacity;
mod parent_drag;
mod path_masks;
mod pen;
mod preview;
mod shape_controls;
mod shape_stroke;
mod shape_values;
mod sidebar;
mod text_animator;
mod timeline;
mod timeline_snap;
pub(crate) use sidebar::{Align, Sidebar};

pub(crate) use browser::Browser;
pub(crate) use inspector::Inspector;
pub(crate) use preview::Preview;
pub(crate) use timeline::Timeline;
#[cfg(test)]
pub(crate) use timeline::compound_colors::{
    Input as CompoundColorsInput, PointerGeometry as CompoundPointerGeometry,
};

pub(crate) mod render_queue;

mod path_controls;

pub(crate) mod color_picker;
pub(crate) mod font_manager;

pub(crate) mod gradient_editor;

pub(crate) mod expression_editor;
pub(crate) mod vertex_editor;

pub(crate) mod ae_import;
pub(crate) mod script_ui;
