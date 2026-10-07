//! Whole-gradient color/opacity snapshots. Temporal interpolation is explicit
//! and requires matching ordered, paint-local stop identities in both rows.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientColorStop {
    pub id: u64,
    pub position: f64,
    pub midpoint: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientOpacityStop {
    pub id: u64,
    pub position: f64,
    pub midpoint: f64,
    pub opacity: f64,
}
// Spatial sorting uses total_cmp, so signed zero may change coincident-stop
// order. Snapshot equality must not erase such a source/render change.
impl PartialEq for GradientColorStop {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && [
                self.position,
                self.midpoint,
                self.red,
                self.green,
                self.blue,
            ]
            .map(f64::to_bits)
                == [
                    other.position,
                    other.midpoint,
                    other.red,
                    other.green,
                    other.blue,
                ]
                .map(f64::to_bits)
    }
}
impl PartialEq for GradientOpacityStop {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && [self.position, self.midpoint, self.opacity].map(f64::to_bits)
                == [other.position, other.midpoint, other.opacity].map(f64::to_bits)
    }
}
/// Ordered independent stop rows. Order breaks ties at coincident positions;
/// identities are paint-local and have no meaning across separate gradients.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientColors {
    pub colors: Vec<GradientColorStop>,
    pub opacities: Vec<GradientOpacityStop>,
}
impl GradientColors {
    fn compatible_topology(&self, other: &Self) -> bool {
        self.colors
            .iter()
            .map(|s| s.id)
            .eq(other.colors.iter().map(|s| s.id))
            && self
                .opacities
                .iter()
                .map(|s| s.id)
                .eq(other.opacities.iter().map(|s| s.id))
    }
    fn interpolate(&self, other: &Self, progress: f64) -> Self {
        // Valid snapshots are bounded, so the difference cannot overflow. Clamp
        // to the endpoints to retain those bounds even at rounding extremes.
        let mix = |a: f64, b: f64| {
            // Constant signed-zero positions affect spatial total_cmp ordering.
            if a.to_bits() == b.to_bits() {
                a
            } else {
                (a + (b - a) * progress).clamp(a.min(b), a.max(b))
            }
        };
        Self {
            colors: self
                .colors
                .iter()
                .zip(&other.colors)
                .map(|(a, b)| GradientColorStop {
                    id: a.id,
                    position: mix(a.position, b.position),
                    midpoint: mix(a.midpoint, b.midpoint),
                    red: mix(a.red, b.red),
                    green: mix(a.green, b.green),
                    blue: mix(a.blue, b.blue),
                })
                .collect(),
            opacities: self
                .opacities
                .iter()
                .zip(&other.opacities)
                .map(|(a, b)| GradientOpacityStop {
                    id: a.id,
                    position: mix(a.position, b.position),
                    midpoint: mix(a.midpoint, b.midpoint),
                    opacity: mix(a.opacity, b.opacity),
                })
                .collect(),
        }
    }
    pub fn value(&self, parameter: GradientParam) -> Option<f64> {
        use GradientParam::*;
        let id = parameter.stop()?;
        match parameter {
            ColorPosition(_) | ColorMidpoint(_) | Red(_) | Green(_) | Blue(_) => {
                let s = self.colors.iter().find(|s| s.id == id)?;
                Some(match parameter {
                    ColorPosition(_) => s.position,
                    ColorMidpoint(_) => s.midpoint,
                    Red(_) => s.red,
                    Green(_) => s.green,
                    Blue(_) => s.blue,
                    _ => unreachable!(),
                })
            }
            OpacityPosition(_) | OpacityMidpoint(_) | Opacity(_) => {
                let s = self.opacities.iter().find(|s| s.id == id)?;
                Some(match parameter {
                    OpacityPosition(_) => s.position,
                    OpacityMidpoint(_) => s.midpoint,
                    Opacity(_) => s.opacity,
                    _ => unreachable!(),
                })
            }
            _ => None,
        }
    }
    pub fn color_at(&self, id: u64) -> Option<u32> {
        let s = self.colors.iter().find(|s| s.id == id)?;
        Some(
            [s.red, s.green, s.blue]
                .into_iter()
                .fold(0, |c, v| (c << 8) | v.round() as u32),
        )
    }
    fn values(&self) -> Vec<(GradientParam, f64)> {
        use GradientParam::*;
        self.colors
            .iter()
            .flat_map(|s| {
                [
                    (ColorPosition(s.id), s.position),
                    (ColorMidpoint(s.id), s.midpoint),
                    (Red(s.id), s.red),
                    (Green(s.id), s.green),
                    (Blue(s.id), s.blue),
                ]
            })
            .chain(self.opacities.iter().flat_map(|s| {
                [
                    (OpacityPosition(s.id), s.position),
                    (OpacityMidpoint(s.id), s.midpoint),
                    (Opacity(s.id), s.opacity),
                ]
            }))
            .collect()
    }
    fn validate(&self, roles: &mut BTreeMap<u64, bool>) -> Result<(), String> {
        if !(2..=ShapeGradient::MAX_STOPS).contains(&self.colors.len())
            || !(2..=ShapeGradient::MAX_STOPS).contains(&self.opacities.len())
        {
            return Err("Keep 2–32 color stops and 2–32 opacity stops".into());
        }
        let mut ids = BTreeSet::new();
        for (id, opacity) in self
            .colors
            .iter()
            .map(|s| (s.id, false))
            .chain(self.opacities.iter().map(|s| (s.id, true)))
        {
            if id == 0
                || id == u64::MAX
                || !ids.insert(id)
                || roles.insert(id, opacity).is_some_and(|old| old != opacity)
            {
                return Err(
                    "Gradient stop identities must be unique, nonzero and keep their row".into(),
                );
            }
        }
        if self
            .values()
            .iter()
            .any(|(p, value)| !ContentsParam::Gradient(*p).accepts(*value))
        {
            return Err("Gradient Colors contains a nonfinite or out-of-range value".into());
        }
        Ok(())
    }
    fn set_value(&mut self, parameter: GradientParam, value: f64) -> Result<(), String> {
        if !ContentsParam::Gradient(parameter).accepts(value) {
            return Err("Invalid Gradient Colors stop value".into());
        }
        use GradientParam::*;
        let id = parameter
            .stop()
            .ok_or("Gradient Colors edits require a stop property")?;
        let value_ref = match parameter {
            ColorPosition(_) | ColorMidpoint(_) | Red(_) | Green(_) | Blue(_) => {
                let s = self
                    .colors
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or("Gradient color stop no longer exists")?;
                match parameter {
                    ColorPosition(_) => &mut s.position,
                    ColorMidpoint(_) => &mut s.midpoint,
                    Red(_) => &mut s.red,
                    Green(_) => &mut s.green,
                    Blue(_) => &mut s.blue,
                    _ => unreachable!(),
                }
            }
            OpacityPosition(_) | OpacityMidpoint(_) | Opacity(_) => {
                let s = self
                    .opacities
                    .iter_mut()
                    .find(|s| s.id == id)
                    .ok_or("Gradient opacity stop no longer exists")?;
                match parameter {
                    OpacityPosition(_) => &mut s.position,
                    OpacityMidpoint(_) => &mut s.midpoint,
                    Opacity(_) => &mut s.opacity,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        };
        *value_ref = value;
        Ok(())
    }
    fn max_id(&self) -> u64 {
        self.colors
            .iter()
            .map(|s| s.id)
            .chain(self.opacities.iter().map(|s| s.id))
            .max()
            .unwrap_or(0)
    }
}

/// An outgoing whole-snapshot mode. Hold is implicit in legacy documents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientColorsInterpolation {
    #[default]
    Hold,
    Linear,
    Smooth,
}

/// A complete paint-local key snapshot at an offset from a copied range's first
/// key. This is source-edit data, not a cross-paint or cross-project clipboard:
/// callers must retain the source paint's identities and establish ownership.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientColorsKeyCopy {
    pub offset: Frame,
    pub colors: GradientColors,
    pub interpolation: GradientColorsInterpolation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientColorsHoldReason {
    BeforeFirstKey,
    NoNextKey,
    IncompatibleTopology,
}

/// Requested and effective outgoing behavior without inferring stop matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientColorsSegmentStatus {
    pub frame: Frame,
    pub next_frame: Option<Frame>,
    pub interpolation: GradientColorsInterpolation,
    pub effective: GradientColorsInterpolation,
    pub hold_reason: Option<GradientColorsHoldReason>,
}

