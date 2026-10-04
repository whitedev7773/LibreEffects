//! Optional desktop metadata. It never participates in rendering or document history.
use libre_effects_core::{CompositionId, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[path = "graph_channels.rs"]
mod channels;
pub(crate) use channels::{GraphChannel, GraphChannels, GraphRanges, MAX_PINNED_CHANNELS};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GraphView {
    pub speed: bool,
    /// None continuously fits the visible graph height; Some freezes the value range.
    pub height: Option<[f64; 2]>,
}
impl GraphView {
    pub fn normalize(&mut self) {
        if self.height.is_some_and(|[low, high]| {
            !low.is_finite()
                || !high.is_finite()
                || low.abs() > 1e15
                || high.abs() > 1e15
                || high - low < 1e-6
        }) {
            self.height = None;
        }
    }
}

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
    pub graph_view: GraphView,
    pub expanded: bool,
    #[serde(default, skip_serializing_if = "GraphChannels::is_legacy")]
    pub graph_channels: GraphChannels,
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
            graph_view: Default::default(),
            expanded: true,
            graph_channels: Default::default(),
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
        self.graph_view.normalize();
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
    pub extra_sidebar_expanded: [bool; 3],
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
            extra_sidebar_expanded: [false; 3],
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
                view.graph_channels.prune(comp);
                true
            } else {
                false
            }
        });
        self.workspace.normalize();
        self.version = if self
            .compositions
            .values()
            .any(|v| !v.graph_channels.is_legacy())
        {
            2
        } else {
            1
        };
    }
    pub fn read(json: &str, project: &Project) -> Self {
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(default)]
            editor_view: Option<serde_json::Value>,
        }
        // Optional legacy metadata must never prevent a valid document opening.
        // Both explicitly versioned schemas use the same validation as native VIEW.
        serde_json::from_str::<Envelope>(json)
            .ok()
            .and_then(|v| v.editor_view)
            .and_then(|value| serde_json::to_vec(&value).ok())
            .and_then(|bytes| Self::read_native(&bytes, project).ok())
            .unwrap_or_default()
    }
    /// The native VIEW chunk is versioned data, so unsupported or malformed
    /// metadata must be reported instead of silently replaced with defaults.
    /// Container decoding has already checked UTF-8, size and duplicate keys.
    pub fn read_native(bytes: &[u8], project: &Project) -> Result<Self, String> {
        fn object<'a>(
            value: &'a serde_json::Value,
            fields: &[&str],
            context: &str,
        ) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
            let object = value
                .as_object()
                .ok_or_else(|| format!("Invalid native {context}: expected an object"))?;
            if let Some(field) = object.keys().find(|key| !fields.contains(&key.as_str())) {
                return Err(format!("Unsupported native {context} field: {field}"));
            }
            Ok(object)
        }
        let value = strict_view_json(bytes)?;
        let root = object(
            &value,
            &["version", "compositions", "workspace"],
            "editor view",
        )?;
        let version = root.get("version").and_then(|version| version.as_u64());
        if !matches!(version, Some(1 | 2)) {
            return Err("Unsupported native editor view version; expected version 1 or 2".into());
        }
        let compositions = root
            .get("compositions")
            .and_then(|value| value.as_object())
            .ok_or("Invalid native editor view: compositions must be an object")?;
        if compositions.len() > 1000 {
            return Err("Native composition view count exceeds limit".into());
        }
        for (id, view) in compositions {
            if id
                .parse::<CompositionId>()
                .ok()
                .is_none_or(|value| value == 0 || value.to_string() != *id)
            {
                return Err("Invalid native composition view ID".into());
            }
            let view = object(
                view,
                &[
                    "frame",
                    "timeline_start",
                    "timeline_zoom",
                    "preview_zoom",
                    "preview_pan",
                    "preview_resolution",
                    "checkerboard",
                    "viewer",
                    "graph_open",
                    "graph_view",
                    "expanded",
                    "graph_channels",
                ],
                "composition view",
            )?;
            if version == Some(1) && view.contains_key("graph_channels") {
                return Err("Graph channels require native editor view version 2".into());
            }
            if let Some(channels) = view.get("graph_channels") {
                let channels = object(
                    channels,
                    &["version", "pinned", "active", "ranges"],
                    "Graph channels",
                )?;
                for field in ["version", "pinned", "active", "ranges"] {
                    if !channels.contains_key(field) {
                        return Err(format!("Invalid native Graph channels: missing {field}"));
                    }
                }
                let pinned = channels["pinned"]
                    .as_array()
                    .ok_or("Invalid native Graph pins: expected an array")?;
                let ranges = channels["ranges"]
                    .as_array()
                    .ok_or("Invalid native Graph ranges: expected an array")?;
                if pinned.len() > MAX_PINNED_CHANNELS || ranges.len() > MAX_PINNED_CHANNELS + 1 {
                    return Err("Graph channel count exceeds limit".into());
                }
            }
            if let Some(graph) = view.get("graph_view") {
                object(graph, &["speed", "height"], "graph view")?;
            }
            if let Some(viewer) = view.get("viewer") {
                object(
                    viewer,
                    &[
                        "rulers",
                        "grid",
                        "guides",
                        "safe",
                        "snap_guides",
                        "snap_grid",
                        "lock_guides",
                        "grid_size",
                        "channel",
                    ],
                    "viewer options",
                )?;
            }
        }
        let workspace = root
            .get("workspace")
            .ok_or("Invalid native editor view: missing workspace")?;
        object(
            workspace,
            &[
                "fractions",
                "timeline_left",
                "sidebar_expanded",
                "extra_sidebar_expanded",
                "effect_controls_open",
                "snapping",
                "align_to_selection",
            ],
            "workspace view",
        )?;
        let mut views: Self = serde_json::from_value(value)
            .map_err(|error| format!("Invalid native editor view: {error}"))?;
        views.normalize(project);
        Ok(views)
    }
    pub fn encode_native(&self, project: &Project) -> Result<Vec<u8>, String> {
        let views = self.for_encoding(project)?;
        let bytes = serde_json::to_vec(&views).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_VIEW_BYTES {
            return Err("Native editor view exceeds 16 MiB".into());
        }
        Ok(bytes)
    }
    fn for_encoding(&self, project: &Project) -> Result<Self, String> {
        if !matches!(self.version, 1 | 2) {
            return Err("Unsupported native editor view version; expected version 1 or 2".into());
        }
        let mut views = self.clone();
        views.normalize(project);
        views.version = if views
            .compositions
            .values()
            .any(|v| !v.graph_channels.is_legacy())
        {
            2
        } else {
            1
        };
        Ok(views)
    }
    pub fn write(&self, project: &Project) -> Result<String, String> {
        let views = self.for_encoding(project)?;
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

const MAX_VIEW_BYTES: usize = 16 * 1024 * 1024;

fn strict_view_json(bytes: &[u8]) -> Result<serde_json::Value, String> {
    if bytes.len() > MAX_VIEW_BYTES {
        return Err("Native editor view exceeds 16 MiB".into());
    }
    struct Unique(serde_json::Value);
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("JSON with unique object keys")
                }
                fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                    Ok(Unique(v.into()))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                    Ok(Unique(v.into()))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                    Ok(Unique(v.into()))
                }
                fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                    serde_json::Number::from_f64(v)
                        .map(|v| Unique(v.into()))
                        .ok_or_else(|| E::custom("Invalid number"))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                    Ok(Unique(v.into()))
                }
                fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                    Ok(Unique(v.into()))
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                    Ok(Unique(serde_json::Value::Null))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Unique, A::Error> {
                    let mut values = Vec::new();
                    while let Some(Unique(value)) = seq.next_element::<Unique>()? {
                        values.push(value);
                    }
                    Ok(Unique(values.into()))
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Unique, A::Error> {
                    let mut values = serde_json::Map::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if values.contains_key(&key) {
                            return Err(serde::de::Error::custom(
                                "Duplicate native editor view key",
                            ));
                        }
                        values.insert(key, map.next_value::<Unique>()?.0);
                    }
                    Ok(Unique(values.into()))
                }
            }
            deserializer.deserialize_any(Visitor)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Unique::deserialize(&mut deserializer)
        .map_err(|e| format!("Invalid native editor view: {e}"))?;
    deserializer
        .end()
        .map_err(|e| format!("Invalid native editor view: {e}"))?;
    Ok(value.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_views_reject_future_versions_shapes_and_unknown_fields() {
        let project = Project::default();
        let valid = ProjectViews::default().encode_native(&project).unwrap();
        let base: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        let mut invalid = Vec::new();
        for value in [
            serde_json::json!(3),
            serde_json::json!("1"),
            serde_json::Value::Null,
        ] {
            let mut view = base.clone();
            view["version"] = value;
            invalid.push(view);
        }
        for value in [serde_json::json!([]), serde_json::json!(true)] {
            let mut view = base.clone();
            view["compositions"] = value;
            invalid.push(view);
        }
        for value in [
            serde_json::json!({"1": {"preview_pan": [0]}}),
            serde_json::json!({"1": {"frame": -1}}),
            serde_json::json!({"1": {"future_field": true}}),
            serde_json::json!({"1": {"viewer": {"future_field": true}}}),
            serde_json::json!({"1": {"graph_view": {"future_field": true}}}),
            serde_json::json!({"01": {}}),
        ] {
            let mut view = base.clone();
            view["compositions"] = value;
            invalid.push(view);
        }
        let mut unknown_workspace = base.clone();
        unknown_workspace["workspace"]["future_field"] = serde_json::json!(true);
        invalid.push(unknown_workspace);
        invalid.push(serde_json::json!({"version": 1}));
        for value in invalid {
            let bytes = serde_json::to_vec(&value).unwrap();
            assert!(
                ProjectViews::read_native(&bytes, &project).is_err(),
                "{value}"
            );
            let container =
                libre_effects_core::project_file::encode(&project, Some(&bytes)).unwrap();
            assert!(
                crate::project_io::decode_project(&container).is_err(),
                "{value}"
            );
        }
        assert_eq!(
            ProjectViews::read_native(&valid, &project).unwrap(),
            ProjectViews::default()
        );
    }

    #[test]
    fn native_views_normalize_valid_out_of_range_values() {
        let project = Project::default();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "compositions": {
                "1": {"frame": 9999, "timeline_zoom": -2, "preview_resolution": 9,
                      "preview_pan": [999999, -999999], "graph_view": {"height": [2, 1]}},
                "999": {}
            },
            "workspace": {"timeline_left": 9999, "fractions": [0, 0, 1, 1]}
        }))
        .unwrap();
        let views = ProjectViews::read_native(&bytes, &project).unwrap();
        assert_eq!(views.compositions.len(), 1);
        let view = &views.compositions[&1];
        assert_eq!(
            (view.frame, view.timeline_zoom, view.preview_resolution),
            (149, 1.0, 1)
        );
        assert_eq!(view.preview_pan, [32768.0, -32768.0]);
        assert_eq!(view.graph_view.height, None);
        assert_eq!(views.workspace.timeline_left, 800.0);
        assert_eq!(views.workspace.fractions, [0.12, 0.12, 0.78, 0.8]);
    }

    #[test]
    fn invalid_graph_ranges_restore_auto_height_without_changing_graph_type() {
        for height in [
            [2.0, 1.0],
            [0.0, 0.0],
            [f64::NAN, 10.0],
            [0.0, f64::INFINITY],
            [-1e16, 1e16],
        ] {
            let mut graph = GraphView {
                speed: true,
                height: Some(height),
            };
            graph.normalize();
            assert!(graph.speed);
            assert!(graph.height.is_none());
        }
        let mut graph = GraphView {
            speed: false,
            height: Some([-40.0, 80.0]),
        };
        graph.normalize();
        assert_eq!(graph.height, Some([-40.0, 80.0]));
    }
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
                graph_view: GraphView {
                    speed: true,
                    height: Some([-200.0, 300.0]),
                },
                expanded: false,
                graph_channels: Default::default(),
            },
        );
        views.workspace.fractions = [0.8, 0.25, 0.7, 0.5];
        views.workspace.align_to_selection = true;
        views.workspace.extra_sidebar_expanded = [true, true, false];
        let json = views.write(&p).unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), p);
        assert_eq!(ProjectViews::read(&json, &p), views);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("views.lfe.json");
        crate::project_io::write_project(&path, &json).unwrap();
        let loaded = crate::project_io::read_editor_project(&path).unwrap();
        assert_eq!(loaded.project, p);
        assert_eq!(loaded.views, views);
        assert_eq!(loaded.format, crate::project_io::ProjectFormat::LegacyJson);
        let native = dir.path().join("views.lep");
        crate::project_io::write_native_project(&native, &p, Some(&views)).unwrap();
        let loaded = crate::project_io::read_editor_project(&native).unwrap();
        assert_eq!(loaded.project, p);
        assert_eq!(loaded.views, views);
        assert_eq!(loaded.format, crate::project_io::ProjectFormat::Lep);
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

    fn channel_fixture() -> (Project, ProjectViews) {
        use libre_effects_core::{Command, Editor, Property};
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let x = GraphChannel {
            id: 1,
            property: Property::PositionX.into(),
        };
        let y = GraphChannel {
            id: 1,
            property: Property::PositionY.into(),
        };
        let mut view = CompositionView::default();
        view.graph_channels.pin(x).unwrap();
        view.graph_channels.activate(y);
        view.graph_channels.ranges.insert(
            x,
            GraphRanges {
                value: Some([-10., 20.]),
                speed: Some([-1., 2.]),
            },
        );
        let mut views = ProjectViews::default();
        views.compositions.insert(1, view);
        (editor.project().clone(), views)
    }

    #[test]
    fn native_v1_emission_is_byte_identical_until_channels_are_needed() {
        let project = Project::default();
        let views = ProjectViews::default();
        assert_eq!(
            String::from_utf8(views.encode_native(&project).unwrap()).unwrap(),
            r#"{"version":1,"compositions":{},"workspace":{"fractions":[0.84,0.2,0.615,0.615],"timeline_left":560.0,"sidebar_expanded":[true,false,false,false],"extra_sidebar_expanded":[false,false,false],"effect_controls_open":false,"snapping":true,"align_to_selection":false}}"#
        );
        let mut views = views;
        views.compositions.insert(
            1,
            CompositionView {
                graph_view: GraphView {
                    speed: true,
                    height: Some([-40., 80.]),
                },
                ..Default::default()
            },
        );
        let bytes = views.encode_native(&project).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 1);
        assert!(value["compositions"]["1"].get("graph_channels").is_none());
        assert_eq!(
            ProjectViews::read_native(&bytes, &project)
                .unwrap()
                .encode_native(&project)
                .unwrap(),
            bytes
        );
        // A stale live pin cannot force a file to use v2 once the saved copy is pruned.
        views
            .compositions
            .get_mut(&1)
            .unwrap()
            .graph_channels
            .pin(GraphChannel {
                id: 99,
                property: libre_effects_core::Property::Opacity.into(),
            })
            .unwrap();
        assert_eq!(views.encode_native(&project).unwrap(), bytes);
    }

    #[test]
    fn native_v2_roundtrips_typed_ranges_and_preserves_source_document() {
        let (project, views) = channel_fixture();
        let source = project.to_json().unwrap();
        let bytes = views.encode_native(&project).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(value["compositions"]["1"]["graph_channels"]["version"], 1);
        assert_eq!(
            value["compositions"]["1"]["graph_channels"]["pinned"][0]["property"]["kind"],
            "transform"
        );
        let loaded = ProjectViews::read_native(&bytes, &project).unwrap();
        assert_eq!(loaded.encode_native(&project).unwrap(), bytes);
        let native = libre_effects_core::project_file::encode(&project, Some(&bytes)).unwrap();
        let opened = crate::project_io::decode_project(&native).unwrap();
        assert_eq!(opened.project.to_json().unwrap(), source);
        assert_eq!(opened.views.encode_native(&opened.project).unwrap(), bytes);
        let json = views.write(&project).unwrap();
        assert_eq!(Project::from_json(&json).unwrap(), project);
        assert_eq!(
            ProjectViews::read(&json, &project)
                .encode_native(&project)
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn native_v2_rejects_invalid_addresses_duplicates_limits_and_v1_smuggling() {
        let (project, views) = channel_fixture();
        let bytes = views.encode_native(&project).unwrap();
        let base: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut invalid = Vec::new();
        for version in [0, 1, 3, 999] {
            let mut value = base.clone();
            value["version"] = version.into();
            invalid.push(value);
        }
        for version in [0, 2, 999] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]["version"] = version.into();
            invalid.push(value);
        }
        for field in ["version", "pinned", "active", "ranges"] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            invalid.push(value);
        }
        let address = base["compositions"]["1"]["graph_channels"]["pinned"][0].clone();
        for pins in [
            serde_json::json!([address.clone(), address.clone()]),
            serde_json::json!(vec![address.clone(); 17]),
            serde_json::json!({}),
        ] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]["pinned"] = pins;
            invalid.push(value);
        }
        for path in [
            serde_json::json!({"kind":"transform","parameter":"FutureParameter"}),
            serde_json::json!({"kind":"path","target":"Shape"}),
            serde_json::json!({"kind":"time_remap","future":true}),
            serde_json::json!({"kind":"transform","parameter":"PositionX","unknown":true}),
            serde_json::json!({"kind":"effect","effect":0,"parameter":"Radius"}),
            serde_json::json!({"kind":"contents","item":0,"parameter":"Width"}),
            serde_json::json!({"kind":"mask","mask":0,"parameter":"Opacity"}),
            serde_json::json!({"kind":"shape","parameter":"DashLength255"}),
        ] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]["active"]["property"] = path;
            invalid.push(value);
        }
        for id in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!("1"),
        ] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]["active"]["id"] = id;
            invalid.push(value);
        }
        let range = base["compositions"]["1"]["graph_channels"]["ranges"][0].clone();
        for ranges in [
            serde_json::json!([range.clone(), range.clone()]),
            serde_json::json!(vec![range.clone(); 18]),
        ] {
            let mut value = base.clone();
            value["compositions"]["1"]["graph_channels"]["ranges"] = ranges;
            invalid.push(value);
        }
        let mut value = base.clone();
        value["compositions"]["1"]["graph_channels"]["ranges"][0]["channel"]["id"] = 99.into();
        invalid.push(value);
        let mut value = base.clone();
        value["compositions"]["1"]["graph_channels"]["ranges"][0]["value"] = serde_json::json!([0]);
        invalid.push(value);
        for value in invalid {
            let bytes = serde_json::to_vec(&value).unwrap();
            assert!(
                ProjectViews::read_native(&bytes, &project).is_err(),
                "{value}"
            );
            let native = libre_effects_core::project_file::encode(&project, Some(&bytes)).unwrap();
            assert!(
                crate::project_io::decode_project(&native).is_err(),
                "{value}"
            );
        }
        let text = String::from_utf8(bytes).unwrap();
        for duplicate in [
            text.replacen("\"version\":2", "\"version\":2,\"version\":2", 1),
            text.replacen("\"id\":1", "\"id\":1,\"id\":1", 1),
        ] {
            assert!(ProjectViews::read_native(duplicate.as_bytes(), &project).is_err());
        }
        assert!(ProjectViews::read_native(&vec![b' '; MAX_VIEW_BYTES + 1], &project).is_err());
        let mut oversized = base.clone();
        oversized["compositions"] = serde_json::Value::Object(
            (1..=1001)
                .map(|id| (id.to_string(), serde_json::json!({})))
                .collect(),
        );
        assert!(
            ProjectViews::read_native(&serde_json::to_vec(&oversized).unwrap(), &project).is_err()
        );
    }

    #[test]
    fn v2_open_prunes_unavailable_addresses_without_materializing_source_tracks() {
        let (project, views) = channel_fixture();
        let bytes = views.encode_native(&project).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let channels = &mut value["compositions"]["1"]["graph_channels"];
        channels["pinned"][0]["property"] = serde_json::json!({"kind":"time_remap"});
        channels["active"] = channels["pinned"][0].clone();
        channels["ranges"][0]["channel"] = channels["pinned"][0].clone();
        let source = project.to_json().unwrap();
        let loaded =
            ProjectViews::read_native(&serde_json::to_vec(&value).unwrap(), &project).unwrap();
        assert!(!loaded.compositions[&1].graph_channels.explicit);
        assert!(loaded.compositions[&1].graph_channels.pinned.is_empty());
        assert!(
            project
                .composition()
                .layer(1)
                .unwrap()
                .time_remap()
                .is_none()
        );
        assert_eq!(project.to_json().unwrap(), source);
        let encoded: serde_json::Value =
            serde_json::from_slice(&loaded.encode_native(&project).unwrap()).unwrap();
        assert_eq!(encoded["version"], 1);
    }

    fn typography_view_fixture(materialize: bool) -> (Project, Vec<(u64, u64)>) {
        use libre_effects_core::{Command, Content, Editor, TextParam, TrackEdit};
        let mut editor = Editor::default();
        let mut addresses = Vec::new();
        for index in 0..2 {
            if index != 0 {
                editor.execute(Command::NewComposition).unwrap();
            }
            editor
                .execute(Command::AddContent {
                    content: Content::Text {
                        text: "Sparse typography".into(),
                        font_size: 48.,
                    },
                    width: 300.,
                    height: 100.,
                    name: "Text".into(),
                })
                .unwrap();
            let id = editor.selected().unwrap();
            addresses.push((editor.project().active_composition_id(), id));
            if materialize {
                for parameter in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading] {
                    editor
                        .execute(Command::EditText {
                            id,
                            parameter,
                            edit: TrackEdit::ToggleAnimation { frame: 0 },
                        })
                        .unwrap();
                }
            }
        }
        (editor.project().clone(), addresses)
    }

    #[test]
    fn sparse_typography_addresses_are_pruned_in_active_and_inactive_save_copies() {
        use libre_effects_core::{Property, PropertyPath, TextParam};
        let (project, addresses) = typography_view_fixture(false);
        let source = project.to_json().unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&source).unwrap()["version"]
                .as_u64()
                .unwrap()
                <= 48
        );
        for mixed in [false, true] {
            let mut views = ProjectViews::default();
            for &(composition, id) in &addresses {
                let mut view = CompositionView::default();
                if mixed {
                    let transform = GraphChannel {
                        id,
                        property: Property::PositionX.into(),
                    };
                    view.graph_channels.pin(transform).unwrap();
                    view.graph_channels.activate(transform);
                }
                views.compositions.insert(composition, view);
            }
            let baseline = views.encode_native(&project).unwrap();
            for &(composition, id) in &addresses {
                let channels = &mut views
                    .compositions
                    .get_mut(&composition)
                    .unwrap()
                    .graph_channels;
                for parameter in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading] {
                    let channel = GraphChannel {
                        id,
                        property: PropertyPath::Text(parameter),
                    };
                    channels.pin(channel).unwrap();
                    channels.activate(channel);
                    channels.ranges.insert(
                        channel,
                        GraphRanges {
                            value: Some([-10., 20.]),
                            speed: Some([-1., 2.]),
                        },
                    );
                }
            }
            let live = views.clone();
            let bytes = views.encode_native(&project).unwrap();
            assert_eq!(bytes, baseline);
            assert_eq!(views, live, "saving must prune only the copy");
            for name in ["FontSize", "Tracking", "Leading"] {
                assert!(!std::str::from_utf8(&bytes).unwrap().contains(name));
            }
            let native = crate::project_io::encode_native_project(&project, Some(&views)).unwrap();
            let opened = crate::project_io::decode_project(&native).unwrap();
            assert_eq!(opened.project, project);
            assert_eq!(opened.views.encode_native(&project).unwrap(), baseline);
            assert_eq!(project.to_json().unwrap(), source);
        }
    }

    #[test]
    fn materialized_typography_lanes_use_project49_view2_and_existing_lep_container() {
        use libre_effects_core::{PropertyPath, TextParam};
        let (project, addresses) = typography_view_fixture(true);
        let source = project.to_json().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&source).unwrap()["version"],
            49
        );
        let mut views = ProjectViews::default();
        for &(composition, id) in &addresses {
            let mut view = CompositionView::default();
            view.frame = 30;
            view.graph_open = true;
            for parameter in [TextParam::FontSize, TextParam::Tracking, TextParam::Leading] {
                let channel = GraphChannel {
                    id,
                    property: PropertyPath::Text(parameter),
                };
                view.graph_channels.pin(channel).unwrap();
                view.graph_channels.activate(channel);
                view.graph_channels.ranges.insert(
                    channel,
                    GraphRanges {
                        value: Some([-10., 200.]),
                        speed: Some([-20., 20.]),
                    },
                );
            }
            views.compositions.insert(composition, view);
        }
        views.normalize(&project);
        let bytes = views.encode_native(&project).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        for (composition, _) in addresses {
            let channel = &value["compositions"][composition.to_string()]["graph_channels"];
            assert_eq!(channel["version"], 1);
            assert_eq!(channel["pinned"].as_array().unwrap().len(), 3);
        }
        let native = crate::project_io::encode_native_project(&project, Some(&views)).unwrap();
        assert_eq!(&native[8..10], &[1, 0]);
        let opened = crate::project_io::decode_project(&native).unwrap();
        assert_eq!(opened.project, project);
        assert_eq!(opened.views, views);
        assert_eq!(
            crate::project_io::encode_native_project(&opened.project, Some(&opened.views)).unwrap(),
            native
        );
        assert_eq!(project.to_json().unwrap(), source);
    }
}

