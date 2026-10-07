//! UI-independent editor view models, shared by the desktop and lightweight tests.
//! Keep this crate free of GPUI, rendering, media and native-window dependencies.
pub mod expression_edit;
pub mod project_browser;
pub mod project_usage;
pub mod recent_projects;
pub mod timeline_filter;
pub mod timeline_navigation;
pub mod timeline_rename;

pub mod automation;
pub mod automation_host;

pub mod automation_ui;

#[cfg(test)]
mod layer_timing_tests;

#[cfg(test)]
mod text_paragraph_tests;

#[cfg(test)]
mod expression_model_tests;
#[cfg(test)]
mod expression_pooling_tests;

pub mod preview_scene;

#[cfg(test)]
mod media_sharing_tests;

pub mod input_routing;

#[cfg(test)]
mod rich_text_tests;

#[cfg(test)]
mod selected_text_tests;

#[cfg(test)]
mod point_text_layout_tests;

#[cfg(test)]
mod authored_spacing_tests;

pub mod ae_import;
pub mod text_buffer;

#[cfg(test)]
mod ae_import_tests;

pub mod ae_import_ui;

#[cfg(test)]
mod spatial_host_tests;
#[cfg(test)]
mod spatial_model_tests;

#[cfg(test)]
mod opacity_model_tests;

#[cfg(test)]
mod opacity_host_tests;

#[cfg(test)]
mod planar_model_tests;

#[cfg(test)]
mod planar_host_tests;

#[cfg(test)]
mod playbar_expression_tests;

#[cfg(test)]
mod continuous_sampling_tests;

#[cfg(test)]
mod gaussian_edge_tests;