/// Complete snapshots including stop count and order. Before the first key and
/// after the last key a snapshot is held. Absent metadata retains legacy Hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientColorsAnimation {
    keys: BTreeMap<Frame, GradientColors>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    outgoing_interpolation: BTreeMap<Frame, GradientColorsInterpolation>,
}
impl GradientColorsAnimation {
    pub const MAX_KEYS: usize = 1000;
    pub const MAX_STORED_STOPS: usize = 32768;
    pub fn keys(&self) -> &BTreeMap<Frame, GradientColors> {
        &self.keys
    }
    /// Outgoing mode at an exact key, including the dormant last-key mode.
    pub fn interpolation(&self, frame: Frame) -> Option<GradientColorsInterpolation> {
        self.keys.contains_key(&frame).then(|| {
            self.outgoing_interpolation
                .get(&frame)
                .copied()
                .unwrap_or_default()
        })
    }
    /// Status of the outgoing segment at an exact key.
    pub fn segment_status(&self, frame: Frame) -> Option<GradientColorsSegmentStatus> {
        use GradientColorsHoldReason::*;
        use GradientColorsInterpolation::Hold;
        let colors = self.keys.get(&frame)?;
        let interpolation = self.interpolation(frame)?;
        let next = self
            .keys
            .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
            .next();
        let hold_reason = match next {
            None => Some(NoNextKey),
            Some((_, other)) if interpolation != Hold && !colors.compatible_topology(other) => {
                Some(IncompatibleTopology)
            }
            _ => None,
        };
        Some(GradientColorsSegmentStatus {
            frame,
            next_frame: next.map(|(&frame, _)| frame),
            interpolation,
            effective: if hold_reason.is_some() {
                Hold
            } else {
                interpolation
            },
            hold_reason,
        })
    }
    /// Resolve the playhead's segment, including the held pre-first region.
    pub fn segment_at(&self, frame: Frame) -> Option<GradientColorsSegmentStatus> {
        let (&start, _) = self
            .keys
            .range(..=frame)
            .next_back()
            .or_else(|| self.keys.first_key_value())?;
        let mut status = self.segment_status(start)?;
        if frame < start {
            status.effective = GradientColorsInterpolation::Hold;
            status.hold_reason = Some(GradientColorsHoldReason::BeforeFirstKey);
        }
        Some(status)
    }
    fn at(&self, frame: Frame) -> Option<GradientColors> {
        let status = self.segment_at(frame)?;
        let colors = self.keys.get(&status.frame)?;
        // Preserve endpoint bits and row storage exactly, including signed zero.
        if frame == status.frame || status.effective == GradientColorsInterpolation::Hold {
            return Some(colors.clone());
        }
        let end = status.next_frame?;
        let t = f64::from(frame - status.frame) / f64::from(end - status.frame);
        let progress = match status.effective {
            GradientColorsInterpolation::Smooth => t * t * (3. - 2. * t),
            _ => t,
        };
        Some(colors.interpolate(self.keys.get(&end)?, progress))
    }
    pub(super) fn required_version(&self) -> u32 {
        if self.outgoing_interpolation.is_empty() {
            54
        } else {
            57
        }
    }
    fn validate_interpolation(&self) -> Result<(), String> {
        if self.outgoing_interpolation.iter().any(|(frame, mode)| {
            *mode == GradientColorsInterpolation::Hold || !self.keys.contains_key(frame)
        }) {
            return Err("Gradient Colors interpolation must reference an existing key and omit Hold defaults".into());
        }
        Ok(())
    }
    fn validate_selection(&self, frames: &BTreeSet<Frame>) -> Result<(), String> {
        if frames.is_empty() || frames.len() > Self::MAX_KEYS {
            return Err("Select 1–1000 Gradient Colors keys".into());
        }
        if frames.iter().any(|frame| !self.keys.contains_key(frame)) {
            return Err("Gradient Colors key no longer exists".into());
        }
        Ok(())
    }
    /// Preview same-paint time scaling with the earliest selected key fixed and
    /// the latest selected key at `to`. Integral offsets round to the nearest
    /// frame, with exact half frames rounding later. Selected source positions
    /// may be reused; rounded duplicates and unselected collisions reject.
    /// This mapping is shared by the editor transaction and selection feedback.
    pub fn scaled_key_frames(
        &self,
        frames: &BTreeSet<Frame>,
        to: Frame,
        duration: Frame,
    ) -> Result<BTreeMap<Frame, Frame>, String> {
        self.validate_selection(frames)?;
        if frames.len() < 2 {
            return Err("Select 2–1000 Gradient Colors keys to scale".into());
        }
        let first = *frames.first().unwrap();
        let last = *frames.last().unwrap();
        if last >= duration || to >= duration {
            return Err("Gradient Colors key is outside the composition".into());
        }
        let new_span = to
            .checked_sub(first)
            .filter(|span| *span > 0)
            .ok_or("Gradient Colors scale end must be after the first selected key")?;
        let old_span = u64::from(last - first);
        let mut used = BTreeSet::new();
        frames
            .iter()
            .map(|&from| {
                // Wide checked arithmetic keeps even u32::MAX-scale spans
                // exact, without floating-point drift at a rounding boundary.
                let offset = u64::from(from - first)
                    .checked_mul(u64::from(new_span))
                    .and_then(|product| product.checked_add(old_span / 2))
                    .ok_or("Gradient Colors scaled key time overflow")?
                    / old_span;
                let destination = Frame::try_from(offset)
                    .ok()
                    .and_then(|offset| first.checked_add(offset))
                    .filter(|destination| *destination < duration)
                    .ok_or("Gradient Colors key is outside the composition")?;
                if !used.insert(destination) {
                    return Err("Scaled Gradient Colors keys would share a frame".into());
                }
                if self.keys.contains_key(&destination) && !frames.contains(&destination) {
                    return Err("Gradient Colors destination already has a key".into());
                }
                Ok((from, destination))
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub enum GradientColorsEdit {
    SetAnimation {
        frame: Frame,
        enabled: bool,
    },
    ToggleKey {
        frame: Frame,
    },
    Set {
        frame: Frame,
        colors: GradientColors,
    },
    Value {
        frame: Frame,
        parameter: GradientParam,
        value: f64,
    },
    Color {
        frame: Frame,
        stop: u64,
        color: u32,
    },
    AddStop {
        frame: Frame,
        opacity: bool,
        position: f64,
    },
    RemoveStop {
        frame: Frame,
        stop: u64,
    },
    /// Move one key without overwriting another key.
    MoveKey {
        from: Frame,
        to: Frame,
    },
    /// Set the outgoing mode of an existing key; incompatible spans hold.
    SetInterpolation {
        frame: Frame,
        interpolation: GradientColorsInterpolation,
    },
    /// Remove an existing key, baking its sample if it was the last key.
    DeleteKey {
        frame: Frame,
    },
    /// Move selected keys together, anchoring the earliest key at `to`.
    /// Selected destinations may overlap sources; unselected keys cannot be
    /// overwritten. Every complete snapshot retains its outgoing mode.
    MoveKeys {
        frames: BTreeSet<Frame>,
        to: Frame,
    },
    /// Scale selected key times with the earliest key fixed and the latest at
    /// `to`. Complete snapshots and outgoing modes move atomically. The shared
    /// `scaled_key_frames` preview defines rounding and collision behavior.
    ScaleKeys {
        frames: BTreeSet<Frame>,
        to: Frame,
    },
    /// Remove selected keys atomically. If none remain, bake the original
    /// animation's sample at the explicit playhead `frame`.
    DeleteKeys {
        frames: BTreeSet<Frame>,
        frame: Frame,
    },
    /// Change the outgoing mode of every selected existing key atomically.
    SetInterpolations {
        frames: BTreeSet<Frame>,
        interpolation: GradientColorsInterpolation,
    },
    /// Insert bounded, complete paint-local snapshots into an existing Colors
    /// animation. Offsets are unique and start at zero. Collisions reject unless
    /// the entire payload exactly matches keys already at the destination.
    PasteKeys {
        keys: Vec<GradientColorsKeyCopy>,
        frame: Frame,
    },
}

impl ShapeGradient {
    pub fn colors_animation(&self) -> Option<&GradientColorsAnimation> {
        self.colors_animation.as_ref()
    }
    pub fn colors_at(&self, node: &ContentsNode, frame: Frame) -> GradientColors {
        if let Some(colors) = self.colors_animation.as_ref().and_then(|a| a.at(frame)) {
            return colors;
        }
        self.legacy_colors_at(node, frame)
    }
    fn legacy_colors_at(&self, node: &ContentsNode, frame: Frame) -> GradientColors {
        use GradientParam::*;
        let value = |p| node.value_at(ContentsParam::Gradient(p), frame);
        GradientColors {
            colors: self
                .colors
                .iter()
                .map(|&id| GradientColorStop {
                    id,
                    position: value(ColorPosition(id)),
                    midpoint: value(ColorMidpoint(id)),
                    red: value(Red(id)),
                    green: value(Green(id)),
                    blue: value(Blue(id)),
                })
                .collect(),
            opacities: self
                .opacities
                .iter()
                .map(|&id| GradientOpacityStop {
                    id,
                    position: value(OpacityPosition(id)),
                    midpoint: value(OpacityMidpoint(id)),
                    opacity: value(Opacity(id)),
                })
                .collect(),
        }
    }
    /// Static display clone for ramp/modal editing. Only stop storage is baked;
    /// endpoint, highlight, stroke and all other node metadata stays intact.
    pub fn sampled_node(&self, node: &ContentsNode, frame: Frame) -> ContentsNode {
        let mut sampled = node.clone();
        bake(&mut sampled, &self.colors_at(node, frame));
        sampled
    }
    pub fn colors_animation_compatible(&self, node: &ContentsNode) -> bool {
        !node
            .parameters
            .iter()
            .any(|(p, track)| is_stop(*p) && !track.keys.is_empty())
    }
    pub(super) fn validate_colors_animation(
        &self,
        node: &ContentsNode,
        duration: Frame,
        version: u32,
    ) -> Result<(), String> {
        let Some(animation) = &self.colors_animation else {
            return Ok(());
        };
        animation.validate_interpolation()?;
        if version < animation.required_version() {
            return Err(format!(
                "Gradient Colors animation requires project version {}",
                animation.required_version()
            ));
        }
        if version < 54
            || animation.keys.is_empty()
            || animation.keys.len() > GradientColorsAnimation::MAX_KEYS
            || !self.colors_animation_compatible(node)
        {
            return Err("Invalid compound Gradient Colors or unsupported project version (requires 54); legacy stop animation cannot coexist".into());
        }
        let mut roles = BTreeMap::new();
        self.legacy_colors_at(node, 0).validate(&mut roles)?;
        let mut total = 0usize;
        for (frame, colors) in &animation.keys {
            colors.validate(&mut roles)?;
            total = total
                .checked_add(colors.colors.len() + colors.opacities.len())
                .ok_or("Gradient Colors storage overflow")?;
            if *frame >= duration
                || colors.max_id() >= self.next_stop
                || total > GradientColorsAnimation::MAX_STORED_STOPS
            {
                return Err(
                    "Gradient Colors key, stop identity or stored-stop budget is invalid".into(),
                );
            }
        }
        Ok(())
    }
}
pub(super) fn is_stop(parameter: ContentsParam) -> bool {
    matches!(parameter, ContentsParam::Gradient(p) if p.stop().is_some())
}
impl ContentsNode {
    pub(super) fn scalar_parameter_available(&self, parameter: ContentsParam) -> bool {
        !is_stop(parameter)
            || self
                .kind
                .gradient()
                .is_none_or(|g| g.colors_animation.is_none())
    }
}
fn bake(node: &mut ContentsNode, colors: &GradientColors) {
    let g = node.kind.gradient_mut().expect("gradient target validated");
    g.colors = colors.colors.iter().map(|s| s.id).collect();
    g.opacities = colors.opacities.iter().map(|s| s.id).collect();
    g.colors_animation = None;
    node.parameters.retain(|p, _| !is_stop(*p));
    node.parameters.extend(
        colors
            .values()
            .into_iter()
            .map(|(p, value)| (ContentsParam::Gradient(p), AnimatedProperty::new(value))),
    );
}
fn store(node: &mut ContentsNode, frame: Frame, colors: GradientColors) -> Result<(), String> {
    let g = node.kind.gradient().ok_or("Select a gradient paint")?;
    let mut roles = BTreeMap::new();
    g.legacy_colors_at(node, 0).validate(&mut roles)?;
    if let Some(animation) = &g.colors_animation {
        for old in animation.keys.values() {
            old.validate(&mut roles)?;
        }
    }
    colors.validate(&mut roles)?;
    if g.colors_at(node, frame) == colors {
        return Ok(());
    }
    let next_stop = g.next_stop.max(
        colors
            .max_id()
            .checked_add(1)
            .ok_or("Gradient stop ID exhausted")?,
    );
    if g.colors_animation.is_some() {
        let g = node.kind.gradient_mut().unwrap();
        g.next_stop = next_stop;
        let keys = &mut g.colors_animation.as_mut().unwrap().keys;
        if !keys.contains_key(&frame) && keys.len() >= GradientColorsAnimation::MAX_KEYS {
            return Err("Gradient Colors supports at most 1000 keys".into());
        }
        keys.insert(frame, colors);
    } else {
        if !g.colors_animation_compatible(node) {
            return Err(
                "Turn off legacy stop animation before editing compound Gradient Colors".into(),
            );
        }
        node.kind.gradient_mut().unwrap().next_stop = next_stop;
        bake(node, &colors);
    }
    Ok(())
}

pub(super) fn edit(
    node: &mut ContentsNode,
    edit: &GradientColorsEdit,
    duration: Frame,
) -> Result<(), String> {
    let frame = match edit {
        GradientColorsEdit::SetAnimation { frame, .. }
        | GradientColorsEdit::ToggleKey { frame }
        | GradientColorsEdit::Set { frame, .. }
        | GradientColorsEdit::Value { frame, .. }
        | GradientColorsEdit::Color { frame, .. }
        | GradientColorsEdit::AddStop { frame, .. }
        | GradientColorsEdit::RemoveStop { frame, .. }
        | GradientColorsEdit::SetInterpolation { frame, .. }
        | GradientColorsEdit::DeleteKey { frame }
        | GradientColorsEdit::DeleteKeys { frame, .. }
        | GradientColorsEdit::PasteKeys { frame, .. } => *frame,
        GradientColorsEdit::MoveKeys { to, .. } | GradientColorsEdit::ScaleKeys { to, .. } => *to,
        GradientColorsEdit::SetInterpolations { frames, .. } => {
            *frames.first().ok_or("Select 1–1000 Gradient Colors keys")?
        }
        GradientColorsEdit::MoveKey { from, to } => {
            if *from >= duration {
                return Err("Gradient Colors key is outside the composition".into());
            }
            *to
        }
    };
    if frame >= duration {
        return Err("Gradient Colors key is outside the composition".into());
    }
    let g = node.kind.gradient().ok_or("Select a gradient paint")?;
    let mut colors = g.colors_at(node, frame);
    match edit {
        GradientColorsEdit::SetAnimation { enabled, .. } => {
            if *enabled == g.colors_animation.is_some() {
                return Ok(());
            }
            if !enabled {
                bake(node, &colors);
            } else {
                if !g.colors_animation_compatible(node) {
                    return Err(
                        "Turn off legacy stop animation before enabling Gradient Colors animation"
                            .into(),
                    );
                }
                node.kind.gradient_mut().unwrap().colors_animation =
                    Some(GradientColorsAnimation {
                        keys: BTreeMap::from([(frame, colors)]),
                        outgoing_interpolation: BTreeMap::new(),
                    });
            }
        }
        GradientColorsEdit::ToggleKey { .. } => {
            if !g.colors_animation_compatible(node) {
                return Err(
                    "Turn off legacy stop animation before enabling Gradient Colors animation"
                        .into(),
                );
            }
            let g = node.kind.gradient_mut().unwrap();
            if let Some(animation) = &mut g.colors_animation {
                if animation.keys.remove(&frame).is_some() {
                    animation.outgoing_interpolation.remove(&frame);
                    if animation.keys.is_empty() {
                        bake(node, &colors);
                    }
                } else {
                    if animation.keys.len() >= GradientColorsAnimation::MAX_KEYS {
                        return Err("Gradient Colors supports at most 1000 keys".into());
                    }
                    animation.keys.insert(frame, colors);
                }
            } else {
                g.colors_animation = Some(GradientColorsAnimation {
                    keys: BTreeMap::from([(frame, colors)]),
                    outgoing_interpolation: BTreeMap::new(),
                });
            }
        }
        GradientColorsEdit::Set { colors, .. } => store(node, frame, colors.clone())?,
        GradientColorsEdit::Value {
            parameter, value, ..
        } => {
            colors.set_value(*parameter, *value)?;
            store(node, frame, colors)?;
        }
        GradientColorsEdit::Color { stop, color, .. } => {
            if *color > 0xffffff {
                return Err("Gradient color must be a 24-bit RGB value".into());
            }
            for (p, shift) in [
                (GradientParam::Red(*stop), 16),
                (GradientParam::Green(*stop), 8),
                (GradientParam::Blue(*stop), 0),
            ] {
                colors.set_value(p, ((color >> shift) & 255) as f64)?;
            }
            store(node, frame, colors)?;
        }
        GradientColorsEdit::AddStop {
            opacity, position, ..
        } => {
            if !position.is_finite() || !(0. ..=100.).contains(position) {
                return Err("Stop location must be 0–100%".into());
            }
            // Reuse the unchanged legacy spatial sampler on a sampled static clone.
            let mut sample = g.sampled_node(node, frame);
            ShapeGradient::add_stop(&mut sample, *opacity, *position, frame)?;
            let colors = sample.kind.gradient().unwrap().colors_at(&sample, frame);
            store(node, frame, colors)?;
        }
        GradientColorsEdit::RemoveStop { stop, .. } => {
            let mut sample = g.sampled_node(node, frame);
            ShapeGradient::remove_stop(&mut sample, *stop)?;
            let colors = sample.kind.gradient().unwrap().colors_at(&sample, frame);
            store(node, frame, colors)?;
        }
        GradientColorsEdit::MoveKey { from, to } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            if !animation.keys.contains_key(from) {
                return Err("Gradient Colors key no longer exists".into());
            }
            if from == to {
                return Ok(());
            }
            if animation.keys.contains_key(to) {
                return Err("Gradient Colors destination already has a key".into());
            }
            let colors = animation.keys.remove(from).unwrap();
            animation.keys.insert(*to, colors);
            if let Some(mode) = animation.outgoing_interpolation.remove(from) {
                animation.outgoing_interpolation.insert(*to, mode);
            }
        }
        GradientColorsEdit::SetInterpolation { interpolation, .. } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            if !animation.keys.contains_key(&frame) {
                return Err("Gradient Colors key no longer exists".into());
            }
            if *interpolation == GradientColorsInterpolation::Hold {
                animation.outgoing_interpolation.remove(&frame);
            } else {
                animation
                    .outgoing_interpolation
                    .insert(frame, *interpolation);
            }
        }
        GradientColorsEdit::DeleteKey { .. } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            if animation.keys.remove(&frame).is_none() {
                return Err("Gradient Colors key no longer exists".into());
            }
            animation.outgoing_interpolation.remove(&frame);
            if animation.keys.is_empty() {
                bake(node, &colors);
            }
        }
        GradientColorsEdit::MoveKeys { frames, to } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            animation.validate_selection(frames)?;
            let first = *frames.first().unwrap();
            let destinations = frames
                .iter()
                .map(|from| {
                    let destination = to
                        .checked_add(from - first)
                        .filter(|destination| *destination < duration)
                        .ok_or("Gradient Colors key is outside the composition")?;
                    if animation.keys.contains_key(&destination) && !frames.contains(&destination) {
                        return Err("Gradient Colors destination already has a key".into());
                    }
                    Ok(destination)
                })
                .collect::<Result<Vec<_>, String>>()?;
            if first == *to {
                return Ok(());
            }
            // Remove every source before inserting any destination, including
            // sparse modes. Otherwise overlapping moves can erase later keys.
            let moved = frames
                .iter()
                .zip(destinations)
                .map(|(from, destination)| {
                    (
                        destination,
                        animation.keys.remove(from).unwrap(),
                        animation.outgoing_interpolation.remove(from),
                    )
                })
                .collect::<Vec<_>>();
            for (destination, colors, mode) in moved {
                animation.keys.insert(destination, colors);
                if let Some(mode) = mode {
                    animation.outgoing_interpolation.insert(destination, mode);
                }
            }
        }
        GradientColorsEdit::ScaleKeys { frames, to } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            let destinations = animation.scaled_key_frames(frames, *to, duration)?;
            if frames.last() == Some(to) {
                return Ok(());
            }
            // Remove all selected snapshots and sparse modes before inserting
            // any destination, so a selected-old overlap cannot erase a key.
            let moved = destinations
                .into_iter()
                .map(|(from, destination)| {
                    (
                        destination,
                        animation.keys.remove(&from).unwrap(),
                        animation.outgoing_interpolation.remove(&from),
                    )
                })
                .collect::<Vec<_>>();
            for (destination, colors, mode) in moved {
                animation.keys.insert(destination, colors);
                if let Some(mode) = mode {
                    animation.outgoing_interpolation.insert(destination, mode);
                }
            }
        }
        GradientColorsEdit::DeleteKeys { frames, .. } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            animation.validate_selection(frames)?;
            for frame in frames {
                animation.keys.remove(frame);
                animation.outgoing_interpolation.remove(frame);
            }
            if animation.keys.is_empty() {
                bake(node, &colors);
            }
        }
        GradientColorsEdit::SetInterpolations {
            frames,
            interpolation,
        } => {
            let animation = node
                .kind
                .gradient_mut()
                .unwrap()
                .colors_animation
                .as_mut()
                .ok_or("Enable Gradient Colors animation first")?;
            animation.validate_selection(frames)?;
            for frame in frames {
                if *interpolation == GradientColorsInterpolation::Hold {
                    animation.outgoing_interpolation.remove(frame);
                } else {
                    animation
                        .outgoing_interpolation
                        .insert(*frame, *interpolation);
                }
            }
        }
        GradientColorsEdit::PasteKeys { keys, .. } => {
            let animation = g
                .colors_animation
                .as_ref()
                .ok_or("Enable Gradient Colors animation first")?;
            if keys.is_empty() || keys.len() > GradientColorsAnimation::MAX_KEYS {
                return Err("Paste 1–1000 Gradient Colors keys".into());
            }
            let mut offsets = BTreeSet::new();
            let mut roles = BTreeMap::new();
            g.legacy_colors_at(node, 0).validate(&mut roles)?;
            for colors in animation.keys.values() {
                colors.validate(&mut roles)?;
            }
            let mut next_stop = g.next_stop;
            let mut destinations = Vec::with_capacity(keys.len());
            for key in keys {
                if !offsets.insert(key.offset) {
                    return Err("Gradient Colors paste offsets must be unique".into());
                }
                let destination = frame
                    .checked_add(key.offset)
                    .filter(|destination| *destination < duration)
                    .ok_or("Gradient Colors key is outside the composition")?;
                key.colors.validate(&mut roles)?;
                next_stop = next_stop.max(
                    key.colors
                        .max_id()
                        .checked_add(1)
                        .ok_or("Gradient stop ID exhausted")?,
                );
                destinations.push(destination);
            }
            if offsets.first() != Some(&0) {
                return Err("Gradient Colors paste offsets must start at zero".into());
            }
            if destinations.iter().zip(keys).all(|(destination, key)| {
                animation.keys.get(destination) == Some(&key.colors)
                    && animation.interpolation(*destination) == Some(key.interpolation)
            }) {
                return Ok(());
            }
            if destinations
                .iter()
                .any(|destination| animation.keys.contains_key(destination))
            {
                return Err("Gradient Colors destination already has a key".into());
            }
            if animation.keys.len() + keys.len() > GradientColorsAnimation::MAX_KEYS {
                return Err("Gradient Colors supports at most 1000 keys".into());
            }
            let g = node.kind.gradient_mut().unwrap();
            g.next_stop = next_stop;
            let animation = g.colors_animation.as_mut().unwrap();
            for (destination, key) in destinations.into_iter().zip(keys) {
                animation.keys.insert(destination, key.colors.clone());
                if key.interpolation != GradientColorsInterpolation::Hold {
                    animation
                        .outgoing_interpolation
                        .insert(destination, key.interpolation);
                }
            }
        }
    }
    Ok(())
}

