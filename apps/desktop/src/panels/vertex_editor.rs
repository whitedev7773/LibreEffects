//! One explicit Pen vertex, edited on an isolated, frozen source transaction.
use crate::editor::{EditorState, Tool};
use libre_effects_core::{
    Affine, Command, CompositionId, Content, Editor, Frame, LayerId, PathTarget, Project,
    VectorPath,
};
use std::{collections::BTreeSet, sync::Arc};

#[path = "vertex_editor_view.rs"]
mod view;
pub(crate) use view::VertexEditor;

/// A Pen-owned selection handoff, or a one-shot return to that exact selection.
/// The Pen additionally verifies that its private selection is idle and singular.
#[derive(Clone)]
pub(crate) struct Request {
    origin: Arc<Project>,
    revision: u64,
    transport: u64,
    composition: CompositionId,
    tool: Tool,
    selection: Option<LayerId>,
    selected_layers: BTreeSet<LayerId>,
    contents_selection: Option<(CompositionId, LayerId, u64)>,
    pub layer: LayerId,
    pub target: PathTarget,
    pub index: usize,
    pub frame: Frame,
    pub path: VectorPath,
    pub world: Affine,
}
impl Request {
    pub fn new(
        s: &EditorState,
        layer: LayerId,
        target: PathTarget,
        index: usize,
        path: VectorPath,
        world: Affine,
    ) -> Result<Self, String> {
        if s.vertex_editor.is_some() {
            return Err("Finish the current vertex edit first".into());
        }
        if !Self::available(s, layer, target, index, &path, world) {
            return Err("Select one editable Pen vertex while playback is stopped".into());
        }
        Ok(Self {
            origin: Arc::new(s.editor.project().clone()),
            revision: s.document_revision,
            transport: s.transport_generation(),
            composition: s.editor.project().active_composition_id(),
            tool: s.tool,
            selection: s.editor.selected(),
            selected_layers: s.selected_layers.clone(),
            contents_selection: s.contents_selection,
            layer,
            target,
            index,
            frame: s.frame,
            path,
            world,
        })
    }
    /// Read-only button availability must not clone the entire source project.
    pub fn available(
        s: &EditorState,
        layer: LayerId,
        target: PathTarget,
        index: usize,
        path: &VectorPath,
        world: Affine,
    ) -> bool {
        s.vertex_editor.is_none()
            && Self::ready(s)
            && s.editor.selected() == Some(layer)
            && index < path.vertices.len()
            && path.valid()
            && evaluated(s, layer, target)
                .is_some_and(|(actual, transform)| actual == *path && transform == world)
    }
    fn ready(s: &EditorState) -> bool {
        s.tool == Tool::Pen
            && !s.playing
            && !s.preview_caching
            && !s.new_composition_requested
            && !s.media_open
            && !s.fonts_open
            && !s.queue_open
            && !s.exporting
            && !s.close_after_save
            && s.recovery.is_none()
            && s.colors.session.is_none()
            && s.text_session.is_none()
            && s.gradient_editor.is_none()
            && s.gradient_preview.is_none()
    }
    pub fn current(&self, s: &EditorState) -> bool {
        self.origin.as_ref() == s.editor.project()
            && self.revision == s.document_revision
            && self.transport == s.transport_generation()
            && self.composition == s.editor.project().active_composition_id()
            && self.frame == s.frame
            && self.tool == Tool::Pen
            && self.tool == s.tool
            && self.selection == Some(self.layer)
            && self.selection == s.editor.selected()
            && self.selected_layers == s.selected_layers
            && self.contents_selection == s.contents_selection
            && Self::ready(s)
            && self.index < self.path.vertices.len()
            && self.path.valid()
            && evaluated(s, self.layer, self.target)
                .is_some_and(|(path, world)| path == self.path && world == self.world)
    }
}

/// Use the same enabled Contents traversal and coordinate spaces as Pen hit testing.
fn evaluated(s: &EditorState, id: LayerId, target: PathTarget) -> Option<(VectorPath, Affine)> {
    let comp = s.editor.project().composition();
    if s.frame >= comp.duration() {
        return None;
    }
    let layer = comp.layer(id).filter(|layer| !layer.locked())?;
    let world = comp.world_transform(id, s.frame)?;
    world.inverse()?;
    if let PathTarget::Contents(item) = target {
        let Content::ShapeContents(contents) = layer.content() else {
            return None;
        };
        let (_, path, local) = contents
            .editable_paths(s.frame)
            .into_iter()
            .find(|(candidate, _, _)| *candidate == item)?;
        let world = world.compose(local);
        world.inverse()?;
        Some((path, world))
    } else {
        let (path, animation) = layer.path_animation(target)?;
        Some((animation.at(path, s.frame), world))
    }
}