#[cfg(test)]
mod trim_view_tests {
    use super::*;
    use libre_effects_core::{
        Command, Content, ContentsEdit, ContentsKind, ContentsParam, Editor, PropertyPath,
        TrackEdit, TrimParam,
    };

    fn fixture() -> (Editor, ProjectViews, Vec<(u64, u64, u64)>) {
        let mut editor = Editor::default();
        let mut views = ProjectViews::default();
        let mut addresses = Vec::new();
        for index in 0..2 {
            if index != 0 {
                editor.execute(Command::NewComposition).unwrap();
            }
            editor
                .execute(Command::AddContent {
                    content: Content::Shape(Default::default()),
                    width: 200.,
                    height: 120.,
                    name: "Trim controls fixture".into(),
                })
                .unwrap();
            let id = editor.selected().unwrap();
            editor
                .execute(Command::Contents {
                    id,
                    edit: ContentsEdit::Promote,
                })
                .unwrap();
            editor
                .execute(Command::Contents {
                    id,
                    edit: ContentsEdit::Add {
                        parent: 1,
                        kind: ContentsKind::TrimPaths,
                    },
                })
                .unwrap();
            let Content::ShapeContents(contents) = editor.selected_layer().unwrap().content()
            else {
                panic!("expected Contents");
            };
            let item = contents
                .rows()
                .into_iter()
                .find(|(_, _, node)| matches!(node.kind, ContentsKind::TrimPaths))
                .unwrap()
                .2
                .id;
            let composition = editor.project().active_composition_id();
            addresses.push((composition, id, item));
            let mut view = CompositionView {
                frame: 10,
                graph_open: true,
                ..Default::default()
            };
            view.graph_view.speed = true;
            for parameter in TrimParam::ALL {
                let channel = GraphChannel {
                    id,
                    property: PropertyPath::Contents {
                        item,
                        parameter: ContentsParam::Trim(parameter),
                    },
                };
                if index != 0 {
                    editor
                        .execute(Command::EditTrack {
                            id,
                            property: channel.property,
                            edit: TrackEdit::ToggleAnimation { frame: 0 },
                        })
                        .unwrap();
                }
                view.graph_channels.pin(channel).unwrap();
                view.graph_channels.activate(channel);
                view.graph_channels.ranges.insert(
                    channel,
                    GraphRanges {
                        value: Some([-360., 720.]),
                        speed: Some([-100., 100.]),
                    },
                );
            }
            if index == 0 {
                editor
                    .execute(Command::Contents {
                        id,
                        edit: ContentsEdit::Enabled {
                            item,
                            enabled: false,
                        },
                    })
                    .unwrap();
            }
            views.compositions.insert(composition, view);
        }
        views.normalize(editor.project());
        (editor, views, addresses)
    }