impl ShapeContents {
    pub(super) fn map_gradient_frames(
        &mut self,
        map: impl Fn(Frame) -> Result<Frame, String>,
    ) -> Result<(), String> {
        fn walk(
            nodes: &mut [ContentsNode],
            map: &impl Fn(Frame) -> Result<Frame, String>,
        ) -> Result<(), String> {
            for node in nodes {
                if let Some(animation) = node
                    .kind
                    .gradient_mut()
                    .and_then(|g| g.colors_animation.as_mut())
                {
                    animation.validate_interpolation()?;
                    let mut keys = BTreeMap::new();
                    let mut outgoing_interpolation = BTreeMap::new();
                    for (&frame, colors) in &animation.keys {
                        let mapped = map(frame)?;
                        if keys.insert(mapped, colors.clone()).is_some() {
                            return Err("Destination frame rate merges Gradient Colors keys".into());
                        }
                        if let Some(mode) = animation.outgoing_interpolation.get(&frame) {
                            outgoing_interpolation.insert(mapped, *mode);
                        }
                    }
                    animation.keys = keys;
                    animation.outgoing_interpolation = outgoing_interpolation;
                }
                if let ContentsKind::Group(children) = &mut node.kind {
                    walk(children, map)?;
                }
            }
            Ok(())
        }
        walk(&mut self.items, &map)
    }
}
pub(super) fn required_version(project: &Project) -> Option<u32> {
    project
        .compositions()
        .into_iter()
        .flat_map(|(_, c)| &c.layers)
        .filter_map(|l| {
            let Content::ShapeContents(contents) = &l.content else {
                return None;
            };
            contents
                .rows()
                .iter()
                .filter_map(|(_, _, node)| {
                    node.kind
                        .gradient()?
                        .colors_animation()
                        .map(GradientColorsAnimation::required_version)
                })
                .max()
        })
        .max()
}
pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::Contents {
            edit: ContentsEdit::GradientColors { .. },
            ..
        } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}

