//! Receipts for a displayed preview and its detached expression geometry.
//! No JavaScript executes here. Pixels and geometry cross the worker boundary together.
use libre_effects_core::{Composition, Content, Layer, LayerId, Project};

/// The normal hidden-matte selection affordance is safe only when its transform
/// chain is authored. A hidden consumer may leave those expressions outside the
/// rendered roots; never invent its controls from unevaluated base values.
pub fn controls_active(comp: &Composition, layer: &Layer, frame: u32, selected: bool) -> bool {
    !matches!(layer.content(), Content::Audio { .. })
        && (comp.layer_active(layer, frame, true)
            || (selected
                && !comp.has_expression_transform(layer.id())
                && frame >= layer.in_frame()
                && frame < layer.out_frame(comp.duration())
                && comp.layers().iter().any(|consumer| {
                    consumer
                        .track_matte()
                        .is_some_and(|matte| matte.source == layer.id())
                })))
}

/// Selection can include hidden controls which were intentionally not evaluated.
/// Keep coordinate-writing gestures blocked even if the rendered roots are empty.
pub fn selection_has_expression_transform(
    comp: &Composition,
    selected: impl IntoIterator<Item = LayerId>,
) -> bool {
    selected
        .into_iter()
        .any(|id| comp.has_expression_transform(id))
}

pub type GradientContext = (u64, u32, u32, u64, u64, u64, u64);