pub(crate) struct Session {
    pub id: u64,
    pub frame: Frame,
    pub layer: LayerId,
    pub target: PathTarget,
    pub index: usize,
    request: Request,
    path: VectorPath,
    project: Project,
    errors: [Option<String>; 6],
    pub error: String,
    pub input_error: Option<usize>,
}
impl Session {
    pub fn new(s: &EditorState, request: Request) -> Result<Self, String> {
        if s.vertex_editor.is_some() || !request.current(s) {
            return Err("The selected vertex changed before the editor opened".into());
        }
        Ok(Self {
            id: crate::color_edit::next_gradient_gesture(),
            frame: request.frame,
            layer: request.layer,
            target: request.target,
            index: request.index,
            path: request.path.clone(),
            project: request.origin.as_ref().clone(),
            request,
            errors: Default::default(),
            error: String::new(),
            input_error: None,
        })
    }
    pub fn current(&self, s: &EditorState) -> bool {
        self.request.current(s)
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn path(&self) -> &VectorPath {
        &self.path
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn value(&self, index: usize) -> Option<f64> {
        let vertex = self.path.vertices.get(self.index)?;
        match index {
            0..=1 => Some(vertex.position[index]),
            2..=3 => Some(vertex.incoming[index - 2]),
            4..=5 => Some(vertex.outgoing[index - 4]),
            _ => None,
        }
    }
    pub fn field_value(&self, index: usize) -> Option<String> {
        self.value(index).map(|value| value.to_string())
    }
    pub fn has_input_error(&self, index: usize) -> bool {
        self.errors.get(index).is_some_and(Option::is_some)
    }
    fn update_error(&mut self) {
        self.input_error = self.errors.iter().position(Option::is_some);
        self.error = self
            .input_error
            .and_then(|index| self.errors[index].clone())
            .unwrap_or_default();
    }
    fn edit_command(&self, path: VectorPath) -> Command {
        Command::EditPath {
            id: self.layer,
            target: self.target,
            frame: self.frame,
            path,
        }
    }
    pub fn command(&self) -> Result<Option<Command>, String> {
        if self.input_error.is_some() {
            return Err(self.error.clone());
        }
        Ok((self.path != self.request.path).then(|| self.edit_command(self.path.clone())))
    }
    fn set_value(&mut self, index: usize, value: f64) -> Result<(), String> {
        if !value.is_finite() || value.abs() > 1_000_000.0 {
            return Err("Enter a finite value from -1000000 to 1000000".into());
        }
        let mut path = self.path.clone();
        let vertex = &mut path.vertices[self.index];
        match index {
            0..=1 => vertex.position[index] = value,
            2..=3 => vertex.incoming[index - 2] = value,
            4..=5 => vertex.outgoing[index - 4] = value,
            _ => return Err("This vertex field no longer exists".into()),
        }
        if path == self.path {
            return Ok(());
        }
        // Rebuild from the opening source, never the previous preview. Thus exactly
        // one EditPath reaches the draft and no historical pose slots accumulate.
        let project = if path == self.request.path {
            self.request.origin.as_ref().clone()
        } else {
            let mut draft = Editor::default();
            draft.replace_project(self.request.origin.as_ref().clone())?;
            draft.execute(self.edit_command(path.clone()))?;
            draft.project().clone()
        };
        self.path = path;
        self.project = project;
        Ok(())
    }
    pub fn input(&mut self, index: usize, text: &str) -> Result<(), String> {
        if index >= self.errors.len() {
            return Err("This vertex field no longer exists".into());
        }
        let result = text
            .trim()
            .parse::<f64>()
            .map_err(|_| "Enter a numeric value".to_owned())
            .and_then(|value| self.set_value(index, value));
        self.errors[index] = result.as_ref().err().cloned();
        self.update_error();
        result
    }
    fn return_request(&self, s: &EditorState) -> Option<Request> {
        let (path, world) = evaluated(s, self.layer, self.target)?;
        Request::new(s, self.layer, self.target, self.index, path, world).ok()
    }
}

impl EditorState {
    /// Late field callbacks must not edit a reopened dialog, even on the same vertex.
    pub(crate) fn vertex_input(&mut self, serial: u64, index: usize, text: &str) {
        self.invalidate_vertex_editor();
        if let Some(session) = &mut self.vertex_editor
            && session.id == serial
        {
            let _ = session.input(index, text);
        }
    }
    pub(crate) fn revert_vertex_field(&mut self, serial: u64, index: usize) -> Option<String> {
        self.invalidate_vertex_editor();
        let session = self.vertex_editor.as_mut().filter(|s| s.id == serial)?;
        if !session.has_input_error(index) {
            return None;
        }
        let value = session.field_value(index)?;
        session.errors[index] = None;
        session.update_error();
        Some(value)
    }
    pub(crate) fn invalidate_vertex_editor(&mut self) -> bool {
        if self
            .vertex_editor
            .as_ref()
            .is_some_and(|session| !session.current(self))
        {
            self.vertex_editor = None;
            self.vertex_return = None;
            self.status = "Vertex edit canceled because the editing context changed".into();
            true
        } else {
            false
        }
    }
    pub(crate) fn cancel_vertex_editor(&mut self) {
        if self.invalidate_vertex_editor() {
            return;
        }
        if let Some(session) = self.vertex_editor.take() {
            self.vertex_return = session.return_request(self);
            self.status = "Vertex edit canceled".into();
        }
    }
    pub(crate) fn accept_vertex_editor(&mut self) {
        if self.invalidate_vertex_editor() {
            return;
        }
        let Some(mut session) = self.vertex_editor.take() else {
            return;
        };
        match session.command() {
            Ok(Some(command)) => match self.editor.execute(command) {
                Ok(()) => self.status = "Vertex applied".into(),
                Err(error) => {
                    session.error = error;
                    self.vertex_editor = Some(session);
                    return;
                }
            },
            Ok(None) => self.status = "Vertex unchanged".into(),
            Err(error) => {
                session.error = error;
                self.vertex_editor = Some(session);
                return;
            }
        }
        self.vertex_return = session.return_request(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        Bezier, ContentsEdit, ContentsParam, Interpolation, PathMask, PathMaskMode, PathVertex,
        Property, PropertyPath, Shape, TrackEdit,
    };

    fn geometry(closed: bool, shift: f64) -> VectorPath {
        VectorPath {
            closed,
            vertices: (0..4)
                .map(|index| {
                    let n = index as f64;
                    PathVertex {
                        position: [30.123456789012345 + n * 40. + shift, 25. - n * 7. + shift],
                        incoming: [-3.25 - n, 2.75 + n],
                        outgoing: [7.5 + n, -1.125 - n],
                    }
                })
                .collect(),
        }
    }
    fn scene(target: PathTarget, animated: bool, frame: Frame) -> EditorState {
        let mut s = EditorState::default();
        s.tool = Tool::Pen;
        s.composition_started = true;
        s.editor
            .execute(Command::AddContent {
                content: Content::Shape(Shape {
                    path: Some(geometry(false, 0.)),
                    fill: false,
                    stroke_width: 3.25,
                    ..Default::default()
                }),
                width: 300.,
                height: 200.,
                name: "Explicit vertex".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::SetPathMasks {
                id: 1,
                masks: vec![PathMask {
                    path: geometry(true, 13.),
                    mode: PathMaskMode::Subtract,
                    inverted: true,
                    ..Default::default()
                }],
            })
            .unwrap();
        if matches!(target, PathTarget::Contents(_)) {
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Promote,
                })
                .unwrap();
            for (parameter, value) in [
                (ContentsParam::Transform(Property::Rotation), 19.),
                (ContentsParam::Skew, 23.),
                (ContentsParam::Transform(Property::ScaleX), -75.),
            ] {
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Track {
                            item: 1,
                            parameter,
                            edit: TrackEdit::Value { frame: 0, value },
                        },
                    })
                    .unwrap();
            }
            s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 2));
        }
        s.editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::Rotation,
                frame: 0,
                value: 31.,
            })
            .unwrap();
        if animated {
            s.editor
                .execute(Command::AnimatePath {
                    id: 1,
                    target,
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                })
                .unwrap();
            let opening = evaluated(&s, 1, target).unwrap().0;
            for (key, shift) in [(20, 18.), (40, 39.), (60, 18.)] {
                let mut path = opening.clone();
                for vertex in &mut path.vertices {
                    vertex.position[0] += shift;
                    vertex.incoming[1] -= shift / 4.;
                }
                s.editor
                    .execute(Command::EditPath {
                        id: 1,
                        target,
                        frame: key,
                        path,
                    })
                    .unwrap();
            }
            for key in [0, 20] {
                s.editor
                    .execute(Command::AnimatePath {
                        id: 1,
                        target,
                        edit: TrackEdit::Interpolate {
                            frame: key,
                            interpolation: Interpolation::Bezier(Bezier {
                                x1: 0.14,
                                y1: -0.1,
                                x2: 0.82,
                                y2: 1.3,
                            }),
                        },
                    })
                    .unwrap();
            }
        }
        s.editor.execute(Command::AddRectangle).unwrap();
        s.editor
            .execute(Command::RenameLayer {
                id: 2,
                name: "Untouched companion".into(),
            })
            .unwrap();
        s.editor.select(1);
        s.selected_layers = [1].into();
        s.frame = frame;
        s.editor.clear_history();
        s
    }
    fn request(s: &EditorState, target: PathTarget) -> Request {
        let (path, world) = evaluated(s, 1, target).unwrap();
        Request::new(s, 1, target, 1, path, world).unwrap()
    }
    fn open(s: &mut EditorState, target: PathTarget) -> u64 {
        let session = Session::new(s, request(s, target)).unwrap();
        let id = session.id;
        s.vertex_editor = Some(session);
        id
    }
    fn animation(project: &Project, target: PathTarget) -> serde_json::Value {
        serde_json::to_value(
            project
                .composition()
                .layer(1)
                .unwrap()
                .path_animation(target)
                .unwrap()
                .1,
        )
        .unwrap()
    }
    const TARGETS: [PathTarget; 3] = [
        PathTarget::Shape,
        PathTarget::Contents(2),
        PathTarget::Mask(1),
    ];

    #[test]
    fn six_local_coordinates_are_independent_and_static_targets_apply_one_exact_command() {
        for target in TARGETS {
            let mut s = scene(target, false, 0);
            let source = s.editor.project().clone();
            let baseline = source.to_json().unwrap();
            let initial = evaluated(&s, 1, target).unwrap().0;
            let mut expected = initial.clone();
            expected.vertices[1] = PathVertex {
                position: [-13.123456789012345, 123.98765432101234],
                incoming: [-22.75, 3.125],
                outgoing: [0.0625, -39.875],
            };
            let id = open(&mut s, target);
            for (field, value) in [
                -13.123456789012345,
                123.98765432101234,
                -22.75,
                3.125,
                0.0625,
                -39.875,
            ]
            .into_iter()
            .enumerate()
            {
                s.vertex_input(id, field, &value.to_string());
                assert_eq!(s.editor.project().to_json().unwrap(), baseline);
                assert!(!s.editor.can_undo());
                assert!(!s.editor.can_redo());
            }
            let draft = s.vertex_editor.as_ref().unwrap();
            assert_eq!(draft.path(), &expected);
            assert_eq!(draft.path().closed, initial.closed);
            assert_eq!(draft.path().vertices.len(), initial.vertices.len());
            assert!(
                matches!(draft.command().unwrap(), Some(Command::EditPath { id: 1, target: t, frame: 0, .. }) if t == target)
            );
            let preview = draft.project().clone();
            let mut independent = Editor::default();
            independent.replace_project(source.clone()).unwrap();
            independent
                .execute(Command::EditPath {
                    id: 1,
                    target,
                    frame: 0,
                    path: expected,
                })
                .unwrap();
            assert_eq!(&preview, independent.project());
            s.accept_vertex_editor();
            assert!(s.vertex_editor.is_none());
            assert!(s.vertex_return.as_ref().unwrap().current(&s));
            assert_eq!(s.editor.project(), &preview);
            let bytes = libre_effects_core::project_file::encode(&preview, None).unwrap();
            assert_eq!(
                libre_effects_core::project_file::decode(&bytes)
                    .unwrap()
                    .project,
                preview
            );
            assert!(s.editor.can_undo());
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            assert!(!s.editor.can_undo());
            assert!(s.editor.can_redo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &preview);
            assert!(!s.editor.can_redo());
        }
    }

    #[test]
    fn individual_component_edits_never_move_other_components_or_vertices() {
        for target in TARGETS {
            for field in 0..6 {
                let s = scene(target, false, 0);
                let mut session = Session::new(&s, request(&s, target)).unwrap();
                let before = session.path().clone();
                let values: Vec<_> = (0..6).map(|index| session.value(index).unwrap()).collect();
                session.input(field, "-999.125").unwrap();
                for (index, value) in values.into_iter().enumerate() {
                    assert_eq!(
                        session.value(index),
                        Some(if index == field { -999.125 } else { value })
                    );
                }
                for index in [0, 2, 3] {
                    assert_eq!(session.path().vertices[index], before.vertices[index]);
                }
                assert_eq!(
                    session.project().composition().layer(2),
                    s.editor.project().composition().layer(2)
                );
            }
        }
    }

    #[test]
    fn exact_opening_roundtrip_noop_and_away_back_preserve_source_keys_redo_and_legacy_version() {
        for target in TARGETS {
            for (animated, frame) in [(false, 0), (true, 20), (true, 10)] {
                for mode in 0..3 {
                    let mut s = scene(target, animated, frame);
                    s.editor
                        .execute(Command::RenameLayer {
                            id: 2,
                            name: "Redo sentinel".into(),
                        })
                        .unwrap();
                    let redo = s.editor.project().clone();
                    assert!(s.editor.can_undo());
                    s.editor.undo();
                    let source = s.editor.project().clone();
                    let id = open(&mut s, target);
                    let values: Vec<_> = (0..6)
                        .map(|index| {
                            s.vertex_editor
                                .as_ref()
                                .unwrap()
                                .field_value(index)
                                .unwrap()
                        })
                        .collect();
                    for (index, value) in values.iter().enumerate() {
                        s.vertex_input(id, index, value);
                    }
                    if mode != 0 {
                        for index in 0..6 {
                            s.vertex_input(id, index, "999.875");
                        }
                        if mode == 1 {
                            for (index, value) in values.iter().enumerate() {
                                s.vertex_input(id, index, value);
                            }
                        }
                    }
                    if mode == 2 {
                        s.cancel_vertex_editor();
                    } else {
                        let draft = s.vertex_editor.as_ref().unwrap();
                        assert!(draft.command().unwrap().is_none());
                        assert_eq!(draft.project(), &source);
                        s.accept_vertex_editor();
                    }
                    assert_eq!(s.editor.project(), &source);
                    assert!(!s.editor.can_undo());
                    assert!(s.editor.can_redo());
                    assert!(s.vertex_return.as_ref().unwrap().current(&s));
                    assert!(s.editor.can_redo());
                    s.editor.redo();
                    assert_eq!(s.editor.project(), &redo);
                }
            }
        }
        let mut s = scene(PathTarget::Shape, false, 0);
        let mut value = serde_json::to_value(s.editor.project()).unwrap();
        value["version"] = 30.into();
        let legacy = Project::from_json(&value.to_string()).unwrap();
        s.editor.replace_project(legacy.clone()).unwrap();
        s.editor.select(1);
        s.editor.clear_history();
        open(&mut s, PathTarget::Shape);
        s.accept_vertex_editor();
        assert_eq!(s.editor.project(), &legacy);
        assert!(!s.editor.can_undo());
    }

    #[test]
    fn existing_eased_keys_and_shared_referenced_poses_survive_one_vertex_edit() {
        for target in TARGETS {
            let mut s = scene(target, true, 20);
            let source = s.editor.project().clone();
            let previous = animation(&source, target);
            assert_eq!(
                previous["timing"]["keys"]["20"]["value"],
                previous["timing"]["keys"]["60"]["value"]
            );
            let id = open(&mut s, target);
            s.vertex_input(id, 5, "-46.12345678912345");
            let preview = s.vertex_editor.as_ref().unwrap().project().clone();
            s.accept_vertex_editor();
            let after = animation(s.editor.project(), target);
            for key in ["0", "40", "60"] {
                assert_eq!(
                    after["timing"]["keys"][key],
                    previous["timing"]["keys"][key]
                );
                let index = previous["timing"]["keys"][key]["value"].as_f64().unwrap() as usize;
                assert_eq!(after["poses"][index], previous["poses"][index]);
            }
            assert_eq!(
                after["timing"]["keys"]["20"]["interpolation"],
                previous["timing"]["keys"]["20"]["interpolation"]
            );
            assert_eq!(after["timing"]["keys"].as_object().unwrap().len(), 4);
            assert_eq!(s.editor.project(), &preview);
            assert!(s.editor.can_undo());
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
            assert!(s.editor.can_redo());
            s.editor.redo();
            assert_eq!(s.editor.project(), &preview);
            let bytes = libre_effects_core::project_file::encode(&preview, None).unwrap();
            assert_eq!(
                libre_effects_core::project_file::decode(&bytes)
                    .unwrap()
                    .project,
                preview
            );
        }
    }

    #[test]
    fn between_key_edit_starts_from_evaluated_pose_adds_only_linear_current_key() {
        for target in TARGETS {
            let mut s = scene(target, true, 10);
            let source = s.editor.project().clone();
            let previous = animation(&source, target);
            let evaluated = evaluated(&s, 1, target).unwrap().0;
            let id = open(&mut s, target);
            assert_eq!(s.vertex_editor.as_ref().unwrap().path(), &evaluated);
            s.vertex_input(id, 2, "21.5");
            let mut expected = evaluated;
            expected.vertices[1].incoming[0] = 21.5;
            s.accept_vertex_editor();
            let layer = s.editor.project().composition().layer(1).unwrap();
            let keys = layer.track(PropertyPath::Path(target)).unwrap().keys();
            assert_eq!(keys.len(), 5);
            assert_eq!(keys[&10].interpolation, Interpolation::Linear);
            let (base, animation_track) = layer.path_animation(target).unwrap();
            assert_eq!(animation_track.at(base, 10), expected);
            let after = animation(s.editor.project(), target);
            for key in ["0", "20", "40", "60"] {
                assert_eq!(
                    after["timing"]["keys"][key],
                    previous["timing"]["keys"][key]
                );
                let index = previous["timing"]["keys"][key]["value"].as_f64().unwrap() as usize;
                assert_eq!(after["poses"][index], previous["poses"][index]);
            }
            assert_eq!(
                base,
                source
                    .composition()
                    .layer(1)
                    .unwrap()
                    .path_animation(target)
                    .unwrap()
                    .0
            );
            assert!(s.editor.can_undo());
            s.editor.undo();
            assert_eq!(s.editor.project(), &source);
        }
    }

    #[test]
    fn numeric_limits_errors_and_roundtrip_precision_are_lossless() {
        let mut s = scene(PathTarget::Shape, true, 10);
        let source = s.editor.project().clone();
        let id = open(&mut s, PathTarget::Shape);
        for field in 0..6 {
            let session = s.vertex_editor.as_ref().unwrap();
            let value = session.value(field).unwrap();
            assert_eq!(
                session
                    .field_value(field)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    .to_bits(),
                value.to_bits()
            );
            for text in [
                "",
                "words",
                "NaN",
                "inf",
                "-inf",
                "1000000.0000001",
                "-1000000.01",
                "1e309",
            ] {
                let before = s.vertex_editor.as_ref().unwrap().project().clone();
                s.vertex_input(id, field, text);
                let session = s.vertex_editor.as_ref().unwrap();
                assert!(session.has_input_error(field), "{text}");
                assert_eq!(session.project(), &before);
                assert!(session.command().is_err());
                s.accept_vertex_editor();
                assert!(s.vertex_editor.is_some());
                assert_eq!(s.editor.project(), &source);
                assert_eq!(
                    s.revert_vertex_field(id, field)
                        .unwrap()
                        .parse::<f64>()
                        .unwrap(),
                    value
                );
            }
            for text in ["1000000", "-1000000", " 1.2345678901234567e-15 "] {
                s.vertex_input(id, field, text);
                let session = s.vertex_editor.as_ref().unwrap();
                assert!(!session.has_input_error(field));
                assert_eq!(session.value(field), Some(text.trim().parse().unwrap()));
            }
        }
        let session = s.vertex_editor.as_mut().unwrap();
        assert!(session.input(6, "1").is_err());
        assert!(session.input(usize::MAX, "1").is_err());
        assert_eq!(session.value(6), None);
        assert_eq!(session.field_value(6), None);
        s.cancel_vertex_editor();
        assert_eq!(s.editor.project(), &source);
        assert!(!s.editor.can_undo());
    }

    #[test]
    fn independent_bad_fields_stay_invalid_until_each_is_fixed_or_reverted() {
        let mut s = scene(PathTarget::Shape, false, 0);
        let source = s.editor.project().clone();
        let id = open(&mut s, PathTarget::Shape);
        s.vertex_input(id, 0, "bad anchor");
        s.vertex_input(id, 3, "NaN");
        s.vertex_input(id, 4, "72.25");
        let session = s.vertex_editor.as_ref().unwrap();
        assert_eq!(session.input_error, Some(0));
        assert!(session.has_input_error(0));
        assert!(session.has_input_error(3));
        assert!(!session.has_input_error(4));
        s.accept_vertex_editor();
        assert_eq!(s.editor.project(), &source);
        assert!(s.vertex_editor.is_some());
        assert!(s.revert_vertex_field(id, 0).is_some());
        assert_eq!(s.vertex_editor.as_ref().unwrap().input_error, Some(3));
        s.vertex_input(id, 3, "2.5");
        assert_eq!(s.vertex_editor.as_ref().unwrap().input_error, None);
        s.accept_vertex_editor();
        assert!(s.vertex_editor.is_none());
        assert_ne!(s.editor.project(), &source);
    }

    #[test]
    fn stale_callbacks_cannot_edit_reopened_dialog_or_apply_cancelled_values() {
        let mut s = scene(PathTarget::Shape, true, 10);
        let source = s.editor.project().clone();
        let first = open(&mut s, PathTarget::Shape);
        s.vertex_input(first, 0, "1234");
        s.cancel_vertex_editor();
        let token = s.vertex_return.take().unwrap();
        assert!(token.current(&s));
        let second = open(&mut s, PathTarget::Shape);
        assert_ne!(first, second);
        s.vertex_input(first, 0, "9999");
        s.vertex_input(first, 3, "not numeric");
        assert_eq!(s.revert_vertex_field(first, 0), None);
        let session = s.vertex_editor.as_ref().unwrap();
        assert!(session.command().unwrap().is_none());
        assert!(session.error.is_empty());
        assert_eq!(session.project(), &source);
        s.vertex_input(second, 1, "52.25");
        let expected = s.vertex_editor.as_ref().unwrap().project().clone();
        s.accept_vertex_editor();
        s.vertex_input(first, 0, "88");
        s.vertex_input(second, 0, "77");
        assert_eq!(s.editor.project(), &expected);
    }

    #[test]
    fn stale_context_changes_invalidate_draft_and_never_issue_selection_return() {
        let changes: Vec<Box<dyn Fn(&mut EditorState)>> = vec![
            Box::new(|s| s.frame += 1),
            Box::new(|s| s.tool = Tool::Select),
            Box::new(|s| s.editor.select(2)),
            Box::new(|s| s.editor.clear_selection()),
            Box::new(|s| {
                s.selected_layers.insert(2);
            }),
            Box::new(|s| s.contents_selection = None),
            Box::new(|s| s.contents_selection = Some((1, 1, 3))),
            Box::new(|s| s.document_revision += 1),
            Box::new(|s| s.playing = true),
            Box::new(|s| s.preview_caching = true),
            Box::new(|s| s.new_composition_requested = true),
            Box::new(|s| s.media_open = true),
            Box::new(|s| s.fonts_open = true),
            Box::new(|s| s.queue_open = true),
            Box::new(|s| s.exporting = true),
            Box::new(|s| s.close_after_save = true),
            Box::new(|s| {
                s.editor
                    .execute(Command::RenameLayer {
                        id: 2,
                        name: "Changed without revision".into(),
                    })
                    .unwrap();
            }),
            Box::new(|s| {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }),
            Box::new(|s| {
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Enabled {
                            item: 1,
                            enabled: false,
                        },
                    })
                    .unwrap();
            }),
            Box::new(|s| {
                s.editor.execute(Command::NewComposition).unwrap();
            }),
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut s = scene(PathTarget::Contents(2), true, 10);
            let id = open(&mut s, PathTarget::Contents(2));
            s.vertex_input(id, 0, "100.5");
            change(&mut s);
            let after_external_change = s.editor.project().clone();
            assert!(s.invalidate_vertex_editor(), "change {index}");
            assert!(s.vertex_editor.is_none());
            assert!(s.vertex_return.is_none());
            s.vertex_input(id, 0, "15");
            s.accept_vertex_editor();
            s.cancel_vertex_editor();
            assert!(s.vertex_return.is_none());
            assert_eq!(s.editor.project(), &after_external_change);
        }
        let mut s = scene(PathTarget::Shape, false, 0);
        open(&mut s, PathTarget::Shape);
        s.vertex_editor.as_mut().unwrap().request.transport =
            s.transport_generation().wrapping_add(1);
        assert!(s.invalidate_vertex_editor());
        assert!(s.vertex_return.is_none());
    }

    #[test]
    fn request_rejects_wrong_target_index_geometry_transform_and_disabled_group() {
        let mut s = scene(PathTarget::Contents(2), true, 10);
        let valid = request(&s, PathTarget::Contents(2));
        for target in [
            PathTarget::Shape,
            PathTarget::Contents(1),
            PathTarget::Contents(999),
            PathTarget::Mask(999),
        ] {
            assert!(Request::new(&s, 1, target, 1, valid.path.clone(), valid.world).is_err());
        }
        for index in [4, usize::MAX] {
            assert!(
                Request::new(&s, 1, valid.target, index, valid.path.clone(), valid.world).is_err()
            );
        }
        let mut wrong = valid.path.clone();
        wrong.vertices[0].position[0] += 0.001;
        assert!(Request::new(&s, 1, valid.target, 1, wrong, valid.world).is_err());
        assert!(
            Request::new(
                &s,
                1,
                valid.target,
                1,
                valid.path.clone(),
                Affine::default()
            )
            .is_err()
        );
        assert!(Request::new(&s, 2, valid.target, 1, valid.path.clone(), valid.world).is_err());
        let session = Session::new(&s, valid.clone()).unwrap();
        s.vertex_editor = Some(session);
        assert!(Session::new(&s, valid.clone()).is_err());
        assert!(Request::new(&s, 1, valid.target, 1, valid.path.clone(), valid.world).is_err());
        assert!(s.vertex_editor.as_ref().unwrap().current(&s));
        s.cancel_vertex_editor();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Enabled {
                    item: 1,
                    enabled: false,
                },
            })
            .unwrap();
        assert!(evaluated(&s, 1, valid.target).is_none());
        assert!(Session::new(&s, valid.clone()).is_err());
        assert!(Request::new(&s, 1, valid.target, 1, valid.path, valid.world).is_err());
    }

    #[test]
    fn preview_rebuilds_from_frozen_source_without_accumulating_animated_pose_slots() {
        for target in TARGETS {
            let s = scene(target, true, 10);
            let source = s.editor.project().clone();
            let old_count = animation(&source, target)["poses"]
                .as_array()
                .unwrap()
                .len();
            let mut session = Session::new(&s, request(&s, target)).unwrap();
            for value in 0..50 {
                session
                    .input(0, &(500. + value as f64).to_string())
                    .unwrap();
                assert_eq!(
                    animation(session.project(), target)["poses"]
                        .as_array()
                        .unwrap()
                        .len(),
                    old_count + 1
                );
                assert_eq!(s.editor.project(), &source);
            }
            assert!(matches!(
                session.command().unwrap(),
                Some(Command::EditPath { .. })
            ));
        }
    }

    #[test]
    fn return_request_is_exact_and_stale_selection_return_is_rejected() {
        for accept in [false, true] {
            let mut s = scene(PathTarget::Contents(2), true, 10);
            let id = open(&mut s, PathTarget::Contents(2));
            s.vertex_input(id, 0, "222.25");
            if accept {
                s.accept_vertex_editor();
            } else {
                s.cancel_vertex_editor();
            }
            let returned = s.vertex_return.take().unwrap();
            assert!(returned.current(&s));
            assert_eq!(returned.path, evaluated(&s, 1, returned.target).unwrap().0);
            s.selected_layers.insert(2);
            assert!(!returned.current(&s));
            s.selected_layers.remove(&2);
            assert!(returned.current(&s));
            s.editor
                .execute(Command::RenameLayer {
                    id: 2,
                    name: "Intervening source".into(),
                })
                .unwrap();
            assert!(!returned.current(&s));
        }
    }

    #[test]
    fn rejected_pose_limit_candidate_preserves_last_valid_draft_and_source_history() {
        let mut s = EditorState::default();
        s.tool = Tool::Pen;
        s.composition_started = true;
        s.editor
            .execute(Command::ConfigureComposition {
                name: "Full pose table".into(),
                width: 300,
                height: 200,
                fps: 30,
                duration: 10000,
            })
            .unwrap();
        let mut base = geometry(false, 0.);
        base.vertices.truncate(3);
        base.vertices[1].position[0] = 0.;
        let poses: Vec<_> = (0..10000)
            .map(|index| {
                let mut pose = base.clone();
                pose.vertices[1].position[0] = index as f64;
                pose
            })
            .collect();
        let keys: std::collections::BTreeMap<_, _> = (0..10000)
            .map(|frame| {
                (
                    frame,
                    libre_effects_core::Keyframe {
                        value: frame as f64,
                        interpolation: Interpolation::Linear,
                        temporal: Default::default(),
                    },
                )
            })
            .collect();
        let path_animation = serde_json::from_value(serde_json::json!({
            "poses": poses, "timing": { "value": 0., "keys": keys }
        }))
        .unwrap();
        s.editor
            .execute(Command::AddContent {
                content: Content::Shape(Shape {
                    path: Some(base),
                    path_animation,
                    fill: false,
                    stroke_width: 3.,
                    ..Default::default()
                }),
                width: 300.,
                height: 200.,
                name: "Bounded animation".into(),
            })
            .unwrap();
        s.selected_layers = [1].into();
        s.frame = 20;
        s.editor.clear_history();
        s.editor
            .execute(Command::RenameLayer {
                id: 1,
                name: "Redo sentinel".into(),
            })
            .unwrap();
        s.editor.undo();
        let source = s.editor.project().clone();
        let id = open(&mut s, PathTarget::Shape);
        // An existing referenced pose remains a legal change at the geometry cap.
        s.vertex_input(id, 0, "21");
        let valid = s.vertex_editor.as_ref().unwrap().project().clone();
        assert_ne!(valid, source);
        // A unique pose cannot be interned. This must fail before replacing draft.
        s.vertex_input(id, 0, "10000");
        let session = s.vertex_editor.as_ref().unwrap();
        assert_eq!(session.value(0), Some(21.));
        assert_eq!(session.project(), &valid);
        assert!(session.error.contains("geometry limit"));
        assert_eq!(session.input_error, Some(0));
        assert_eq!(s.editor.project(), &source);
        assert!(!s.editor.can_undo());
        assert!(s.editor.can_redo());
        s.accept_vertex_editor();
        assert!(s.vertex_editor.is_some());
        assert_eq!(s.editor.project(), &source);
        assert_eq!(s.revert_vertex_field(id, 0), Some("21".into()));
        s.accept_vertex_editor();
        assert!(s.vertex_editor.is_none());
        assert_eq!(s.editor.project(), &valid);
        assert!(s.editor.can_undo());
        assert!(!s.editor.can_redo());
        s.editor.undo();
        assert_eq!(s.editor.project(), &source);
        assert!(!s.editor.can_undo());
        s.editor.redo();
        assert_eq!(s.editor.project(), &valid);
    }

    #[test]
    fn pending_save_and_close_rejects_opening_without_restricting_ordinary_save() {
        let mut s = scene(PathTarget::Shape, false, 0);
        let opening = request(&s, PathTarget::Shape);
        s.saving = true;
        assert!(opening.current(&s));
        assert!(Request::available(
            &s,
            opening.layer,
            opening.target,
            opening.index,
            &opening.path,
            opening.world
        ));
        s.close_after_save = true;
        assert!(!opening.current(&s));
        assert!(!Request::available(
            &s,
            opening.layer,
            opening.target,
            opening.index,
            &opening.path,
            opening.world
        ));
        assert!(Session::new(&s, opening.clone()).is_err());
        assert!(
            Request::new(
                &s,
                opening.layer,
                opening.target,
                opening.index,
                opening.path,
                opening.world
            )
            .is_err()
        );
        assert!(!s.editor.can_undo());
    }
}
