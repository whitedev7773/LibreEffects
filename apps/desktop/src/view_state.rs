//! Optional desktop metadata. It never participates in rendering or document history.
use libre_effects_core::{CompositionId, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CompositionView {
    pub frame: u32,
    pub timeline_start: u32,
    pub timeline_zoom: f32,
    pub preview_zoom: Option<f32>,
    pub preview_pan: [f32; 2],
    pub preview_resolution: u32,
    pub checkerboard: bool,
    pub viewer: crate::viewer_tools::ViewerOptions,
    pub graph_open: bool,
    pub expanded: bool,
}
impl Default for CompositionView {
    fn default() -> Self {
        Self {
            frame: 0,
            timeline_start: 0,
            timeline_zoom: 1.0,
            preview_zoom: None,
            preview_pan: [0.0; 2],
            preview_resolution: 1,
            checkerboard: false,
            viewer: Default::default(),
            graph_open: false,
            expanded: true,
        }
    }
}
fn finite(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}
impl CompositionView {
    pub fn normalize(&mut self, duration: u32) {
        self.frame = self.frame.min(duration - 1);
        self.viewer.normalize();
        self.timeline_zoom = finite(self.timeline_zoom, 1.0, 1.0, 64.0);
        let visible = ((duration as f32 / self.timeline_zoom).ceil() as u32).max(2);
        self.timeline_start = self.timeline_start.min(duration.saturating_sub(visible));
        self.preview_zoom = self.preview_zoom.map(|v| finite(v, 0.5, 0.0625, 8.0));
        self.preview_pan = self.preview_pan.map(|v| finite(v, 0.0, -32768.0, 32768.0));
        if !matches!(self.preview_resolution, 1 | 2 | 4) {
            self.preview_resolution = 1;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorkspaceView {
    // Outer width, project/viewer width, viewer/timeline height, right panel height.
    pub fractions: [f32; 4],
    pub timeline_left: f32,
    pub sidebar_expanded: [bool; 4],
    pub effect_controls_open: bool,
    pub snapping: bool,
    pub align_to_selection: bool,
}
impl Default for WorkspaceView {
    fn default() -> Self {
        Self {
            fractions: [0.84, 0.20, 0.615, 0.615],
            timeline_left: 560.0,
            sidebar_expanded: [true, false, false, false],
            effect_controls_open: false,
            snapping: true,
            align_to_selection: false,
        }
    }
}
impl WorkspaceView {
    fn normalize(&mut self) {
        for (i, min) in [0.12, 0.12, 0.22, 0.20].into_iter().enumerate() {
            self.fractions[i] = finite(
                self.fractions[i],
                Self::default().fractions[i],
                min,
                1.0 - min,
            );
        }
        self.timeline_left = finite(self.timeline_left, 560.0, 380.0, 800.0);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProjectViews {
    version: u32,
    pub compositions: BTreeMap<CompositionId, CompositionView>,
    pub workspace: WorkspaceView,
}
impl Default for ProjectViews {
    fn default() -> Self {
        Self {
            version: 1,
            compositions: BTreeMap::new(),
            workspace: WorkspaceView::default(),
        }
    }
}
impl ProjectViews {
    pub fn normalize(&mut self, project: &Project) {
        self.compositions.retain(|id, view| {
            if let Some(comp) = project.composition_by_id(*id) {
                view.normalize(comp.duration());
                true
            } else {
                false
            }
        });
        self.workspace.normalize();
    }
    pub fn read(json: &str, project: &Project) -> Self {
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(default)]
            editor_view: Option<ProjectViews>,
        }
        // Optional UI metadata must never stop an otherwise valid document opening.
        let mut views = serde_json::from_str::<Envelope>(json)
            .ok()
            .and_then(|v| v.editor_view)
            .filter(|v| v.version == 1)
            .unwrap_or_default();
        views.normalize(project);
        views
    }
    pub fn write(&self, project: &Project) -> Result<String, String> {
        let mut views = self.clone();
        views.normalize(project);
        let metadata = serde_json::to_string_pretty(&views).map_err(|e| e.to_string())?;
        // Avoid reparsing/cloning embedded image data just to attach desktop metadata.
        let mut json = project.to_json()?;
        let end = json.rfind('}').ok_or("Invalid project JSON")?;
        json.truncate(end);
        json.push_str(",\n  \"editor_view\": ");
        json.push_str(&metadata);
        json.push_str("\n}");
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_views_roundtrip_without_changing_the_render_document() {
        let p = Project::default();
        let mut views = ProjectViews::default();
        views.compositions.insert(
            1,
            CompositionView {
                frame: 90,
                timeline_zoom: 4.0,
                timeline_start: 70,
                preview_pan: [32.0, -18.0],
                preview_zoom: Some(0.75),
                preview_resolution: 2,
                checkerboard: true,
                viewer: crate::viewer_tools::ViewerOptions {
                    rulers: true,
                    grid: true,
                    channel: crate::viewer_tools::Channel::Alpha,
                    ..Default::default()
                },
                graph_open: true,
                expanded: false,
            },
        );
        views.workspace.fractions = [0.8, 0.25, 0.7, 0.5];
        views.workspace.align_to_selection = true;
        let json = views.write(&p).unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), p);
        assert_eq!(ProjectViews::read(&json, &p), views);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("views.lfe.json");
        crate::project_io::write_project(&path, &json).unwrap();
        let (loaded, loaded_views) = crate::project_io::read_editor_project(&path).unwrap();
        assert_eq!(loaded, p);
        assert_eq!(loaded_views, views);
    }
    #[test]
    fn invalid_or_future_views_do_not_break_old_documents_or_layout() {
        let p = Project::default();
        assert_eq!(
            ProjectViews::read(&p.to_json().unwrap(), &p),
            ProjectViews::default()
        );
        for value in [
            serde_json::json!({"version":999}),
            serde_json::json!({"compositions": []}),
        ] {
            let mut json: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
            json["editor_view"] = value;
            assert_eq!(
                ProjectViews::read(&json.to_string(), &p),
                ProjectViews::default()
            );
        }
        let mut views = ProjectViews::default();
        views.compositions.insert(
            1,
            CompositionView {
                frame: u32::MAX,
                timeline_start: u32::MAX,
                timeline_zoom: -1.0,
                preview_zoom: Some(f32::INFINITY),
                preview_pan: [f32::NAN, 999999.0],
                preview_resolution: 0,
                ..Default::default()
            },
        );
        views.compositions.insert(99, CompositionView::default());
        views.workspace.fractions = [f32::NAN, -5.0, 99.0, 0.0];
        views.normalize(&p);
        assert_eq!(views.compositions.len(), 1);
        let v = &views.compositions[&1];
        assert_eq!((v.frame, v.timeline_start, v.timeline_zoom), (149, 0, 1.0));
        assert_eq!(
            (v.preview_zoom, v.preview_pan, v.preview_resolution),
            (Some(0.5), [0.0, 32768.0], 1)
        );
        assert_eq!(views.workspace.fractions, [0.84, 0.12, 0.78, 0.2]);
    }
}
