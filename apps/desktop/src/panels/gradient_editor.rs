//! A modal, isolated Contents gradient transaction. No draft enters source history or I/O.
use crate::editor::EditorState;
use libre_effects_core::{
    Command, Content, ContentsEdit, ContentsNode, ContentsParam, Editor, Frame, GradientColorsEdit,
    GradientParam, LayerId, Project, TrackEdit,
};
use std::sync::Arc;

#[path = "gradient_editor_view.rs"]
mod view;
pub(crate) use view::GradientEditor;

pub(crate) struct Session {
    pub id: u64,
    origin: Arc<Project>,
    revision: u64,
    transport: u64,
    tool: crate::editor::Tool,
    pub frame: Frame,
    pub layer: LayerId,
    pub item: u64,
    draft: Editor,
    commands: Vec<Command>,
    compound: bool,
    selection_generation: u64,
    segment_start: usize,
    pub selected: GradientParam,
    pub error: String,
    pub input_error: Option<usize>,
}
impl Session {
    pub fn new(s: &EditorState, item: u64) -> Result<Self, String> {
        let layer = s.editor.selected_layer().ok_or("Select a shape layer")?;
        if layer.locked() {
            return Err("Unlock the layer before editing its gradient".into());
        }
        let Content::ShapeContents(contents) = layer.content() else {
            return Err("Select a Contents gradient paint".into());
        };
        let gradient = contents
            .node(item)
            .and_then(|n| n.kind.gradient())
            .ok_or("Select a Gradient Fill or Gradient Stroke")?;
        let compound = gradient.colors_animation().is_some();
        let mut draft = Editor::default();
        draft.replace_project(s.editor.project().clone())?;
        draft.select(layer.id());
        // Compound colors are edited in a temporary static sample. The original
        // compound track remains untouched, and OK writes one complete snapshot.
        if compound {
            draft.execute(Command::Contents {
                id: layer.id(),
                edit: ContentsEdit::GradientColors {
                    item,
                    edit: GradientColorsEdit::SetAnimation {
                        frame: s.frame,
                        enabled: false,
                    },
                },
            })?;
        }
        let Content::ShapeContents(draft_contents) = draft.selected_layer().unwrap().content()
        else {
            unreachable!()
        };
        let selected = GradientParam::ColorPosition(
            draft_contents
                .node(item)
                .unwrap()
                .kind
                .gradient()
                .unwrap()
                .colors[0],
        );
        draft.clear_history();
        let session = Self {
            id: crate::color_edit::next_gradient_gesture(),
            origin: Arc::new(s.editor.project().clone()),
            revision: s.document_revision,
            transport: s.transport_generation(),
            tool: s.tool,
            frame: s.frame,
            layer: layer.id(),
            item,
            draft,
            commands: vec![],
            compound,
            selection_generation: 0,
            segment_start: 0,
            selected,
            error: String::new(),
            input_error: None,
        };
        if !session.current(s) {
            return Err("Stop playback and select this gradient paint before editing".into());
        }
        Ok(session)
    }
    pub fn current(&self, s: &EditorState) -> bool {
        self.origin.as_ref() == s.editor.project()
            && self.revision == s.document_revision
            && self.transport == s.transport_generation()
            && self.frame == s.frame
            && self.tool == s.tool
            && !s.playing
            && s.colors.session.is_none()
            && s.text_session.is_none()
            && s.editor.selected() == Some(self.layer)
            && s.contents_selection
                == Some((self.origin.active_composition_id(), self.layer, self.item))
    }
    pub fn node(&self) -> &ContentsNode {
        let Content::ShapeContents(contents) = self
            .draft
            .project()
            .composition()
            .layer(self.layer)
            .unwrap()
            .content()
        else {
            unreachable!()
        };
        contents.node(self.item).unwrap()
    }
    fn original_node(&self) -> &ContentsNode {
        let Content::ShapeContents(contents) = self
            .origin
            .composition()
            .layer(self.layer)
            .unwrap()
            .content()
        else {
            unreachable!()
        };
        contents.node(self.item).unwrap()
    }
    fn corrections(&self) -> Vec<Command> {
        let original = self.original_node();
        let current = self.node();
        original
            .parameters
            .iter()
            .filter_map(|(&parameter, track)| {
                let ContentsParam::Gradient(p) = parameter else {
                    return None;
                };
                let changed = current.parameters.get(&parameter)?;
                if changed == track {
                    return None;
                }
                let before = original.value_at(parameter, self.frame);
                let after = current.value_at(parameter, self.frame);
                let same = if matches!(
                    p,
                    GradientParam::Red(_) | GradientParam::Green(_) | GradientParam::Blue(_)
                ) {
                    before.round() == after.round()
                } else {
                    (before - after).abs() < 1e-8
                };
                if !same {
                    return None;
                }
                // A return to the opening value restores the original track, including
                // removing a key created only by the modal. Keep historical sampling
                // commands: new stops may have sampled an earlier, intentional draft.
                let edit = if !track.keys().is_empty() && !track.keys().contains_key(&self.frame) {
                    TrackEdit::ToggleKey { frame: self.frame }
                } else {
                    TrackEdit::Value {
                        frame: self.frame,
                        value: track.value_at(self.frame),
                    }
                };
                Some(Command::Contents {
                    id: self.layer,
                    edit: ContentsEdit::Track {
                        item: self.item,
                        parameter,
                        edit,
                    },
                })
            })
            .collect()
    }
    fn unchanged_paint(&self, project: &Project) -> bool {
        let Content::ShapeContents(contents) =
            project.composition().layer(self.layer).unwrap().content()
        else {
            unreachable!()
        };
        let mut current = contents.node(self.item).unwrap().clone();
        let original = self.original_node();
        let before = original.kind.gradient().unwrap();
        let after = current.kind.gradient().unwrap();
        if before.colors != after.colors
            || before.opacities != after.opacities
            || before.radial != after.radial
        {
            return false;
        }
        // Added-then-removed stops can advance only the private ID allocator. If
        // every user-visible property/track is restored, this is still a no-op.
        match &mut current.kind {
            libre_effects_core::ContentsKind::GradientFill { gradient, .. }
            | libre_effects_core::ContentsKind::GradientStroke { gradient, .. } => {
                *gradient = before.clone()
            }
            _ => unreachable!(),
        }
        current == *original
    }
    fn finalized(&self) -> Result<(Project, Vec<Command>), String> {
        if self.compound {
            let original = self.original_node();
            let node = self.node();
            let mut colors = node.kind.gradient().unwrap().colors_at(node, self.frame);
            let opening = original
                .kind
                .gradient()
                .unwrap()
                .colors_at(original, self.frame);
            // RGB/HEX controls display byte colors. Restore untouched/display-
            // equal channel precision, including a sampled fractional opening
            // value reached again after a color-picker round trip.
            for stop in &mut colors.colors {
                if let Some(before) = opening.colors.iter().find(|before| before.id == stop.id) {
                    for (after, value) in [
                        (&mut stop.red, before.red),
                        (&mut stop.green, before.green),
                        (&mut stop.blue, before.blue),
                    ] {
                        if after.round() == value.round() {
                            *after = value;
                        }
                    }
                }
            }
            if opening == colors {
                return Ok((self.origin.as_ref().clone(), vec![]));
            }
            let command = Command::Contents {
                id: self.layer,
                edit: ContentsEdit::GradientColors {
                    item: self.item,
                    edit: GradientColorsEdit::Set {
                        frame: self.frame,
                        colors,
                    },
                },
            };
            let mut temporary = Editor::default();
            temporary.replace_project(self.origin.as_ref().clone())?;
            temporary.execute(command.clone())?;
            return Ok((temporary.project().clone(), vec![command]));
        }
        let corrections = self.corrections();
        let project = if corrections.is_empty() {
            self.draft.project().clone()
        } else {
            let mut temporary = Editor::default();
            temporary.replace_project(self.draft.project().clone())?;
            temporary.execute(Command::Batch(corrections.clone()))?;
            temporary.project().clone()
        };
        Ok((
            if self.unchanged_paint(&project) {
                self.origin.as_ref().clone()
            } else {
                project
            },
            corrections,
        ))
    }
    pub fn preview(&self, s: &EditorState) -> Option<Project> {
        if !self.current(s) {
            return None;
        }
        self.finalized().ok().map(|(project, _)| project)
    }
    pub fn command(&self) -> Result<Option<Command>, String> {
        let (project, corrections) = self.finalized()?;
        if project == *self.origin {
            return Ok(None);
        }
        if self.compound {
            return Ok(corrections.into_iter().next());
        }
        let mut commands = self.commands.clone();
        commands.extend(corrections);
        Ok(Some(Command::Batch(commands)))
    }
    fn apply(&mut self, command: Command) -> Result<(), String> {
        self.draft.execute(command.clone())?;
        self.draft.clear_history();
        self.record(command);
        self.error.clear();
        self.input_error = None;
        Ok(())
    }
    fn apply_draft_sequence(&mut self, commands: Vec<Command>) -> Result<(), String> {
        // A multi-step private bridge is atomic even if its second step exceeds
        // a validation or metadata bound. Only the finished candidate is adopted.
        let mut temporary = Editor::default();
        temporary.replace_project(self.draft.project().clone())?;
        temporary.select(self.layer);
        for command in &commands {
            temporary.execute(command.clone())?;
        }
        temporary.clear_history();
        self.draft = temporary;
        for command in commands {
            self.record(command);
        }
        self.error.clear();
        self.input_error = None;
        Ok(())
    }
    fn record(&mut self, command: Command) {
        // Scalars commute until a topology operation samples their values. Keep the last
        // value for each scalar in that segment, without recording every pointer move.
        if let Command::Batch(commands) = command {
            for command in commands {
                self.record(command);
            }
            return;
        }
        if let Command::Contents {
            edit: ContentsEdit::Track { parameter, .. },
            ..
        } = &command
        {
            if let Some(slot) = self.commands[self.segment_start..].iter_mut().find(|old| {
                matches!(old, Command::Contents { edit: ContentsEdit::Track { parameter: p, .. }, .. } if p == parameter)
            }) {
                *slot = command;
                return;
            }
        } else {
            self.segment_start = self.commands.len() + 1;
        }
        self.commands.push(command);
    }
    pub fn value(&self, parameter: GradientParam) -> f64 {
        self.node()
            .value_at(ContentsParam::Gradient(parameter), self.frame)
    }
    pub fn set_value(&mut self, parameter: GradientParam, value: f64) -> Result<(), String> {
        let (lo, hi) = parameter.bounds();
        if !value.is_finite() || !(lo..=hi).contains(&value) {
            return Err(format!("Enter a finite value from {lo} to {hi}"));
        }
        if !self
            .node()
            .parameters
            .contains_key(&ContentsParam::Gradient(parameter))
        {
            return Err("This gradient stop no longer exists".into());
        }
        let previous = self.value(parameter);
        if if self.compound {
            previous.to_bits() == value.to_bits()
        } else {
            (previous - value).abs() < 1e-8
        } {
            self.error.clear();
            self.input_error = None;
            return Ok(());
        }
        if self.compound && previous == value && previous.to_bits() != value.to_bits() {
            // Legacy scalar equality treats signed zero alike, while total_cmp
            // distinguishes coincident-stop order. Neither private bridge state
            // is previewed; both steps must validate before the draft is adopted.
            let command = |value| Command::Contents {
                id: self.layer,
                edit: ContentsEdit::Track {
                    item: self.item,
                    parameter: ContentsParam::Gradient(parameter),
                    edit: TrackEdit::Value {
                        frame: self.frame,
                        value,
                    },
                },
            };
            return self.apply_draft_sequence(vec![command(1.), command(value)]);
        }
        self.apply(Command::Contents {
            id: self.layer,
            edit: ContentsEdit::Track {
                item: self.item,
                parameter: ContentsParam::Gradient(parameter),
                edit: TrackEdit::Value {
                    frame: self.frame,
                    value,
                },
            },
        })
    }
    pub fn add(&mut self, opacity: bool, position: f64) -> Result<(), String> {
        self.apply(Command::Contents {
            id: self.layer,
            edit: ContentsEdit::AddGradientStop {
                item: self.item,
                opacity,
                position,
                frame: self.frame,
            },
        })?;
        self.selection_generation = self
            .selection_generation
            .checked_add(1)
            .expect("Gradient selection exhausted");
        let gradient = self.node().kind.gradient().unwrap();
        self.selected = if opacity {
            GradientParam::OpacityPosition(*gradient.opacities.last().unwrap())
        } else {
            GradientParam::ColorPosition(*gradient.colors.last().unwrap())
        };
        Ok(())
    }
    pub fn remove(&mut self) -> Result<(), String> {
        let stop = self.selected.stop().ok_or("Select a stop")?;
        let opacity = self
            .node()
            .kind
            .gradient()
            .unwrap()
            .opacities
            .contains(&stop);
        self.apply(Command::Contents {
            id: self.layer,
            edit: ContentsEdit::RemoveGradientStop {
                item: self.item,
                stop,
            },
        })?;
        self.selection_generation = self
            .selection_generation
            .checked_add(1)
            .expect("Gradient selection exhausted");
        let gradient = self.node().kind.gradient().unwrap();
        self.selected = if opacity {
            GradientParam::OpacityPosition(gradient.opacities[0])
        } else {
            GradientParam::ColorPosition(gradient.colors[0])
        };
        Ok(())
    }
    pub fn select(&mut self, parameter: GradientParam) -> Result<(), String> {
        if !self
            .node()
            .parameters
            .contains_key(&ContentsParam::Gradient(parameter))
        {
            return Err("Select an existing gradient stop".into());
        }
        self.selection_generation = self
            .selection_generation
            .checked_add(1)
            .expect("Gradient selection exhausted");
        self.selected = parameter;
        self.error.clear();
        self.input_error = None;
        Ok(())
    }
    pub fn opacity(&self) -> bool {
        self.selected.stop().is_some_and(|stop| {
            self.node()
                .kind
                .gradient()
                .unwrap()
                .opacities
                .contains(&stop)
        })
    }
    fn number_text(&self, value: f64) -> String {
        if self.compound {
            value.to_string()
        } else {
            format!("{value:.2}")
        }
    }
    pub fn field_value(&self, index: usize) -> Option<String> {
        let stop = self.selected.stop()?;
        let opacity = self.opacity();
        Some(match index {
            0 => self.number_text(self.value(if opacity {
                GradientParam::OpacityPosition(stop)
            } else {
                GradientParam::ColorPosition(stop)
            })),
            1 => self.number_text(self.value(if opacity {
                GradientParam::OpacityMidpoint(stop)
            } else {
                GradientParam::ColorMidpoint(stop)
            })),
            2 if opacity => self.number_text(self.value(GradientParam::Opacity(stop))),
            3..=6 if !opacity => {
                let color = self
                    .node()
                    .kind
                    .gradient()?
                    .color_at(self.node(), stop, self.frame)?;
                if index == 3 {
                    format!("{color:06X}")
                } else {
                    ((color >> ((6 - index) * 8)) & 255).to_string()
                }
            }
            _ => return None,
        })
    }
    pub fn input(&mut self, index: usize, text: &str) -> Result<(), String> {
        let stop = self.selected.stop().ok_or("Select a stop")?;
        if index < 3 {
            let parameter = match (index, self.opacity()) {
                (0, false) => GradientParam::ColorPosition(stop),
                (0, true) => GradientParam::OpacityPosition(stop),
                (1, false) => GradientParam::ColorMidpoint(stop),
                (1, true) => GradientParam::OpacityMidpoint(stop),
                (2, true) => GradientParam::Opacity(stop),
                _ => return Err("Select an opacity stop".into()),
            };
            let value = text.trim().parse().map_err(|_| "Enter a numeric value")?;
            self.set_value(parameter, value)
        } else {
            let mut color = crate::color_edit::Session::new(
                crate::color_edit::Target::GradientStop(self.layer, self.item, stop),
                self.draft.project(),
                0,
                self.frame,
            )?;
            let normalized;
            let input = if (4..=6).contains(&index) {
                let value: f64 = text
                    .trim()
                    .parse()
                    .map_err(|_| "RGB values must be integers from 0 to 255")?;
                if !value.is_finite() || !(0. ..=255.).contains(&value) || value.fract() != 0. {
                    return Err("RGB values must be integers from 0 to 255".into());
                }
                normalized = (value as u8).to_string();
                normalized.as_str()
            } else {
                text
            };
            color.input(index - 3, input)?;
            if let Some(command) = color.command() {
                self.apply(command)?;
            }
            self.error.clear();
            self.input_error = None;
            Ok(())
        }
    }
}