/// The new exact-key operations must not fall through a mixed batch into the
/// legacy generic migration route. Keep historical commands' batching intact.
pub(super) fn validate_new_edit_batch(command: &Command) -> Result<(), String> {
    let mut stack = vec![(command, 0usize)];
    let (mut found, mut pure, mut count, mut max_depth) = (false, true, 0usize, 0usize);
    while let Some((command, depth)) = stack.pop() {
        count = count.saturating_add(1);
        max_depth = max_depth.max(depth);
        match command {
            Command::Contents {
                edit: ContentsEdit::GradientColors { edit, .. },
                ..
            } => {
                found |= matches!(
                    edit,
                    GradientColorsEdit::SetInterpolation { .. }
                        | GradientColorsEdit::DeleteKey { .. }
                        | GradientColorsEdit::MoveKeys { .. }
                        | GradientColorsEdit::ScaleKeys { .. }
                        | GradientColorsEdit::DeleteKeys { .. }
                        | GradientColorsEdit::SetInterpolations { .. }
                        | GradientColorsEdit::PasteKeys { .. }
                );
            }
            Command::Batch(commands) if !commands.is_empty() => {
                stack.extend(commands.iter().map(|command| (command, depth + 1)));
            }
            _ => pure = false,
        }
    }
    if found && (!pure || count > 10000 || max_depth > 64) {
        return Err("Gradient Colors key operations require a bounded, nonempty Gradient Colors-only transaction".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "gradient_colors_scale_tests.rs"]
mod scale_tests;
