//! Opt-in continuous nested visual time, over a bounded detached frame view.
//! Authored tracks keep integer keys. These samples never enter saved documents.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderSampleReceipt {
    pub composition: CompositionId,
    pub sample: CompositionSample,
}

impl Composition {
    /// None is the legacy native frame-grid contract. Explicit false opts into
    /// continuous nested visual sampling without changing this composition's FPS.
    pub fn preserve_nested_frame_rate(&self) -> Option<bool> {
        self.preserve_nested_frame_rate
    }
}

pub(super) fn materialized(project: &Project) -> bool {
    project
        .compositions()
        .iter()
        .any(|(_, comp)| comp.preserve_nested_frame_rate.is_some())
}

pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::SetPreserveNestedFrameRate { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    let Command::SetPreserveNestedFrameRate {
        composition,
        preserve,
    } = command
    else {
        return None;
    };
    Some((|| {
        let project = &mut state.project;
        let comp = if *composition == project.composition_id {
            &mut project.composition
        } else {
            project
                .other_compositions
                .get_mut(composition)
                .ok_or("Composition not found")?
        };
        comp.preserve_nested_frame_rate = Some(*preserve);
        Ok(())
    })())
}

impl Layer {
    /// Map containing-composition time to source time. Trim and expression
    /// startTime are separate metadata, not additional origin subtractions.
    pub fn composition_sample(
        &self,
        parent_sample: CompositionSample,
        parent_fps: FrameRate,
        source: &Composition,
    ) -> Result<Option<CompositionSample>, String> {
        let Content::Composition { start_frame, .. } = self.content else {
            return Ok(None);
        };
        let parent_sample = parent_sample.in_rate(parent_fps)?;
        if source.preserve_nested_frame_rate.is_none() && !parent_sample.is_fractional() {
            // Preserve the pre-existing integer conversion and remap tolerance.
            return self
                .composition_frame(parent_sample.floor_frame()?, parent_fps, source)
                .map(|frame| CompositionSample::from_frame(frame, source.fps))
                .transpose();
        }
        let sample = if let Some(remap) = &self.time_remap {
            CompositionSample::from_seconds(remap.sample(parent_sample.frame()), source.fps)?
        } else {
            parent_sample
                .subtract_frames(start_frame, parent_fps)?
                .in_rate(source.fps)?
        };
        if sample.seconds() < 0.0 || sample.frame() > f64::from(Frame::MAX) {
            return Ok(None);
        }
        if sample.floor_frame()? >= source.duration {
            return Ok(None);
        }
        if source.preserve_nested_frame_rate == Some(false) {
            Ok(Some(sample))
        } else {
            Ok(Some(sample.quantized(source.fps)?))
        }
    }
}

fn reject_feature(name: &str) -> String {
    format!("Continuous nested sampling does not yet support {name}")
}

fn static_contents(nodes: &[ContentsNode]) -> Result<(), String> {
    for node in nodes {
        if node.parameters.values().any(|track| !track.keys.is_empty()) {
            return Err(reject_feature("animated Shape Contents parameters"));
        }
        match &node.kind {
            ContentsKind::Group(children) => static_contents(children)?,
            ContentsKind::Path { animation, .. } if animation.animated() => {
                return Err(reject_feature("animated Shape Contents paths"));
            }
            ContentsKind::GradientFill { .. } | ContentsKind::GradientStroke { .. } => {
                return Err(reject_feature("Shape Contents gradients"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reject every varying family not materialized below. Hidden parents, templates
/// and matte providers remain dependencies, so the admission covers this whole
/// sampled composition, not unrelated compositions in the project.
pub(super) fn admit(comp: &Composition, sample: CompositionSample) -> Result<(), String> {
    if !sample.is_fractional() {
        return Ok(());
    }
    if sample.frame().fract() == 0.0 {
        return Err(
            "Fractional sample is below the supported floating-point frame resolution".into(),
        );
    }
    if comp.camera.is_some() || comp.layers.iter().any(Layer::is_three_d) {
        return Err(reject_feature("spatial/camera compositions"));
    }
    for layer in &comp.layers {
        if layer
            .text_parameters
            .values()
            .any(|track| !track.keys.is_empty())
            || !layer.text_selector.is_default()
            || !layer.text_animators.is_empty()
            || !layer.text_range_selectors.is_empty()
        {
            return Err(reject_feature("animated text typography/paint/selectors"));
        }
        for mask in &layer.path_masks {
            if mask.animation.animated()
                || mask.parameters.values().any(|track| !track.keys.is_empty())
            {
                return Err(reject_feature("authored mask animation"));
            }
        }
        for effect in &layer.effect_stack {
            if effect.kind() != EffectKind::SliderControl
                && effect.kind().parameters().iter().any(|spec| {
                    effect
                        .parameter(spec.parameter)
                        .is_some_and(|track| !track.keys.is_empty())
                })
            {
                return Err(reject_feature("animated effects other than Slider Control"));
            }
        }
        match &layer.content {
            Content::Video { .. } | Content::ImageSequence { .. } => {
                return Err(reject_feature("video or image-sequence footage"));
            }
            Content::Shape(shape)
                if shape.path_animation.animated()
                    || shape
                        .parameters
                        .values()
                        .any(|track| !track.keys.is_empty()) =>
            {
                return Err(reject_feature("animated shape geometry or paint"));
            }
            Content::ShapeContents(contents) => static_contents(&contents.items)?,
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn freeze(comp: &mut Composition, sample: CompositionSample) -> Result<(), String> {
    admit(comp, sample)?;
    if !sample.is_fractional() {
        return Ok(());
    }
    let frame = sample.frame();
    let floor = sample.floor_frame()?;
    let seconds_per_frame = comp.fps.seconds(1);
    for layer in &mut comp.layers {
        layer.visible = layer.active_at_sample(frame, comp.duration);
        let position = layer.position2_sample(frame, seconds_per_frame)?;
        let opacity = layer.opacity_sample(frame, seconds_per_frame)?;
        let text = layer.source_text_at(floor).map(str::to_owned);
        for track in layer.properties.values_mut() {
            track.value = track.sample(frame);
            track.keys.clear();
        }
        layer
            .properties
            .insert(Property::PositionX, AnimatedProperty::new(position[0]));
        layer
            .properties
            .insert(Property::PositionY, AnimatedProperty::new(position[1]));
        layer
            .properties
            .insert(Property::Opacity, AnimatedProperty::new(opacity));
        layer.planar_position = None;
        layer.opacity_timing = None;
        for effect in &mut layer.effect_stack {
            if effect.kind() == EffectKind::SliderControl {
                let track = effect
                    .parameter_mut(EffectParam::Amount)
                    .ok_or("Missing Slider amount")?;
                track.value = track.sample(frame);
                track.keys.clear();
            }
        }
        // Source Text's authored key contract is discrete hold, not interpolation
        // of pool indices. Baked text retains the native uniform/rich style rules.
        if let Some(text) = text {
            layer.bake_source_text(text)?;
        }
    }
    Ok(())
}

pub(super) fn reject_serialization<S>(
    _: &Option<RenderSampleReceipt>,
    _: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    Err(serde::ser::Error::custom(
        "A sampled render view cannot be serialized",
    ))
}

impl Project {
    pub fn render_sample_receipt(&self) -> Option<RenderSampleReceipt> {
        self.render_sample
    }
    pub fn evaluated_at_sample(
        &self,
        composition: CompositionId,
        sample: CompositionSample,
    ) -> bool {
        self.render_sample.is_some_and(|receipt| {
            receipt.composition == composition && receipt.sample.key() == sample.key()
        })
    }
}