#[derive(Clone, Debug)]
pub struct PreviewRequest {
    pub project: Project,
    pub frame: u32,
    pub dimension: u32,
    pub revision: u64,
    pub document_revision: u64,
    pub core_generation: u64,
    pub transport: u64,
    pub gradient_gesture: Option<u64>,
}
impl PreviewRequest {
    pub fn gradient_context(&self) -> Option<GradientContext> {
        self.gradient_gesture.map(|id| {
            (
                id,
                self.frame,
                self.dimension,
                self.revision,
                self.document_revision,
                self.core_generation,
                self.transport,
            )
        })
    }
    pub fn same_gradient(&self, other: &Self) -> bool {
        self.gradient_context().is_some() && self.gradient_context() == other.gradient_context()
    }
    pub fn discards_gradient_frame(
        &self,
        previous: Option<GradientContext>,
        displayed: Option<(&Project, u32, u32)>,
    ) -> bool {
        previous.is_some()
            && previous != self.gradient_context()
            && displayed.is_some_and(|(p, f, d)| {
                p != &self.project || f != self.frame || d != self.dimension
            })
    }
    pub fn same_context(&self, other: &Self) -> bool {
        self.project == other.project
            && self.gradient_gesture == other.gradient_gesture
            && self.dimension == other.dimension
            && self.revision == other.revision
            && self.document_revision == other.document_revision
            && self.core_generation == other.core_generation
            && self.transport == other.transport
    }
    pub fn accepts(&self, ready: &Self, _playing: bool) -> bool {
        self.same_context(ready) && self.frame == ready.frame
    }
    /// Errors conservatively require a worker result, never an authored fallback.
    pub fn needs_evaluated_view(&self) -> bool {
        self.project
            .expression_roots(self.project.active_composition_id(), self.frame, true)
            .map_or(true, |roots| !roots.is_empty())
    }
    pub fn validate_evaluated_view(&self, view: Option<&Project>) -> Result<(), String> {
        match view {
            Some(view)
                if libre_effects_core::CompositionSample::from_frame(
                    self.frame,
                    self.project.composition().fps(),
                )
                .is_ok_and(|sample| {
                    view.evaluated_at_sample(self.project.active_composition_id(), sample)
                }) && view.active_composition_id() == self.project.active_composition_id() =>
            {
                Ok(())
            }
            Some(_) => Err("Expression preview geometry does not match the rendered frame".into()),
            None if self.needs_evaluated_view() => {
                Err("Expression preview is missing evaluated geometry".into())
            }
            None => Ok(()),
        }
    }
    /// Exact receipts are required for playback, editing and selection.
    pub fn current_geometry<'a>(
        &self,
        displayed: &'a Self,
        view: Option<&'a Project>,
    ) -> Option<&'a Project> {
        (self.accepts(displayed, false) && displayed.validate_evaluated_view(view).is_ok())
            .then(|| view.unwrap_or(&displayed.project))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{
        Command, Editor, ExpressionTarget, Property, expression_runtime as ae,
    };

    fn request() -> PreviewRequest {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::Position,
                source: "[value[0] + time * 30 + 100, value[1] + 50]".into(),
                enabled: true,
            })
            .unwrap();
        PreviewRequest {
            project: editor.project().clone(),
            frame: 12,
            dimension: 640,
            revision: 4,
            document_revision: 8,
            core_generation: 6,
            transport: 3,
            gradient_gesture: None,
        }
    }
    fn evaluated(request: &PreviewRequest) -> Project {
        let comp = request.project.active_composition_id();
        let snapshot = request
            .project
            .expression_snapshot(comp, request.frame)
            .unwrap();
        let roots = request
            .project
            .expression_roots(comp, request.frame, true)
            .unwrap();
        let values = ae::ExpressionEvaluator::default()
            .evaluate(&snapshot, &roots)
            .unwrap();
        request
            .project
            .with_evaluated_properties(comp, request.frame, true, &values)
            .unwrap()
    }
    #[test]
    fn expression_geometry_uses_render_view_without_modifying_the_authored_project() {
        let request = request();
        let before = request.project.to_json().unwrap();
        let view = evaluated(&request);
        let source = request.project.composition().layer(1).unwrap();
        let geometry = request.current_geometry(&request, Some(&view)).unwrap();
        let layer = geometry.composition().layer(1).unwrap();
        assert_eq!(
            layer
                .property(Property::PositionX)
                .expect("known scalar fixture property")
                .value_at(12),
            source
                .property(Property::PositionX)
                .expect("known scalar fixture property")
                .value_at(12)
                + 112.0
        );
        assert_eq!(
            layer
                .property(Property::PositionY)
                .expect("known scalar fixture property")
                .value_at(12),
            source
                .property(Property::PositionY)
                .expect("known scalar fixture property")
                .value_at(12)
                + 50.0
        );
        let authored_world = request
            .project
            .composition()
            .world_transform(1, 12)
            .unwrap()
            .point([0., 0.]);
        let evaluated_world = geometry
            .composition()
            .world_transform(1, 12)
            .unwrap()
            .point([0., 0.]);
        assert_eq!(
            evaluated_world,
            [authored_world[0] + 112., authored_world[1] + 50.]
        );
        assert_eq!(request.project.to_json().unwrap(), before);
        assert!(view.to_json().is_err());
    }
    #[test]
    fn missing_stale_and_authored_views_fail_closed() {
        let current = request();
        let view = evaluated(&current);
        assert!(current.current_geometry(&current, None).is_none());
        assert!(
            current
                .validate_evaluated_view(Some(&current.project))
                .is_err()
        );
        for change in [
            |r: &mut PreviewRequest| r.frame += 1,
            |r: &mut PreviewRequest| r.dimension /= 2,
            |r: &mut PreviewRequest| r.revision += 1,
            |r: &mut PreviewRequest| r.document_revision += 1,
            |r: &mut PreviewRequest| r.core_generation += 1,
            |r: &mut PreviewRequest| r.transport += 1,
            |r: &mut PreviewRequest| r.gradient_gesture = Some(1),
        ] {
            let mut next = current.clone();
            change(&mut next);
            assert!(next.current_geometry(&current, Some(&view)).is_none());
        }
        let mut next = current.clone();
        next.frame += 1;
        assert!(!next.accepts(&current, true));
        assert!(next.validate_evaluated_view(Some(&view)).is_err());
        let mut editor = Editor::default();
        editor.replace_project(current.project.clone()).unwrap();
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame: 12,
                value: 100.,
            })
            .unwrap();
        next = current.clone();
        next.project = editor.project().clone();
        assert!(next.current_geometry(&current, Some(&view)).is_none());
    }
    #[test]
    fn ordinary_pixel_cache_hits_can_use_authored_geometry() {
        let mut current = request();
        let mut editor = Editor::default();
        editor.replace_project(current.project.clone()).unwrap();
        editor
            .execute(Command::SetExpressionEnabled {
                id: 1,
                target: ExpressionTarget::Position,
                enabled: false,
            })
            .unwrap();
        current.project = editor.project().clone();
        assert!(!current.needs_evaluated_view());
        assert!(current.current_geometry(&current, None).is_some());
    }
    #[test]
    fn selected_hidden_matte_controls_never_fall_back_to_expression_authored_transforms() {
        use libre_effects_core::{MatteMode, TrackMatte};
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetTrackMatte {
                id: 2,
                matte: Some(TrackMatte {
                    source: 1,
                    mode: MatteMode::Alpha,
                }),
            })
            .unwrap();
        // A visible consumer still keeps the raw, non-expression matte controls.
        let comp = editor.project().composition();
        assert!(!comp.layer_active(comp.layer(1).unwrap(), 0, true));
        assert!(controls_active(comp, comp.layer(1).unwrap(), 0, true));
        assert!(!controls_active(comp, comp.layer(1).unwrap(), 0, false));
        assert!(!selection_has_expression_transform(comp, [1]));
        editor.execute(Command::ToggleVisible(2)).unwrap();
        // Hiding the consumer does not remove the existing authored affordance.
        let comp = editor.project().composition();
        assert!(controls_active(comp, comp.layer(1).unwrap(), 0, true));
        editor
            .execute(Command::SetExpression {
                id: 1,
                target: ExpressionTarget::Position,
                source: "[600, 400]".into(),
                enabled: true,
            })
            .unwrap();
        assert!(
            editor
                .project()
                .expression_roots(1, 0, true)
                .unwrap()
                .is_empty()
        );
        let comp = editor.project().composition();
        assert!(selection_has_expression_transform(comp, [1]));
        assert!(!controls_active(comp, comp.layer(1).unwrap(), 0, true));
        // Even when the consumer becomes visible and evaluates the matte, this
        // conservative fallback policy does not silently expose base controls.
        editor.execute(Command::ToggleVisible(2)).unwrap();
        let comp = editor.project().composition();
        assert!(!controls_active(comp, comp.layer(1).unwrap(), 0, true));
        editor
            .execute(Command::SetExpressionEnabled {
                id: 1,
                target: ExpressionTarget::Position,
                enabled: false,
            })
            .unwrap();
        let comp = editor.project().composition();
        assert!(controls_active(comp, comp.layer(1).unwrap(), 0, true));
        assert!(!selection_has_expression_transform(comp, [1]));

        // A hidden ancestor's expression must apply the same guard as the matte's own.
        editor.execute(Command::ToggleVisible(2)).unwrap();
        editor.execute(Command::AddNull).unwrap();
        editor.execute(Command::ToggleVisible(3)).unwrap();
        editor
            .execute(Command::SetParent {
                id: 1,
                parent: Some(3),
                frame: 0,
            })
            .unwrap();
        editor
            .execute(Command::SetExpression {
                id: 3,
                target: ExpressionTarget::Scale,
                source: "[150, 150]".into(),
                enabled: true,
            })
            .unwrap();
        assert!(
            editor
                .project()
                .expression_roots(1, 0, true)
                .unwrap()
                .is_empty()
        );
        let comp = editor.project().composition();
        assert!(selection_has_expression_transform(comp, [1]));
        assert!(!controls_active(comp, comp.layer(1).unwrap(), 0, true));
    }
}