impl EditorState {
    /// Compound fields also bind the exact stop-selection generation. A delayed
    /// callback cannot regain authority after selecting another stop and back.
    pub(crate) fn gradient_compound_input(
        &mut self,
        session_id: u64,
        generation: u64,
        stop: u64,
        index: usize,
        text: &str,
    ) -> String {
        self.invalidate_gradient_editor();
        if self.gradient_editor.as_ref().is_some_and(|session| {
            session.id == session_id
                && session.compound
                && session.selection_generation == generation
                && session.selected.stop() == Some(stop)
        }) {
            self.gradient_input(session_id, index, text);
        }
        self.gradient_editor
            .as_ref()
            .and_then(|session| session.field_value(index))
            .unwrap_or_default()
    }
    /// The originating dialog generation is captured by each field callback.
    pub(crate) fn gradient_input(&mut self, session_id: u64, index: usize, text: &str) {
        self.invalidate_gradient_editor();
        if let Some(session) = &mut self.gradient_editor
            && session.id == session_id
        {
            if let Err(error) = session.input(index, text) {
                session.error = error;
                session.input_error = Some(index);
            }
        }
    }
    pub(crate) fn revert_gradient_field(
        &mut self,
        session_id: u64,
        index: usize,
    ) -> Option<String> {
        self.invalidate_gradient_editor();
        let session = self
            .gradient_editor
            .as_mut()
            .filter(|s| s.id == session_id)?;
        let value = (session.input_error == Some(index))
            .then(|| session.field_value(index))
            .flatten();
        session.error.clear();
        session.input_error = None;
        value
    }
    /// Called by both event dispatch and state observers; never commits a stale draft.
    pub(crate) fn invalidate_gradient_editor(&mut self) -> bool {
        if self
            .gradient_editor
            .as_ref()
            .is_some_and(|draft| !draft.current(self))
        {
            self.gradient_editor = None;
            self.status = "Gradient edit canceled because the editing context changed".into();
            true
        } else {
            false
        }
    }
    pub(crate) fn accept_gradient_editor(&mut self) {
        if self.invalidate_gradient_editor() {
            return;
        }
        let Some(mut draft) = self.gradient_editor.take() else {
            return;
        };
        if draft.input_error.is_some() {
            self.gradient_editor = Some(draft);
            return;
        }
        let command = match draft.command() {
            Ok(command) => command,
            Err(error) => {
                draft.error = error;
                self.gradient_editor = Some(draft);
                return;
            }
        };
        if let Some(command) = command {
            match self.editor.execute(command) {
                Ok(()) => self.status = "Gradient applied".into(),
                Err(error) => {
                    draft.error = error;
                    self.gradient_editor = Some(draft);
                }
            }
        } else {
            self.status = "Gradient unchanged".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{ContentsKind, PaintComposite, Project, Property, ShapeStroke};
    fn scene(stroke: bool) -> EditorState {
        let mut s = EditorState::default();
        s.editor
            .execute(Command::AddContent {
                content: Content::Shape(Default::default()),
                width: 160.,
                height: 100.,
                name: "Gradient".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Promote,
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 1,
                    kind: if stroke {
                        ContentsKind::GradientStroke {
                            style: ShapeStroke::default(),
                            gradient: Default::default(),
                        }
                    } else {
                        ContentsKind::GradientFill {
                            even_odd: false,
                            gradient: Default::default(),
                        }
                    },
                },
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Composite {
                    item: 5,
                    mode: PaintComposite::AbovePrevious,
                },
            })
            .unwrap();
        s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 5));
        s.editor.clear_history();
        s
    }
    fn animate(s: &mut EditorState, parameter: GradientParam) {
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item: 5,
                    parameter: ContentsParam::Gradient(parameter),
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                },
            })
            .unwrap();
    }
    fn draft(s: &EditorState) -> Session {
        Session::new(s, 5).unwrap()
    }

    #[test]
    fn modal_multiple_color_opacity_topology_edits_apply_one_undo_for_fill_and_stroke() {
        for stroke in [false, true] {
            let mut s = scene(stroke);
            for p in [
                GradientParam::ColorPosition(1),
                GradientParam::Red(1),
                GradientParam::Opacity(3),
            ] {
                animate(&mut s, p);
            }
            s.frame = 30;
            s.editor.clear_history();
            let before = s.editor.project().clone();
            let mut d = draft(&s);
            for v in [5., 10., 25.] {
                d.set_value(GradientParam::ColorPosition(1), v).unwrap();
            }
            d.set_value(GradientParam::ColorMidpoint(1), 35.).unwrap();
            d.input(3, "12EF34").unwrap();
            d.add(false, 70.).unwrap();
            let added_color = d.selected.stop().unwrap();
            d.input(3, "FEDCBA").unwrap();
            d.set_value(GradientParam::ColorPosition(added_color), 85.)
                .unwrap();
            d.selected = GradientParam::ColorPosition(2);
            d.remove().unwrap();
            d.add(true, 40.).unwrap();
            let added_opacity = d.selected.stop().unwrap();
            d.input(2, "35").unwrap();
            d.input(1, "65").unwrap();
            d.set_value(GradientParam::Opacity(3), 80.).unwrap();
            d.selected = GradientParam::OpacityPosition(4);
            d.remove().unwrap();
            assert_eq!(s.editor.project(), &before);
            assert!(!s.editor.can_undo());
            let preview = d.preview(&s).unwrap();
            assert_eq!(
                Project::from_json(&preview.to_json().unwrap()).unwrap(),
                preview
            );
            let parameters = &d.node().parameters;
            assert_eq!(
                parameters[&ContentsParam::Gradient(GradientParam::ColorPosition(1))]
                    .keys()
                    .len(),
                2
            );
            assert_eq!(
                parameters[&ContentsParam::Gradient(GradientParam::Red(1))]
                    .keys()
                    .len(),
                2
            );
            assert_eq!(
                parameters[&ContentsParam::Gradient(GradientParam::Opacity(3))]
                    .keys()
                    .len(),
                2
            );
            assert!(
                parameters[&ContentsParam::Gradient(GradientParam::Opacity(added_opacity))]
                    .keys()
                    .is_empty()
            );
            s.gradient_editor = Some(d);
            s.accept_gradient_editor();
            assert!(s.gradient_editor.is_none());
            assert_eq!(s.editor.project(), &preview);
            s.editor.undo();
            assert_eq!(s.editor.project(), &before);
            assert!(
                !s.editor.can_undo(),
                "All modal edits must be exactly one transaction"
            );
            s.editor.redo();
            assert_eq!(s.editor.project(), &preview);
        }
    }
    #[test]
    fn modal_preview_isolated_from_autosave_export_history_and_matches_accepted_pixels() {
        let mut s = scene(false);
        let before = s.editor.project().clone();
        let renderer = crate::rendering::Renderer::new();
        let pixels = renderer.render_output(&before, 0, 480, 270).unwrap();
        let mut d = draft(&s);
        d.input(3, "FF0000").unwrap();
        d.set_value(GradientParam::Opacity(3), 15.).unwrap();
        d.add(false, 30.).unwrap();
        let preview = d.preview(&s).unwrap();
        let draft_pixels = renderer.render_preview(&preview, 0, 480).unwrap();
        assert_ne!(draft_pixels, pixels);
        s.gradient_editor = Some(d);
        assert_eq!(
            s.text_project(),
            before,
            "Autosave snapshot excludes modal edits"
        );
        assert_eq!(
            renderer
                .render_output(s.editor.project(), 0, 480, 270)
                .unwrap(),
            pixels
        );
        assert_eq!(
            s.editor.project().to_json().unwrap(),
            before.to_json().unwrap()
        );
        assert!(!s.editor.can_undo());
        s.accept_gradient_editor();
        assert_eq!(
            renderer
                .render_output(s.editor.project(), 0, 480, 270)
                .unwrap(),
            draft_pixels
        );
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
    }
    #[test]
    fn modal_cancel_preserves_source_and_redo_after_repeated_reopening() {
        let mut s = scene(false);
        s.editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame: 0,
                value: 20.,
            })
            .unwrap();
        let redo = s.editor.project().clone();
        s.editor.undo();
        let before = s.editor.project().clone();
        for _ in 0..3 {
            let mut d = draft(&s);
            d.add(false, 45.).unwrap();
            d.input(3, "ABCDEF").unwrap();
            d.add(true, 60.).unwrap();
            d.input(2, "20").unwrap();
            s.gradient_editor = Some(d);
            // Cancel, Escape, window deactivation and window-close handlers all drop this draft.
            s.gradient_editor = None;
            assert_eq!(s.editor.project(), &before);
            assert_eq!(s.text_project(), before);
            assert!(s.editor.can_redo());
            assert!(!s.editor.can_undo());
        }
        s.editor.redo();
        assert_eq!(s.editor.project(), &redo);
    }
    #[test]
    fn modal_invalidated_by_document_frame_layer_item_tool_transport_and_other_dialog() {
        let cases: Vec<Box<dyn Fn(&mut EditorState)>> = vec![
            Box::new(|s| s.frame += 1),
            Box::new(|s| s.document_revision += 1),
            Box::new(|s| s.tool = crate::editor::Tool::Hand),
            Box::new(|s| s.playing = true),
            Box::new(|s| s.contents_selection = None),
            Box::new(|s| s.contents_selection.as_mut().unwrap().2 = 4),
            Box::new(|s| s.editor.clear_selection()),
            Box::new(|s| {
                s.editor.execute(Command::ToggleLocked(1)).unwrap();
            }),
            Box::new(|s| {
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Remove(5),
                    })
                    .unwrap();
            }),
            Box::new(|s| {
                s.editor.execute(Command::NewComposition).unwrap();
            }),
            Box::new(|s| {
                s.colors.session = Some(
                    crate::color_edit::Session::new(
                        crate::color_edit::Target::GradientStop(1, 5, 1),
                        s.editor.project(),
                        s.document_revision,
                        s.frame,
                    )
                    .unwrap(),
                );
            }),
        ];
        for mutate in cases {
            let mut s = scene(false);
            let mut d = draft(&s);
            d.input(3, "123456").unwrap();
            mutate(&mut s);
            let changed = s.editor.project().clone();
            assert!(!d.current(&s));
            assert!(d.preview(&s).is_none());
            s.gradient_editor = Some(d);
            s.accept_gradient_editor();
            assert!(s.gradient_editor.is_none());
            assert_eq!(
                s.editor.project(),
                &changed,
                "A stale modal must never overwrite newer work"
            );
        }
        // A stop/restart token can invalidate a draft even if playhead and playing are unchanged.
        let mut s = scene(false);
        let mut d = draft(&s);
        d.transport = d.transport.wrapping_add(1);
        s.gradient_editor = Some(d);
        assert!(s.invalidate_gradient_editor());
    }
    #[test]
    fn modal_stop_limits_and_failed_inputs_are_atomic() {
        for opacity in [false, true] {
            let s = scene(false);
            let mut d = draft(&s);
            if opacity {
                d.selected = GradientParam::OpacityPosition(3);
            }
            assert!(d.remove().is_err());
            for _ in 2..32 {
                d.add(opacity, 50.).unwrap();
            }
            let full = d.draft.project().clone();
            assert!(d.add(opacity, 50.).is_err());
            assert_eq!(d.draft.project(), &full);
            for v in [f64::NAN, f64::INFINITY, -1., 101.] {
                assert!(d.set_value(d.selected, v).is_err());
                assert_eq!(d.draft.project(), &full);
            }
            assert!(d.input(1, "0").is_err());
            assert!(d.input(1, "100").is_err());
            if opacity {
                assert!(d.input(2, "NaN").is_err());
            } else {
                assert!(d.input(3, "GGGGGG").is_err());
                assert!(d.input(3, "12345678").is_err());
                assert!(d.input(4, "256").is_err());
            }
            assert_eq!(d.draft.project(), &full);
            for _ in 2..32 {
                d.remove().unwrap();
            }
            assert!(d.remove().is_err());
            let mut commit = s.editor;
            commit.execute(d.command().unwrap().unwrap()).unwrap();
            assert_eq!(commit.project(), d.draft.project());
        }
    }
    #[test]
    fn modal_journal_coalesces_drag_values_but_preserves_sampling_boundaries() {
        let s = scene(false);
        let mut d = draft(&s);
        for value in 0..=100 {
            d.set_value(GradientParam::ColorPosition(1), value as f64)
                .unwrap();
        }
        assert_eq!(d.commands.len(), 1);
        d.set_value(GradientParam::ColorPosition(1), 0.).unwrap();
        assert!(
            d.command().unwrap().is_none(),
            "Returning to unchanged static values is a no-op"
        );
        d.input(3, "FF0000").unwrap();
        d.add(false, 25.).unwrap();
        let added = d.selected.stop().unwrap();
        let sampled_color = d
            .node()
            .kind
            .gradient()
            .unwrap()
            .color_at(d.node(), added, 0)
            .unwrap();
        d.selected = GradientParam::ColorPosition(1);
        d.input(3, "00FF00").unwrap();
        let mut commit = s.editor;
        commit.execute(d.command().unwrap().unwrap()).unwrap();
        assert_eq!(commit.project(), d.draft.project());
        assert_eq!(
            d.node()
                .kind
                .gradient()
                .unwrap()
                .color_at(d.node(), added, 0)
                .unwrap(),
            sampled_color
        );
    }
    #[test]
    fn modal_unchanged_and_invalid_accept_leave_source_history_untouched() {
        let mut s = scene(false);
        let before = s.editor.project().clone();
        s.gradient_editor = Some(draft(&s));
        s.accept_gradient_editor();
        assert!(s.gradient_editor.is_none());
        assert!(!s.editor.can_undo());
        let mut d = draft(&s);
        d.input(3, "123456").unwrap();
        d.error = "Invalid typed input".into();
        d.input_error = Some(3);
        s.gradient_editor = Some(d);
        s.accept_gradient_editor();
        assert!(s.gradient_editor.is_some());
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        s.gradient_editor
            .as_mut()
            .unwrap()
            .input(3, "654321")
            .unwrap();
        s.accept_gradient_editor();
        assert!(s.editor.can_undo());
    }
    #[test]
    fn modal_preserves_unedited_tracks_paint_properties_and_other_nodes() {
        let mut s = scene(true);
        animate(&mut s, GradientParam::EndX);
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    item: 5,
                    parameter: ContentsParam::Gradient(GradientParam::EndX),
                    edit: TrackEdit::Value {
                        frame: 60,
                        value: 500.,
                    },
                },
            })
            .unwrap();
        s.frame = 30;
        let before = s.editor.project().clone();
        let mut d = draft(&s);
        d.input(3, "2468AC").unwrap();
        d.add(true, 75.).unwrap();
        let expected = d.draft.project().clone();
        let Content::ShapeContents(original) = before.composition().layer(1).unwrap().content()
        else {
            panic!()
        };
        let Content::ShapeContents(after) = expected.composition().layer(1).unwrap().content()
        else {
            panic!()
        };
        for id in [2, 3, 4] {
            assert_eq!(original.node(id), after.node(id));
        }
        assert_eq!(
            original.node(5).unwrap().parameters[&ContentsParam::Gradient(GradientParam::EndX)],
            after.node(5).unwrap().parameters[&ContentsParam::Gradient(GradientParam::EndX)]
        );
        s.gradient_editor = Some(d);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &expected);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
    }
    #[test]
    fn modal_old_field_callbacks_cannot_change_a_reopened_session() {
        let mut s = scene(false);
        let old = draft(&s).id;
        let fresh = draft(&s);
        let id = fresh.id;
        let before = fresh.draft.project().clone();
        s.gradient_editor = Some(fresh);
        s.gradient_input(old, 3, "ABCDEF");
        assert_eq!(s.gradient_editor.as_ref().unwrap().draft.project(), &before);
        s.gradient_input(id, 3, "ABCDEF");
        assert_ne!(s.gradient_editor.as_ref().unwrap().draft.project(), &before);
        s.gradient_editor = None;
        s.gradient_input(id, 3, "123456");
        assert_eq!(s.editor.project(), &before);
    }
    #[test]
    fn modal_animated_edit_then_restore_is_noop_and_preserves_redo_and_exact_tracks() {
        for keyed_here in [false, true] {
            for parameter in [
                GradientParam::ColorPosition(1),
                GradientParam::ColorMidpoint(1),
                GradientParam::Opacity(3),
                GradientParam::Red(1),
            ] {
                let mut s = scene(false);
                animate(&mut s, parameter);
                for (frame, value) in [(0, 12.6), (60, 26.4)] {
                    s.editor
                        .execute(Command::Contents {
                            id: 1,
                            edit: ContentsEdit::Track {
                                item: 5,
                                parameter: ContentsParam::Gradient(parameter),
                                edit: TrackEdit::Value { frame, value },
                            },
                        })
                        .unwrap();
                }
                s.editor
                    .execute(Command::Contents {
                        id: 1,
                        edit: ContentsEdit::Track {
                            item: 5,
                            parameter: ContentsParam::Gradient(parameter),
                            edit: TrackEdit::Interpolate {
                                frame: 0,
                                interpolation: libre_effects_core::Interpolation::Smooth,
                            },
                        },
                    })
                    .unwrap();
                if keyed_here {
                    s.editor
                        .execute(Command::Contents {
                            id: 1,
                            edit: ContentsEdit::Track {
                                item: 5,
                                parameter: ContentsParam::Gradient(parameter),
                                edit: TrackEdit::Value {
                                    frame: 30,
                                    value: 18.6,
                                },
                            },
                        })
                        .unwrap();
                }
                s.frame = 30;
                s.editor.clear_history();
                s.editor
                    .execute(Command::SetValue {
                        id: 1,
                        property: Property::PositionX,
                        frame: 0,
                        value: 30.,
                    })
                    .unwrap();
                s.editor.undo();
                let original = s.editor.project().clone();
                let mut d = draft(&s);
                let initial = d.value(parameter);
                d.set_value(parameter, 40.).unwrap();
                if matches!(parameter, GradientParam::Red(_)) {
                    d.input(4, &initial.round().to_string()).unwrap();
                } else {
                    d.set_value(parameter, initial).unwrap();
                }
                assert_eq!(d.preview(&s).unwrap(), original);
                assert!(d.command().unwrap().is_none());
                s.gradient_editor = Some(d);
                s.accept_gradient_editor();
                assert_eq!(s.editor.project(), &original);
                assert!(!s.editor.can_undo());
                assert!(s.editor.can_redo());
            }
        }
    }
    #[test]
    fn modal_normalization_preserves_added_stop_sampling_and_restores_original_tracks() {
        let mut s = scene(false);
        for p in [
            GradientParam::Red(1),
            GradientParam::Green(1),
            GradientParam::Blue(1),
            GradientParam::ColorPosition(1),
            GradientParam::ColorMidpoint(1),
        ] {
            animate(&mut s, p);
        }
        s.frame = 30;
        s.editor.clear_history();
        let before = s.editor.project().clone();
        let mut d = draft(&s);
        let initial_color = d.field_value(3).unwrap();
        let initial_position = d.value(GradientParam::ColorPosition(1));
        let initial_midpoint = d.value(GradientParam::ColorMidpoint(1));
        d.input(3, "FF1234").unwrap();
        d.set_value(GradientParam::ColorPosition(1), 20.).unwrap();
        d.set_value(GradientParam::ColorMidpoint(1), 30.).unwrap();
        d.add(false, 45.).unwrap();
        let added = d.selected.stop().unwrap();
        let sampled = d
            .node()
            .kind
            .gradient()
            .unwrap()
            .color_at(d.node(), added, 30)
            .unwrap();
        d.selected = GradientParam::ColorPosition(1);
        d.input(3, &initial_color).unwrap();
        d.set_value(GradientParam::ColorPosition(1), initial_position)
            .unwrap();
        d.set_value(GradientParam::ColorMidpoint(1), initial_midpoint)
            .unwrap();
        let preview = d.preview(&s).unwrap();
        let Content::ShapeContents(contents) = preview.composition().layer(1).unwrap().content()
        else {
            panic!()
        };
        for p in [
            GradientParam::Red(1),
            GradientParam::Green(1),
            GradientParam::Blue(1),
            GradientParam::ColorPosition(1),
            GradientParam::ColorMidpoint(1),
        ] {
            assert_eq!(
                contents.node(5).unwrap().parameters[&ContentsParam::Gradient(p)],
                d.original_node().parameters[&ContentsParam::Gradient(p)]
            );
        }
        let node = contents.node(5).unwrap();
        assert_eq!(
            node.kind
                .gradient()
                .unwrap()
                .color_at(node, added, 30)
                .unwrap(),
            sampled
        );
        s.gradient_editor = Some(d);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &preview);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
    #[test]
    fn modal_add_then_remove_is_noop_including_stop_id_allocator() {
        let mut s = scene(false);
        let before = s.editor.project().clone();
        let mut d = draft(&s);
        for _ in 0..4 {
            for opacity in [false, true] {
                d.add(opacity, 45.).unwrap();
                d.remove().unwrap();
            }
        }
        assert_ne!(d.draft.project(), &before, "Temporary allocator advanced");
        assert_eq!(d.preview(&s).unwrap(), before);
        assert!(d.command().unwrap().is_none());
        s.gradient_editor = Some(d);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
    #[test]
    fn modal_rejected_input_can_be_corrected_or_reverted_without_losing_prior_edits() {
        for revert in [false, true] {
            let mut s = scene(false);
            let mut d = draft(&s);
            d.add(false, 70.).unwrap();
            let id = d.id;
            let good = d.field_value(0).unwrap();
            let expected = d.preview(&s).unwrap();
            s.gradient_editor = Some(d);
            s.gradient_input(id, 0, "invalid");
            s.accept_gradient_editor();
            assert!(s.gradient_editor.is_some());
            assert_eq!(s.gradient_editor.as_ref().unwrap().input_error, Some(0));
            if revert {
                assert_eq!(s.revert_gradient_field(id, 0), Some(good));
            } else {
                s.gradient_input(id, 0, &good);
            }
            assert_eq!(s.gradient_editor.as_ref().unwrap().input_error, None);
            s.accept_gradient_editor();
            assert_eq!(s.editor.project(), &expected);
            assert!(s.gradient_editor.is_none());
        }
    }
    #[test]
    fn modal_rgb_scrub_accepts_integral_decimal_values_and_rejects_invalid_channels() {
        let s = scene(false);
        let mut d = draft(&s);
        d.input(4, "12.00").unwrap();
        d.input(5, "34.0").unwrap();
        d.input(6, "255.00").unwrap();
        assert_eq!(d.field_value(3).as_deref(), Some("0C22FF"));
        let before = d.draft.project().clone();
        for text in ["12.5", "-1.0", "256.0", "NaN", "inf"] {
            assert!(d.input(4, text).is_err());
            assert_eq!(d.draft.project(), &before);
        }
    }
    #[test]
    fn modal_selection_discards_rejected_text_before_switching_stop_or_row() {
        let mut s = scene(false);
        let mut d = draft(&s);
        d.input(3, "123456").unwrap();
        let expected = d.preview(&s).unwrap();
        let id = d.id;
        s.gradient_editor = Some(d);
        s.gradient_input(id, 3, "invalid");
        s.gradient_editor
            .as_mut()
            .unwrap()
            .select(GradientParam::OpacityPosition(3))
            .unwrap();
        assert_eq!(s.gradient_editor.as_ref().unwrap().input_error, None);
        s.gradient_input(id, 0, "invalid");
        s.gradient_editor
            .as_mut()
            .unwrap()
            .select(GradientParam::ColorMidpoint(1))
            .unwrap();
        assert_eq!(s.gradient_editor.as_ref().unwrap().input_error, None);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &expected);
    }
    #[test]
    fn modal_nested_paint_in_nonroot_composition_leaves_other_paints_and_composition_unchanged() {
        let mut s = scene(false);
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 1,
                    kind: ContentsKind::Group(vec![]),
                },
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 6,
                    kind: ContentsKind::GradientStroke {
                        style: Default::default(),
                        gradient: Default::default(),
                    },
                },
            })
            .unwrap();
        s.editor.execute(Command::DuplicateComposition).unwrap();
        let composition = s.editor.project().active_composition_id();
        assert_ne!(composition, 1);
        let layer = s.editor.project().composition().layers()[0].id();
        s.editor.select(layer);
        let Content::ShapeContents(contents) = s.editor.selected_layer().unwrap().content() else {
            panic!()
        };
        let item = contents
            .rows()
            .into_iter()
            .find(|(_, _, node)| matches!(node.kind, ContentsKind::GradientStroke { .. }))
            .unwrap()
            .2
            .id;
        s.contents_selection = Some((composition, layer, item));
        s.editor.clear_history();
        let before = s.editor.project().clone();
        let mut d = Session::new(&s, item).unwrap();
        d.input(3, "468ACE").unwrap();
        d.add(true, 80.).unwrap();
        d.input(2, "35").unwrap();
        let preview = d.preview(&s).unwrap();
        let Content::ShapeContents(prior) = before.composition().layer(layer).unwrap().content()
        else {
            panic!()
        };
        let Content::ShapeContents(after) = preview.composition().layer(layer).unwrap().content()
        else {
            panic!()
        };
        for (_, _, node) in prior.rows() {
            if node.id != item && !matches!(node.kind, ContentsKind::Group(_)) {
                assert_eq!(after.node(node.id), Some(node));
            }
        }
        assert_eq!(
            before.compositions().into_iter().find(|(id, _)| *id == 1),
            preview.compositions().into_iter().find(|(id, _)| *id == 1)
        );
        s.gradient_editor = Some(d);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &preview);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
}