    #[test]
    fn trim_active_and_inactive_pins_roundtrip_schema50_view2_address1_and_lep1() {
        let (editor, views, addresses) = fixture();
        let project = editor.project();
        let source = project.to_json().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&source).unwrap()["version"],
            50
        );
        assert_ne!(addresses[0].0, project.active_composition_id());
        let bytes = views.encode_native(project).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 2);
        for (composition, _, _) in &addresses {
            let channels = &value["compositions"][composition.to_string()]["graph_channels"];
            assert_eq!(channels["version"], 1);
            assert_eq!(channels["pinned"].as_array().unwrap().len(), 3);
            for (index, name) in ["Trim.Start", "Trim.End", "Trim.Offset"]
                .into_iter()
                .enumerate()
            {
                assert_eq!(channels["pinned"][index]["property"]["parameter"], name);
                assert_eq!(channels["pinned"][index]["property"]["kind"], "contents");
            }
        }
        let native = crate::project_io::encode_native_project(project, Some(&views)).unwrap();
        assert_eq!(&native[8..10], &[1, 0]);
        let opened = crate::project_io::decode_project(&native).unwrap();
        assert_eq!(opened.project, *project);
        assert_eq!(opened.views, views);
        assert_eq!(
            crate::project_io::encode_native_project(&opened.project, Some(&opened.views)).unwrap(),
            native
        );
        assert_eq!(project.to_json().unwrap(), source);
        // A Trim source does not itself opt its view into explicit Graph lanes.
        let legacy = ProjectViews::default().encode_native(project).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&legacy).unwrap()["version"],
            1
        );
    }

    #[test]
    fn trim_save_prunes_unavailable_active_and_inactive_pins_only_in_copy() {
        let (mut editor, mut views, addresses) = fixture();
        let originally_active = editor.project().active_composition_id();
        for &(composition, id, item) in &addresses {
            editor.activate_composition(composition).unwrap();
            editor
                .execute(Command::Contents {
                    id,
                    edit: ContentsEdit::Remove(item),
                })
                .unwrap();
            views
                .compositions
                .get_mut(&composition)
                .unwrap()
                .graph_channels
                .reconcile(Some(editor.project().composition()), false);
        }
        editor.activate_composition(originally_active).unwrap();
        let live = views.clone();
        let source = editor.project().to_json().unwrap();
        let bytes = views.encode_native(editor.project()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 1);
        let loaded = ProjectViews::read_native(&bytes, editor.project()).unwrap();
        for &(composition, _, _) in &addresses {
            assert!(
                loaded.compositions[&composition]
                    .graph_channels
                    .pinned
                    .is_empty()
            );
            assert!(
                loaded.compositions[&composition]
                    .graph_channels
                    .ranges
                    .is_empty()
            );
            assert_eq!(
                views.compositions[&composition].graph_channels.pinned.len(),
                3
            );
        }
        assert_eq!(views, live);
        assert_eq!(editor.project().to_json().unwrap(), source);
        let native =
            crate::project_io::encode_native_project(editor.project(), Some(&views)).unwrap();
        let opened = crate::project_io::decode_project(&native).unwrap();
        assert_eq!(opened.project, *editor.project());
        assert_eq!(opened.views, loaded);
        assert_eq!(views, live);
    }
}
