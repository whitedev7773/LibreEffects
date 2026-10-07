//! Persisted numeric programs and pure, frame-scoped worker boundaries.
//!
//! Core never executes JavaScript. Numeric edits always edit authored tracks:
//! an enabled expression receives their pre-expression sample as `value`.
use super::*;
use libre_effects_ae_expressions as ae;

pub const MAX_EXPRESSION_SOURCE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExpressionTarget {
    Position,
    Scale,
    Opacity,
    /// Stable effect identity; names are resolved only when constructing a snapshot.
    Slider(EffectId),
    SourceText,
    MaskPath(u64),
}

impl ExpressionTarget {
    /// Position and Scale are vector expressions shared by their component tracks.
    pub fn from_property(property: Property) -> Option<Self> {
        match property {
            Property::PositionX | Property::PositionY => Some(Self::Position),
            Property::ScaleX | Property::ScaleY => Some(Self::Scale),
            Property::Opacity => Some(Self::Opacity),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericExpression {
    pub target: ExpressionTarget,
    pub source: String,
    pub enabled: bool,
    /// Explicit compatibility locals; source bytes remain unchanged and strict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_bindings: Vec<String>,
}

impl Layer {
    pub fn expressions(&self) -> &[NumericExpression] {
        &self.expressions
    }
    pub fn expression(&self, target: ExpressionTarget) -> Option<&NumericExpression> {
        self.expressions
            .iter()
            .find(|program| program.target == target)
    }
    pub fn expression_for(&self, target: ExpressionTarget) -> Option<&NumericExpression> {
        self.expression(target)
    }
    pub fn has_enabled_expression(&self, target: ExpressionTarget) -> bool {
        self.expression(target)
            .is_some_and(|program| program.enabled)
    }
    fn expression_value(
        &self,
        target: ExpressionTarget,
        frame: f64,
        seconds_per_frame: f64,
    ) -> Result<ae::PropertyValue, String> {
        use ae::PropertyValue;
        if target == ExpressionTarget::Opacity {
            return Ok(PropertyValue::Scalar(
                self.opacity_sample(frame, seconds_per_frame)?,
            ));
        }
        let value = |property| {
            self.property(property)
                .expect("admitted scalar expression composition")
                .sample(frame)
        };
        Ok(match target {
            ExpressionTarget::Position => {
                PropertyValue::Vector2(self.position2_sample(frame, seconds_per_frame)?)
            }
            ExpressionTarget::Scale => {
                PropertyValue::Vector2([value(Property::ScaleX), value(Property::ScaleY)])
            }
            ExpressionTarget::Opacity => unreachable!("Opacity uses its time-aware owner"),
            ExpressionTarget::SourceText => PropertyValue::Text(
                self.source_text_at(frame.floor() as Frame)
                    .ok_or("Source Text requires a text layer")?
                    .into(),
            ),
            ExpressionTarget::MaskPath(id) => PropertyValue::Path(path_value(
                &self
                    .path_masks
                    .iter()
                    .find(|mask| mask.id == id)
                    .ok_or("Expression mask no longer exists")?
                    .path_at(frame.floor() as Frame),
            )),
            ExpressionTarget::Slider(effect) => PropertyValue::Scalar(
                self.effect_stack
                    .iter()
                    .find(|item| item.id() == effect)
                    .expect("validated slider target")
                    .parameter(EffectParam::Amount)
                    .expect("validated slider amount")
                    .sample(frame)
                    .clamp(-1_000_000.0, 1_000_000.0),
            ),
        })
    }
    fn expression_sample<'a>(
        &'a self,
        target: ExpressionTarget,
        frame: f64,
        seconds_per_frame: f64,
        sources: &mut SourcePool<'a>,
    ) -> Result<ae::PropertySnapshot, String> {
        let expression = self
            .expression(target)
            .map(|program| {
                sources
                    .intern(&program.source)
                    .map(|source_id| ae::ExpressionProgram {
                        source_id,
                        enabled: program.enabled,
                        local_bindings: program.local_bindings.clone(),
                    })
            })
            .transpose()?;
        Ok(ae::PropertySnapshot {
            authored_value: self.expression_value(target, frame, seconds_per_frame)?,
            expression,
        })
    }
}

/// Borrow identities from the validated authored project; only distinct source
/// bytes are cloned into the transient worker snapshot. No authored interning or
/// source normalization changes saved documents or independent property contexts.
#[derive(Default)]
struct SourcePool<'a> {
    ids: BTreeMap<&'a str, ae::ExpressionSourceId>,
    sources: Vec<String>,
    bytes: usize,
}
impl<'a> SourcePool<'a> {
    fn intern(&mut self, source: &'a str) -> Result<ae::ExpressionSourceId, String> {
        if let Some(id) = self.ids.get(source) {
            return Ok(*id);
        }
        let bytes = self
            .bytes
            .checked_add(source.len())
            .ok_or("Expression source budget exceeded")?;
        if self.sources.len() >= ae::MAX_EXPRESSION_SOURCES
            || source.len() > ae::MAX_EXPRESSION_SOURCE_BYTES
            || bytes > ae::MAX_TOTAL_EXPRESSION_SOURCE_BYTES
        {
            return Err("Expression unique-source budget exceeded".into());
        }
        let id = ae::ExpressionSourceId(
            u32::try_from(self.sources.len()).map_err(|_| "Expression source count overflow")?,
        );
        self.sources.push(source.to_owned());
        self.ids.insert(source, id);
        self.bytes = bytes;
        Ok(id)
    }
}

impl Composition {
    /// Whether authored pointer geometry may differ from evaluated geometry.
    /// Hidden parents still affect transforms. An invalid or overly deep chain
    /// returns true so edit guards fail closed instead of using unsafe geometry.
    pub fn has_expression_transform(&self, layer_id: LayerId) -> bool {
        let mut current = Some(layer_id);
        let mut visited = BTreeSet::new();
        for _ in 0..=self.layers.len().min(1000) {
            let Some(id) = current else {
                return false;
            };
            if !visited.insert(id) {
                return true;
            }
            let Some(layer) = self.layer(id) else {
                return true;
            };
            if layer.has_enabled_expression(ExpressionTarget::Position)
                || layer.has_enabled_expression(ExpressionTarget::Scale)
            {
                return true;
            }
            current = layer.parent;
        }
        true
    }
}

fn target_exists(layer: &Layer, target: ExpressionTarget) -> bool {
    match target {
        ExpressionTarget::Slider(id) => layer
            .effect_stack
            .iter()
            .any(|effect| effect.id() == id && effect.kind() == EffectKind::SliderControl),
        ExpressionTarget::SourceText => {
            matches!(layer.content, Content::Text { .. })
                && layer.rich_text.as_ref().is_none_or(|rich| {
                    let first = rich
                        .runs
                        .first()
                        .map_or(&rich.default_style, |run| &run.style);
                    rich.runs.iter().all(|run| &run.style == first)
                })
        }
        ExpressionTarget::MaskPath(id) => layer.path_masks.iter().any(|mask| mask.id == id),
        _ => true,
    }
}

fn path_value(path: &VectorPath) -> ae::ExpressionPath {
    ae::ExpressionPath {
        vertices: path.vertices.iter().map(|v| v.position).collect(),
        in_tangents: path.vertices.iter().map(|v| v.incoming).collect(),
        out_tangents: path.vertices.iter().map(|v| v.outgoing).collect(),
        closed: path.closed,
    }
}

fn native_path(path: &ae::ExpressionPath) -> VectorPath {
    VectorPath {
        vertices: path
            .vertices
            .iter()
            .zip(&path.in_tangents)
            .zip(&path.out_tangents)
            .map(|((position, incoming), outgoing)| PathVertex {
                position: *position,
                incoming: *incoming,
                outgoing: *outgoing,
            })
            .collect(),
        closed: path.closed,
    }
}

pub(super) fn requires_playbar_version(project: &Project) -> bool {
    project.compositions().into_iter().any(|(_, comp)| {
        comp.layers.iter().any(|layer| {
            layer.expressions.iter().any(|program| {
                !program.local_bindings.is_empty()
                    || matches!(
                        program.target,
                        ExpressionTarget::SourceText | ExpressionTarget::MaskPath(_)
                    )
            })
        })
    })
}

pub(super) fn materialized(project: &Project) -> bool {
    project.compositions().into_iter().any(|(_, comp)| {
        comp.layers.iter().any(|layer| {
            !layer.expressions.is_empty()
                || layer
                    .effect_stack
                    .iter()
                    .any(|effect| effect.kind() == EffectKind::SliderControl)
        })
    })
}

pub(super) fn validate(layer: &Layer, version: u32) -> Result<(), String> {
    if version < 65
        && (!layer.expressions.is_empty()
            || layer
                .effect_stack
                .iter()
                .any(|effect| effect.kind() == EffectKind::SliderControl))
    {
        return Err("Numeric expressions and Slider Control require project version 65".into());
    }
    if version < 76
        && layer.expressions.iter().any(|program| {
            !program.local_bindings.is_empty()
                || matches!(
                    program.target,
                    ExpressionTarget::SourceText | ExpressionTarget::MaskPath(_)
                )
        })
    {
        return Err(
            "Text/path expressions and explicit local bindings require project version 76".into(),
        );
    }
    let mut targets = BTreeSet::new();
    if layer.expressions.len() > 132
        || layer.expressions.iter().any(|program| {
            !targets.insert(program.target)
                || !target_exists(layer, program.target)
                || program.source.is_empty()
                || program.source.len() > MAX_EXPRESSION_SOURCE_BYTES
                || program.source.contains('\0')
                || ae::validate_local_bindings(&program.local_bindings).is_err()
        })
    {
        return Err(
            "Expression targets must be unique and valid, with 1–16384 bytes of source and no NUL"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn slider_edit(layer: &Layer, edit: &EffectEdit) -> bool {
    let effect = match edit {
        EffectEdit::Add(kind) => return *kind == EffectKind::SliderControl,
        EffectEdit::Remove(effect)
        | EffectEdit::Duplicate(effect)
        | EffectEdit::Reset(effect)
        | EffectEdit::EditKeyframe { effect, .. }
        | EffectEdit::Move { effect, .. }
        | EffectEdit::Bypass { effect, .. }
        | EffectEdit::Rename { effect, .. }
        | EffectEdit::SetValue { effect, .. }
        | EffectEdit::ToggleAnimation { effect, .. }
        | EffectEdit::ToggleKey { effect, .. }
        | EffectEdit::Interpolate { effect, .. } => *effect,
        _ => return false,
    };
    layer
        .effect_stack
        .iter()
        .any(|item| item.id() == effect && item.kind() == EffectKind::SliderControl)
}

pub(super) fn edits_only(project: &Project, command: &Command) -> bool {
    match command {
        Command::SetExpression { .. }
        | Command::SetExpressionLocalBindings { .. }
        | Command::SetExpressionEnabled { .. }
        | Command::RemoveExpression { .. } => true,
        Command::Effect { id, edit } => project
            .composition
            .layer(*id)
            .is_some_and(|layer| slider_edit(layer, edit)),
        Command::EditTrack {
            id,
            property:
                PropertyPath::Effect {
                    effect,
                    parameter: EffectParam::Amount,
                },
            ..
        } => project
            .composition
            .layer(*id)
            .is_some_and(|layer| target_exists(layer, ExpressionTarget::Slider(*effect))),
        Command::Batch(commands) => {
            !commands.is_empty() && commands.iter().all(|command| edits_only(project, command))
        }
        _ => false,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let (id, target) = match command {
        Command::SetExpression { id, target, .. }
        | Command::SetExpressionLocalBindings { id, target, .. }
        | Command::SetExpressionEnabled { id, target, .. }
        | Command::RemoveExpression { id, target } => (*id, *target),
        _ => return None,
    };
    Some((|| {
        let layer = state
            .project
            .composition
            .layers
            .iter_mut()
            .find(|layer| layer.id == id)
            .ok_or("Layer not found")?;
        if layer.locked {
            return Err("Unlock the layer before editing expressions".into());
        }
        if !target_exists(layer, target) {
            return Err("Expression target must be an existing transform, Slider, closed mask, or uniformly styled Source Text".into());
        }
        match command {
            Command::SetExpression {
                source, enabled, ..
            } => {
                // AE assignment of the empty string removes the expression.
                if source.is_empty() {
                    layer.expressions.retain(|program| program.target != target);
                } else {
                    if source.len() > MAX_EXPRESSION_SOURCE_BYTES || source.contains('\0') {
                        return Err(
                            "Expression source must be at most 16384 bytes with no NUL".into()
                        );
                    }
                    let replacement = NumericExpression {
                        target,
                        source: source.clone(),
                        enabled: *enabled,
                        local_bindings: layer
                            .expression(target)
                            .map_or_else(Vec::new, |program| program.local_bindings.clone()),
                    };
                    if let Some(program) = layer
                        .expressions
                        .iter_mut()
                        .find(|program| program.target == target)
                    {
                        *program = replacement;
                    } else {
                        layer.expressions.push(replacement);
                    }
                }
            }
            Command::SetExpressionLocalBindings { bindings, .. } => {
                ae::validate_local_bindings(bindings)?;
                let program = layer
                    .expressions
                    .iter_mut()
                    .find(|program| program.target == target)
                    .ok_or("Assign an expression before setting its local bindings")?;
                program.local_bindings = bindings.clone();
            }
            Command::SetExpressionEnabled { enabled, .. } => {
                if let Some(program) = layer
                    .expressions
                    .iter_mut()
                    .find(|program| program.target == target)
                {
                    program.enabled = *enabled;
                } else if *enabled {
                    return Err("Assign an expression before enabling it".into());
                }
            }
            Command::RemoveExpression { .. } => {
                layer.expressions.retain(|program| program.target != target)
            }
            _ => unreachable!(),
        }
        Ok(())
    })())
}

pub(super) fn reject_transient_serialization<S>(
    _: &Option<(CompositionId, Frame)>,
    _: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    Err(serde::ser::Error::custom(
        "An evaluated render view cannot be serialized",
    ))
}

fn address(
    composition: CompositionId,
    layer: LayerId,
    property: ae::ExpressionProperty,
) -> ae::PropertyAddress {
    ae::PropertyAddress {
        composition: ae::CompositionId(composition),
        layer: ae::LayerId(layer),
        property,
    }
}

impl Project {
    fn expression_composition(
        &self,
        composition: CompositionId,
        frame: Frame,
    ) -> Result<&Composition, String> {
        if self.evaluated_frame.is_some() {
            return Err("Build expression snapshots from the authored project".into());
        }
        let comp = self
            .composition_by_id(composition)
            .ok_or("Composition not found")?;
        if frame >= comp.duration {
            return Err("Expression frame is outside the composition".into());
        }
        Ok(comp)
    }

    fn expression_composition_sample(
        &self,
        composition: CompositionId,
        sample: CompositionSample,
    ) -> Result<(&Composition, CompositionSample), String> {
        let comp = self
            .composition_by_id(composition)
            .ok_or("Composition not found")?;
        let sample = sample.in_rate(comp.fps)?;
        let frame = sample.floor_frame()?;
        let comp = self.expression_composition(composition, frame)?;
        Ok((comp, sample))
    }

    pub fn expression_roots_at_sample(
        &self,
        composition: CompositionId,
        sample: CompositionSample,
        include_guides: bool,
    ) -> Result<Vec<ae::PropertyAddress>, String> {
        let (comp, sample) = self.expression_composition_sample(composition, sample)?;
        render_sampling::admit(comp, sample)?;
        self.expression_roots(composition, sample.floor_frame()?, include_guides)
    }

    /// Pure pre-expression samples at an exact composition frame. Hidden layers
    /// remain resolvable dependencies. Effect names resolve first in stack order.
    pub fn expression_snapshot(
        &self,
        composition: CompositionId,
        frame: Frame,
    ) -> Result<ae::CompositionSnapshot, String> {
        let fps = self
            .composition_by_id(composition)
            .ok_or("Composition not found")?
            .fps;
        self.expression_snapshot_at_sample(composition, CompositionSample::from_frame(frame, fps)?)
    }

    pub fn expression_snapshot_at_sample(
        &self,
        composition: CompositionId,
        sample: CompositionSample,
    ) -> Result<ae::CompositionSnapshot, String> {
        self.validate()?;
        let (comp, sample) = self.expression_composition_sample(composition, sample)?;
        render_sampling::admit(comp, sample)?;
        let frame = sample.frame();
        if comp.has_spatial_layers() {
            return Err(
                "Expression evaluation in spatial compositions is not yet supported".into(),
            );
        }
        let rate = ae::FrameRate {
            numerator: comp.fps.numerator(),
            denominator: comp.fps.denominator(),
        };
        let mut sources = SourcePool::default();
        let layers = comp
            .layers
            .iter()
            .map(|layer| {
                let mut names = BTreeSet::new();
                let sliders = layer
                    .effect_stack
                    .iter()
                    .filter(|effect| {
                        effect.kind() == EffectKind::SliderControl && names.insert(effect.name())
                    })
                    .map(|effect| {
                        Ok(ae::SliderSnapshot {
                            name: effect.name().into(),
                            property: layer.expression_sample(
                                ExpressionTarget::Slider(effect.id()),
                                frame,
                                comp.fps.seconds(1),
                                &mut sources,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let mut markers = layer
                    .markers()
                    .iter()
                    .map(|marker| ae::MarkerSnapshot {
                        time: rate.frames_to_time(f64::from(marker.frame())),
                        comment: marker.name().into(),
                    })
                    .collect::<Vec<_>>();
                markers.sort_by(|a, b| a.time.total_cmp(&b.time));
                Ok(ae::LayerSnapshot {
                    id: ae::LayerId(layer.id),
                    name: layer.name.clone(),
                    start_time: rate.frames_to_time(layer.start_frame() as f64),
                    in_point: rate.frames_to_time(f64::from(layer.in_frame)),
                    out_point: rate.frames_to_time(f64::from(layer.out_frame(comp.duration))),
                    position: layer.expression_sample(
                        ExpressionTarget::Position,
                        frame,
                        comp.fps.seconds(1),
                        &mut sources,
                    )?,
                    scale: layer.expression_sample(
                        ExpressionTarget::Scale,
                        frame,
                        comp.fps.seconds(1),
                        &mut sources,
                    )?,
                    opacity: layer.expression_sample(
                        ExpressionTarget::Opacity,
                        frame,
                        comp.fps.seconds(1),
                        &mut sources,
                    )?,
                    source_text: layer
                        .source_text_at(frame.floor() as Frame)
                        .map(|_| {
                            layer.expression_sample(
                                ExpressionTarget::SourceText,
                                frame,
                                comp.fps.seconds(1),
                                &mut sources,
                            )
                        })
                        .transpose()?,
                    masks: layer
                        .path_masks
                        .iter()
                        .map(|mask| {
                            Ok(ae::MaskSnapshot {
                                id: mask.id,
                                property: layer.expression_sample(
                                    ExpressionTarget::MaskPath(mask.id),
                                    frame,
                                    comp.fps.seconds(1),
                                    &mut sources,
                                )?,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                    sliders,
                    markers,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ae::CompositionSnapshot {
            id: ae::CompositionId(composition),
            width: comp.width,
            height: comp.height,
            duration: rate.frames_to_time(f64::from(comp.duration)),
            frame_rate: rate,
            time: sample.seconds(),
            sources: sources.sources,
            layers,
        })
    }

    /// Only expressions which can affect the current render are roots. Sliders
    /// and hidden templates are lazy dependencies, never eager roots.
    pub fn expression_roots(
        &self,
        composition: CompositionId,
        frame: Frame,
        include_guides: bool,
    ) -> Result<Vec<ae::PropertyAddress>, String> {
        let comp = self.expression_composition(composition, frame)?;
        let mut pixels = BTreeSet::new();
        let mut transforms = BTreeSet::new();
        let mut pending = comp
            .layers
            .iter()
            .filter(|layer| {
                comp.layer_active(layer, frame, include_guides)
                    && !matches!(layer.content, Content::Null | Content::Audio { .. })
            })
            .map(|layer| layer.id)
            .collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            let layer = comp
                .layer(id)
                .ok_or("Missing expression render dependency")?;
            // Isolated mattes ignore visibility, Solo and Guide but honor trim.
            if frame < layer.in_frame
                || frame >= layer.out_frame(comp.duration)
                || !pixels.insert(id)
            {
                continue;
            }
            if let Some(matte) = layer.track_matte {
                pending.push(matte.source);
            }
        }
        let mut pending = pixels.iter().copied().collect::<Vec<_>>();
        if include_guides {
            // Preview draws controls for visible, active Nulls even though they
            // have no rendered pixels. Their opacity is never a render root.
            pending.extend(
                comp.layers
                    .iter()
                    .filter(|layer| {
                        matches!(layer.content, Content::Null)
                            && comp.layer_active(layer, frame, true)
                    })
                    .map(|layer| layer.id),
            );
        }
        while let Some(id) = pending.pop() {
            if !transforms.insert(id) {
                continue;
            }
            let layer = comp
                .layer(id)
                .ok_or("Missing expression transform dependency")?;
            if let Some(parent) = layer.parent {
                pending.push(parent);
            }
        }
        let mut roots = Vec::new();
        for layer in &comp.layers {
            if transforms.contains(&layer.id) {
                for (target, property) in [
                    (ExpressionTarget::Position, ae::ExpressionProperty::Position),
                    (ExpressionTarget::Scale, ae::ExpressionProperty::Scale),
                ] {
                    if layer.has_enabled_expression(target) {
                        roots.push(address(composition, layer.id, property));
                    }
                }
            }
            if pixels.contains(&layer.id) {
                if layer.has_enabled_expression(ExpressionTarget::SourceText) {
                    roots.push(address(
                        composition,
                        layer.id,
                        ae::ExpressionProperty::SourceText,
                    ));
                }
                for mask in &layer.path_masks {
                    if layer.has_enabled_expression(ExpressionTarget::MaskPath(mask.id)) {
                        roots.push(address(
                            composition,
                            layer.id,
                            ae::ExpressionProperty::MaskPath(mask.id),
                        ));
                    }
                }
            }
            if pixels.contains(&layer.id) && layer.has_enabled_expression(ExpressionTarget::Opacity)
            {
                roots.push(address(
                    composition,
                    layer.id,
                    ae::ExpressionProperty::Opacity,
                ));
            }
        }
        if comp.has_spatial_layers() && !roots.is_empty() {
            return Err(
                "Expression evaluation in spatial compositions is not yet supported".into(),
            );
        }
        Ok(roots)
    }

    /// Apply successful worker results to a detached frame-only render project.
    /// The caller must also check its source/context receipt before displaying it.
    /// Errors and missing roots never silently substitute authored values. Both
    /// native and JSON saves, serde serialization, and editor commits reject the view.
    pub fn with_evaluated_properties(
        &self,
        composition: CompositionId,
        frame: Frame,
        include_guides: bool,
        evaluated: &ae::EvaluatedProperties,
    ) -> Result<Project, String> {
        let fps = self
            .composition_by_id(composition)
            .ok_or("Composition not found")?
            .fps;
        self.with_evaluated_properties_at_sample(
            composition,
            CompositionSample::from_frame(frame, fps)?,
            include_guides,
            evaluated,
        )
    }

    pub fn with_evaluated_properties_at_sample(
        &self,
        composition: CompositionId,
        sample: CompositionSample,
        include_guides: bool,
        evaluated: &ae::EvaluatedProperties,
    ) -> Result<Self, String> {
        self.validate()?;
        let (comp, sample) = self.expression_composition_sample(composition, sample)?;
        render_sampling::admit(comp, sample)?;
        let frame = sample.floor_frame()?;
        if comp.has_spatial_layers() {
            return Err(
                "Expression evaluation in spatial compositions is not yet supported".into(),
            );
        }
        let time = sample.seconds();
        if evaluated.composition != ae::CompositionId(composition) || evaluated.time != time {
            return Err(
                "Expression results do not match the requested composition and frame".into(),
            );
        }
        let roots = self.expression_roots_at_sample(composition, sample, include_guides)?;
        if roots
            .iter()
            .any(|root| !evaluated.values.contains_key(root))
        {
            return Err("Expression evaluation is missing a required render property".into());
        }
        for (property, dependencies) in &evaluated.dependencies {
            if !evaluated.values.contains_key(property)
                || dependencies
                    .iter()
                    .any(|dependency| !evaluated.values.contains_key(dependency))
            {
                return Err("Expression evaluation has an unresolved dependency".into());
            }
        }
        let mut patches = Vec::new();
        for (key, value) in &evaluated.values {
            if key.composition != ae::CompositionId(composition) {
                return Err("Expression result belongs to another composition".into());
            }
            let layer = comp
                .layer(key.layer.0)
                .ok_or("Expression result layer not found")?;
            let target = match &key.property {
                ae::ExpressionProperty::Position => ExpressionTarget::Position,
                ae::ExpressionProperty::Scale => ExpressionTarget::Scale,
                ae::ExpressionProperty::Opacity => ExpressionTarget::Opacity,
                ae::ExpressionProperty::SourceText => ExpressionTarget::SourceText,
                ae::ExpressionProperty::MaskPath(id) => ExpressionTarget::MaskPath(*id),
                ae::ExpressionProperty::Slider(name) => ExpressionTarget::Slider(
                    layer
                        .effect_stack
                        .iter()
                        .find(|effect| {
                            effect.kind() == EffectKind::SliderControl && effect.name() == name
                        })
                        .ok_or("Expression result slider not found")?
                        .id(),
                ),
            };
            validate_value(target, value)?;
            if !layer.has_enabled_expression(target) {
                if layer.expression_value(target, sample.frame(), comp.fps.seconds(1))? != *value {
                    return Err(
                        "Expression result changed a pre-expression authored property".into(),
                    );
                }
                continue;
            }
            patches.push((layer.id, target, value));
        }
        reject_dependency_cycles(evaluated)?;
        let mut result = self.clone();
        let destination = if result.composition_id == composition {
            &mut result.composition
        } else {
            result
                .other_compositions
                .get_mut(&composition)
                .ok_or("Composition not found")?
        };
        render_sampling::freeze(destination, sample)?;
        for (id, target, value) in patches {
            let layer = destination
                .layers
                .iter_mut()
                .find(|layer| layer.id == id)
                .ok_or("Layer not found")?;
            let mut replace = |property, value| {
                let track = layer
                    .properties
                    .get_mut(&property)
                    .expect("validated property");
                track.keys.clear();
                track.value = value;
            };
            match (target, value) {
                (ExpressionTarget::Position, ae::PropertyValue::Vector2(v)) => {
                    // The evaluated frame view freezes a joined source only in
                    // this unsaveable copy; authored keys/absence remain intact.
                    layer
                        .properties
                        .insert(Property::PositionX, AnimatedProperty::new(v[0]));
                    layer
                        .properties
                        .insert(Property::PositionY, AnimatedProperty::new(v[1]));
                    layer.planar_position = None;
                }
                (ExpressionTarget::Scale, ae::PropertyValue::Vector2(v)) => {
                    replace(Property::ScaleX, v[0]);
                    replace(Property::ScaleY, v[1]);
                }
                (ExpressionTarget::Opacity, ae::PropertyValue::Scalar(v)) => {
                    replace(Property::Opacity, *v);
                    // Only the transient frame view loses timing. Authored source
                    // stays unchanged and this view cannot be saved or committed.
                    layer.opacity_timing = None;
                }
                (ExpressionTarget::Slider(id), ae::PropertyValue::Scalar(v)) => {
                    let track = layer
                        .effect_stack
                        .iter_mut()
                        .find(|effect| effect.id() == id)
                        .expect("validated slider")
                        .parameter_mut(EffectParam::Amount)
                        .expect("validated slider parameter");
                    track.keys.clear();
                    track.value = *v;
                }
                (ExpressionTarget::SourceText, ae::PropertyValue::Text(text)) => {
                    // Only an unsaveable evaluated copy changes. Uniform run styling
                    // is explicit; mixed-run TextDocument replacement is unsupported.
                    if let Some(rich) = &layer.rich_text
                        && !matches!(&layer.content, Content::Text { text: authored, .. } if authored == text)
                    {
                        let style = rich
                            .runs
                            .first()
                            .map_or(&rich.default_style, |run| &run.style)
                            .clone();
                        let point_origin = rich.point_origin;
                        let runs = if text.is_empty() {
                            vec![]
                        } else {
                            vec![TextStyleRun {
                                start: 0,
                                end: text.len(),
                                style: style.clone(),
                            }]
                        };
                        let mut replacement = RichText::new(text, style, runs)?;
                        replacement.point_origin = point_origin;
                        layer.rich_text = Some(replacement);
                    }
                    let Content::Text { text: authored, .. } = &mut layer.content else {
                        return Err("Source Text expression requires a text layer".into());
                    };
                    *authored = text.clone();
                    layer.source_text_animation = SourceTextAnimation::default();
                }
                (ExpressionTarget::MaskPath(id), ae::PropertyValue::Path(path)) => {
                    let mask = layer
                        .path_masks
                        .iter_mut()
                        .find(|mask| mask.id == id)
                        .ok_or("Expression result mask not found")?;
                    mask.path = native_path(path);
                    mask.animation = PathAnimation::default();
                }
                _ => unreachable!("validated result dimensions"),
            }
        }
        result.evaluated_frame = Some((composition, frame));
        result.render_sample = Some(RenderSampleReceipt {
            composition,
            sample,
        });
        Ok(result)
    }

    pub fn evaluated_frame(&self) -> Option<(CompositionId, Frame)> {
        self.evaluated_frame
    }
}

fn validate_value(target: ExpressionTarget, value: &ae::PropertyValue) -> Result<(), String> {
    let valid = match (target, value) {
        (ExpressionTarget::Position, ae::PropertyValue::Vector2(v)) => {
            Property::PositionX.accepts(v[0]) && Property::PositionY.accepts(v[1])
        }
        (ExpressionTarget::Scale, ae::PropertyValue::Vector2(v)) => {
            Property::ScaleX.accepts(v[0]) && Property::ScaleY.accepts(v[1])
        }
        // Expression and pre-expression results are raw. Paint clamps them;
        // persisted authored values still use Property::Opacity.accepts.
        (ExpressionTarget::Opacity, ae::PropertyValue::Scalar(v)) => v.is_finite(),
        (ExpressionTarget::Slider(_), ae::PropertyValue::Scalar(v)) => {
            v.is_finite() && v.abs() <= 1_000_000.0
        }
        (ExpressionTarget::SourceText, ae::PropertyValue::Text(text)) => {
            text.len() <= ae::MAX_EXPRESSION_TEXT_BYTES && !text.contains('\0')
        }
        (ExpressionTarget::MaskPath(_), ae::PropertyValue::Path(path)) => {
            path.closed && path.is_valid()
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(
            "Expression result has invalid dimensions or is outside the property's finite bounds"
                .into(),
        )
    }
}

fn reject_dependency_cycles(evaluated: &ae::EvaluatedProperties) -> Result<(), String> {
    fn visit<'a>(
        key: &'a ae::PropertyAddress,
        evaluated: &'a ae::EvaluatedProperties,
        visiting: &mut BTreeSet<&'a ae::PropertyAddress>,
        done: &mut BTreeSet<&'a ae::PropertyAddress>,
    ) -> Result<(), String> {
        if done.contains(key) {
            return Ok(());
        }
        if !visiting.insert(key) {
            return Err("Expression dependency cycle in worker results".into());
        }
        if visiting.len() > 64 {
            return Err("Expression dependency depth exceeds 64".into());
        }
        for dependency in evaluated.dependencies.get(key).into_iter().flatten() {
            visit(dependency, evaluated, visiting, done)?;
        }
        visiting.remove(key);
        done.insert(key);
        Ok(())
    }
    let mut done = BTreeSet::new();
    for key in evaluated.values.keys() {
        visit(key, evaluated, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}