#[cfg(test)]
mod compound_tests {
    use super::*;
    use libre_effects_core::ContentsKind;
    fn scene() -> EditorState {
        let mut s = EditorState::default();
        s.editor
            .execute(Command::AddContent {
                content: Content::ShapeContents(Default::default()),
                width: 200.,
                height: 120.,
                name: "Compound".into(),
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 0,
                    kind: ContentsKind::GradientFill {
                        even_odd: false,
                        gradient: Default::default(),
                    },
                },
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientColors {
                    item: 1,
                    edit: GradientColorsEdit::SetAnimation {
                        frame: 0,
                        enabled: true,
                    },
                },
            })
            .unwrap();
        s.frame = 30;
        s.contents_selection = Some((s.editor.project().active_composition_id(), 1, 1));
        s.editor.clear_history();
        s
    }
    fn node(project: &Project) -> &ContentsNode {
        let Content::ShapeContents(contents) = project.composition().layer(1).unwrap().content()
        else {
            panic!()
        };
        contents.node(1).unwrap()
    }
    #[test]
    fn compound_modal_samples_to_private_static_draft_and_commits_one_complete_key() {
        let mut s = scene();
        let before = s.editor.project().clone();
        let mut draft = Session::new(&s, 1).unwrap();
        assert!(draft.compound);
        assert!(
            draft
                .node()
                .kind
                .gradient()
                .unwrap()
                .colors_animation()
                .is_none()
        );
        draft.input(3, "12EF34").unwrap();
        draft.add(false, 25.).unwrap();
        draft.input(3, "FEDCBA").unwrap();
        draft.add(true, 65.).unwrap();
        draft.input(2, "40").unwrap();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        let preview = draft.preview(&s).unwrap();
        assert!(matches!(
            draft.command().unwrap(),
            Some(Command::Contents {
                edit: ContentsEdit::GradientColors {
                    edit: GradientColorsEdit::Set { frame: 30, .. },
                    ..
                },
                ..
            })
        ));
        let original = node(&before);
        let changed = node(&preview);
        assert_eq!(
            original.parameters, changed.parameters,
            "legacy base stop tracks stay untouched"
        );
        let keys = changed
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys();
        assert_eq!(keys.keys().copied().collect::<Vec<_>>(), vec![0, 30]);
        assert_eq!(keys[&30].colors.len(), 3);
        assert_eq!(keys[&30].opacities.len(), 3);
        assert_eq!(keys[&30].colors[0].red, 18.);
        assert_eq!(
            original.kind.gradient().unwrap().colors_at(original, 29),
            changed.kind.gradient().unwrap().colors_at(changed, 29)
        );
        s.gradient_editor = Some(draft);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &preview);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        s.editor.redo();
        assert_eq!(s.editor.project(), &preview);
    }
    #[test]
    fn compound_modal_noop_away_back_and_added_removed_stop_preserve_redo() {
        for mode in 0..3 {
            let mut s = scene();
            s.editor
                .execute(Command::Contents {
                    id: 1,
                    edit: ContentsEdit::Rename {
                        item: 1,
                        name: "Temporary".into(),
                    },
                })
                .unwrap();
            s.editor.undo();
            let before = s.editor.project().clone();
            let mut draft = Session::new(&s, 1).unwrap();
            if mode == 1 {
                draft
                    .set_value(GradientParam::ColorPosition(1), 25.)
                    .unwrap();
                draft
                    .set_value(GradientParam::ColorPosition(1), 0.)
                    .unwrap();
            } else if mode == 2 {
                draft.add(false, 25.).unwrap();
                draft.remove().unwrap();
                draft.add(true, 65.).unwrap();
                draft.remove().unwrap();
            }
            assert!(draft.command().unwrap().is_none());
            assert_eq!(draft.preview(&s).unwrap(), before);
            s.gradient_editor = Some(draft);
            s.accept_gradient_editor();
            assert_eq!(s.editor.project(), &before);
            assert!(s.editor.can_redo());
            assert!(!s.editor.can_undo());
        }
    }
    #[test]
    fn compound_modal_tiny_value_and_existing_key_replacement_are_exact() {
        let mut s = scene();
        let mut draft = Session::new(&s, 1).unwrap();
        draft
            .set_value(GradientParam::ColorPosition(1), 1e-10)
            .unwrap();
        assert!(draft.command().unwrap().is_some());
        s.gradient_editor = Some(draft);
        s.accept_gradient_editor();
        let mut draft = Session::new(&s, 1).unwrap();
        draft.input(3, "123456").unwrap();
        let before = s.editor.project().clone();
        let preview = draft.preview(&s).unwrap();
        let before_keys = node(&before)
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys();
        let after_keys = node(&preview)
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap()
            .keys();
        assert_eq!(
            before_keys.keys().collect::<Vec<_>>(),
            after_keys.keys().collect::<Vec<_>>()
        );
        assert_eq!(before_keys[&0], after_keys[&0]);
        assert_eq!(after_keys[&30].colors[0].position, 1e-10);
    }
    #[test]
    fn compound_modal_invalid_and_stale_input_never_retargets_the_snapshot() {
        let mut s = scene();
        let mut draft = Session::new(&s, 1).unwrap();
        let original = s.editor.project().clone();
        for value in [f64::NAN, f64::INFINITY, -1., 101.] {
            assert!(
                draft
                    .set_value(GradientParam::ColorPosition(1), value)
                    .is_err()
            );
        }
        assert!(draft.command().unwrap().is_none());
        draft.input(3, "123456").unwrap();
        s.gradient_editor = Some(draft);
        s.frame = 31;
        s.accept_gradient_editor();
        assert!(s.gradient_editor.is_none());
        assert_eq!(s.editor.project(), &original);
        assert!(!s.editor.can_undo());
    }
    #[test]
    fn compound_modal_field_callbacks_bind_stop_selection_generation_and_restore_invalid_text() {
        let mut s = scene();
        let draft = Session::new(&s, 1).unwrap();
        let (id, generation) = (draft.id, draft.selection_generation);
        s.gradient_editor = Some(draft);
        assert_eq!(
            s.gradient_compound_input(id, generation, 1, 0, "0.0000000001"),
            "0.0000000001"
        );
        let preview = s.gradient_editor.as_ref().unwrap().preview(&s).unwrap();
        assert_eq!(
            s.gradient_compound_input(id, generation, 1, 0, "NaN"),
            "0.0000000001"
        );
        assert_eq!(s.gradient_editor.as_ref().unwrap().input_error, Some(0));
        s.accept_gradient_editor();
        assert!(s.gradient_editor.is_some());
        s.gradient_editor
            .as_mut()
            .unwrap()
            .select(GradientParam::ColorPosition(2))
            .unwrap();
        s.gradient_editor
            .as_mut()
            .unwrap()
            .select(GradientParam::ColorPosition(1))
            .unwrap();
        assert_eq!(
            s.gradient_compound_input(id, generation, 1, 0, "20"),
            "0.0000000001"
        );
        assert_eq!(
            s.gradient_editor.as_ref().unwrap().preview(&s).unwrap(),
            preview
        );
        let fresh = s.gradient_editor.as_ref().unwrap().selection_generation;
        assert_eq!(s.gradient_compound_input(id, fresh, 1, 0, "20"), "20");
        s.accept_gradient_editor();
        assert!(s.gradient_editor.is_none());
        let node = node(s.editor.project());
        assert_eq!(
            node.kind.gradient().unwrap().colors_at(node, 30).colors[0].position,
            20.
        );
    }
    #[test]
    fn compound_modal_fractional_rgb_display_roundtrip_retains_raw_values_and_redo() {
        let mut s = scene();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientColors {
                    item: 1,
                    edit: GradientColorsEdit::Value {
                        frame: 30,
                        parameter: GradientParam::Red(1),
                        value: 127.5,
                    },
                },
            })
            .unwrap();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Rename {
                    item: 1,
                    name: "Temporary".into(),
                },
            })
            .unwrap();
        s.editor.undo();
        let before = s.editor.project().clone();
        let mut draft = Session::new(&s, 1).unwrap();
        assert_eq!(draft.field_value(4).as_deref(), Some("128"));
        draft.input(4, "12").unwrap();
        draft.input(4, "128").unwrap();
        assert!(draft.command().unwrap().is_none());
        assert_eq!(draft.preview(&s).unwrap(), before);
        s.gradient_editor = Some(draft);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &before);
        assert!(s.editor.can_redo());
    }
    #[test]
    fn compound_modal_signed_zero_location_preserves_bits_without_intermediate_source_edit() {
        let mut s = scene();
        s.editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::GradientColors {
                    item: 1,
                    edit: GradientColorsEdit::Value {
                        frame: 30,
                        parameter: GradientParam::ColorPosition(2),
                        value: 0.,
                    },
                },
            })
            .unwrap();
        s.editor.clear_history();
        let before = s.editor.project().clone();
        let mut draft = Session::new(&s, 1).unwrap();
        draft.select(GradientParam::ColorPosition(2)).unwrap();
        draft
            .set_value(GradientParam::ColorPosition(2), -0.)
            .unwrap();
        assert_eq!(
            draft.value(GradientParam::ColorPosition(2)).to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
        assert!(matches!(
            draft.command().unwrap(),
            Some(Command::Contents {
                edit: ContentsEdit::GradientColors {
                    edit: GradientColorsEdit::Set { .. },
                    ..
                },
                ..
            })
        ));
        let preview = draft.preview(&s).unwrap();
        let current = node(&preview);
        assert_eq!(
            current
                .kind
                .gradient()
                .unwrap()
                .colors_at(current, 30)
                .colors[1]
                .position
                .to_bits(),
            (-0.0f64).to_bits()
        );
        s.gradient_editor = Some(draft);
        s.accept_gradient_editor();
        assert_eq!(s.editor.project(), &preview);
        s.editor.undo();
        assert_eq!(s.editor.project(), &before);
        assert!(!s.editor.can_undo());
    }
    #[test]
    fn compound_modal_failed_private_bridge_never_exposes_intermediate_draft() {
        let s = scene();
        let mut draft = Session::new(&s, 1).unwrap();
        let before = draft.draft.project().clone();
        let command = |value| Command::Contents {
            id: 1,
            edit: ContentsEdit::Track {
                item: 1,
                parameter: ContentsParam::Gradient(GradientParam::ColorPosition(1)),
                edit: TrackEdit::Value { frame: 30, value },
            },
        };
        assert!(
            draft
                .apply_draft_sequence(vec![command(1.), command(101.)])
                .is_err()
        );
        assert_eq!(draft.draft.project(), &before);
        assert!(draft.commands.is_empty());
        assert!(draft.command().unwrap().is_none());
        assert_eq!(draft.preview(&s).unwrap(), *s.editor.project());
    }
}
