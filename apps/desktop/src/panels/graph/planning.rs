//! Atomic, full-identity graph plans. Mixed-lane operations affect time only.
//! Detached previews never touch the live editor, assets, schema or Undo/Redo.
use super::*;
use crate::view_state::GraphChannel;
use libre_effects_core::{Editor, KeyCopy, KeyRef, KeyScale, Project};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(super) fn channel(key: KeyRef) -> GraphChannel {
    GraphChannel {
        id: key.id,
        property: key.property,
    }
}
pub(super) fn time_bounds(keys: &[KeyRef]) -> Option<(u32, u32)> {
    Some((
        keys.iter().map(|k| k.frame).min()?,
        keys.iter().map(|k| k.frame).max()?,
    ))
}
pub(super) fn multiple_channels(keys: &[KeyRef]) -> bool {
    keys.first()
        .is_some_and(|first| keys.iter().any(|key| channel(*key) != channel(*first)))
}
fn validate(project: &Project, keys: &[KeyRef]) -> Result<Vec<KeyRef>, String> {
    if keys.is_empty() {
        return Err("Select graph keyframes first".into());
    }
    let keys: Vec<_> = keys
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for key in &keys {
        if matches!(key.property, PropertyPath::Path(_)) {
            return Err("Geometry paths are not scalar graph channels".into());
        }
        let layer = project
            .composition()
            .layer(key.id)
            .ok_or("Selected layer no longer exists")?;
        if layer.locked() {
            return Err("Unlock every selected graph layer before editing keys".into());
        }
        if key.frame >= project.composition().duration()
            || !layer
                .track(key.property)
                .is_some_and(|t| t.keys().contains_key(&key.frame))
        {
            return Err("Selected graph key no longer exists".into());
        }
    }
    Ok(keys)
}

#[derive(Clone)]
pub(super) struct EditPlan {
    pub command: Option<Command>,
    pub keys: Vec<KeyRef>,
    pub active: Option<KeyRef>,
    pub tracks: BTreeMap<GraphChannel, AnimatedProperty>,
}
impl EditPlan {
    fn build(
        project: &Project,
        source: &[KeyRef],
        command: Option<Command>,
        keys: Vec<KeyRef>,
        active: Option<KeyRef>,
    ) -> Result<Self, String> {
        let source = validate(project, source)?;
        let channels: BTreeSet<_> = source.iter().copied().map(channel).collect();
        let mut preview = Editor::default();
        if let Some(command) = &command {
            preview.replace_project(project.clone())?;
            preview.execute(command.clone())?;
        }
        let result = if command.is_some() {
            preview.project()
        } else {
            project
        };
        let tracks: BTreeMap<_, _> = channels
            .into_iter()
            .map(|c| {
                (
                    c,
                    result
                        .composition()
                        .layer(c.id)
                        .unwrap()
                        .track(c.property)
                        .unwrap()
                        .clone(),
                )
            })
            .collect();
        // A generic core command may migrate metadata even when its tracks did
        // not change. Never dispatch it for a graph no-op (including a drag back).
        let changed = tracks.iter().any(|(c, track)| {
            project.composition().layer(c.id).unwrap().track(c.property) != Some(track)
        });
        Ok(Self {
            command: command.filter(|_| changed),
            keys,
            active,
            tracks,
        })
    }
    pub fn translate(
        project: &Project,
        keys: &[KeyRef],
        delta: i64,
        active: Option<KeyRef>,
    ) -> Result<Self, String> {
        let source = validate(project, keys)?;
        let duration = project.composition().duration();
        let moved = source
            .iter()
            .map(|key| {
                let to = i64::from(key.frame)
                    .checked_add(delta)
                    .ok_or("Key time is outside the composition")?;
                if to < 0 || to >= i64::from(duration) {
                    return Err("Key time is outside the composition".to_string());
                }
                Ok(KeyRef {
                    frame: to as u32,
                    ..*key
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let active = remap_active(&source, &moved, active);
        Self::build(
            project,
            &source,
            (delta != 0).then(|| Command::MoveKeys {
                keys: source.clone(),
                delta,
            }),
            moved,
            active,
        )
    }
    pub fn scale_time(
        project: &Project,
        keys: &[KeyRef],
        origin: f64,
        factor: f64,
        active: Option<KeyRef>,
    ) -> Result<Self, String> {
        let source = validate(project, keys)?;
        let (first, last) = time_bounds(&source).unwrap();
        if first == last {
            return Err("Select keys at two distinct times to scale time".into());
        }
        let scale = KeyScale {
            time_origin: origin,
            time_scale: factor,
            value_origin: 0.0,
            value_scale: 1.0,
        };
        let moved = source
            .iter()
            .map(|key| {
                Ok(KeyRef {
                    frame: scale.frame(key.frame, project.composition().duration())?,
                    ..*key
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let active = remap_active(&source, &moved, active);
        Self::build(
            project,
            &source,
            (factor != 1.0).then(|| Command::ScaleKeys {
                keys: source.clone(),
                scale,
            }),
            moved,
            active,
        )
    }
    /// Value scaling is deliberately lane-local; group edits never broadcast
    /// absolute values or a common ordinate pivot across incompatible units.
    pub fn scale_value(
        project: &Project,
        keys: &[KeyRef],
        factor: f64,
        active: Option<KeyRef>,
    ) -> Result<Self, String> {
        let source = validate(project, keys)?;
        if source.len() < 2 {
            return Err("Select at least two keys to scale values".into());
        }
        if multiple_channels(&source) {
            return Err("Select one channel to scale values".into());
        }
        if !factor.is_finite() {
            return Err("Value scale must be finite".into());
        }
        let track = project
            .composition()
            .layer(source[0].id)
            .unwrap()
            .track(source[0].property)
            .unwrap();
        let scale = KeyScale {
            time_origin: 0.0,
            time_scale: 1.0,
            value_origin: source
                .iter()
                .map(|key| track.keys()[&key.frame].value)
                .fold(f64::INFINITY, f64::min),
            value_scale: factor,
        };
        Self::build(
            project,
            &source,
            (factor != 1.0).then(|| Command::ScaleKeys {
                keys: source.clone(),
                scale,
            }),
            source.clone(),
            active.filter(|key| source.contains(key)),
        )
    }
    /// Paste uses the core clipboard addressing rules, narrowed to channels
    /// already included in Graph. Copied source keys may have been cut/deleted.
    pub fn paste(state: &EditorState) -> Result<Self, String> {
        Self::paste_copies(
            state.editor.project(),
            state.graph_clipboard(),
            &state.graph_included_channels(),
            state.graph_active_channel(),
            state.frame,
            state.graph_key,
            &selection::included(state),
        )
    }
    fn paste_copies(
        project: &Project,
        copies: &[KeyCopy],
        included: &[GraphChannel],
        active_channel: Option<GraphChannel>,
        frame: u32,
        active_key: Option<KeyRef>,
        current_selection: &[KeyRef],
    ) -> Result<Self, String> {
        if copies.is_empty() {
            return Ok(Self {
                command: None,
                keys: current_selection.to_vec(),
                active: active_key,
                tracks: BTreeMap::new(),
            });
        }
        let single_source = copies
            .iter()
            .map(|copy| copy.key.id)
            .collect::<BTreeSet<_>>()
            .len()
            == 1;
        let target = if single_source {
            Some(
                active_channel
                    .ok_or("Activate a destination Graph channel before pasting")?
                    .id,
            )
        } else {
            None
        };
        let first = copies.iter().map(|copy| copy.key.frame).min().unwrap();
        let mut destinations = Vec::new();
        let mut target_channels = BTreeSet::new();
        for copy in copies {
            let destination = KeyRef {
                id: target.unwrap_or(copy.key.id),
                property: copy.key.property,
                frame: frame
                    .checked_add(copy.key.frame - first)
                    .filter(|&frame| frame < project.composition().duration())
                    .ok_or("Pasted key time is outside the composition")?,
            };
            let lane = channel(destination);
            if !included.contains(&lane) || !lane.available(project.composition()) {
                return Err(
                    "Pin or activate every scalar destination channel in Graph before pasting"
                        .into(),
                );
            }
            let layer = project
                .composition()
                .layer(lane.id)
                .ok_or("Paste destination layer no longer exists")?;
            if layer.locked() {
                return Err("Unlock every destination Graph layer before pasting".into());
            }
            target_channels.insert(lane);
            destinations.push(destination);
        }
        let active = active_key
            .and_then(|key| {
                copies
                    .iter()
                    .position(|copy| copy.key == key)
                    .map(|i| destinations[i])
            })
            .or_else(|| {
                destinations
                    .iter()
                    .copied()
                    .find(|key| Some(channel(*key)) == active_channel)
            })
            .or_else(|| destinations.first().copied());
        let command = Command::PasteKeys {
            keys: copies.to_vec(),
            frame,
            target,
        };
        let mut preview = Editor::default();
        preview.replace_project(project.clone())?;
        // This preserves all core no-overwrite, matching-effect, value/handle,
        // lock and composition-range validation as one detached transaction.
        preview.execute(command.clone())?;
        let tracks: BTreeMap<_, _> = target_channels
            .into_iter()
            .map(|lane| {
                (
                    lane,
                    preview
                        .project()
                        .composition()
                        .layer(lane.id)
                        .unwrap()
                        .track(lane.property)
                        .unwrap()
                        .clone(),
                )
            })
            .collect();
        let changed = tracks.iter().any(|(lane, track)| {
            project
                .composition()
                .layer(lane.id)
                .unwrap()
                .track(lane.property)
                != Some(track)
        });
        Ok(Self {
            command: changed.then_some(command),
            keys: destinations
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            active,
            tracks,
        })
    }
    pub fn delete(project: &Project, keys: &[KeyRef]) -> Result<Self, String> {
        Self::build(
            project,
            keys,
            Some(Command::DeleteKeys(keys.to_vec())),
            vec![],
            None,
        )
    }
    pub fn interpolation(
        project: &Project,
        keys: &[KeyRef],
        interpolation: Interpolation,
    ) -> Result<Self, String> {
        let edits = keys
            .iter()
            .map(|key| Command::EditTrack {
                id: key.id,
                property: key.property,
                edit: TrackEdit::Interpolate {
                    frame: key.frame,
                    interpolation,
                },
            })
            .collect();
        Self::build(
            project,
            keys,
            Some(Command::Batch(edits)),
            keys.to_vec(),
            None,
        )
    }
    pub fn temporal_mode(
        project: &Project,
        keys: &[KeyRef],
        mode: TemporalMode,
    ) -> Result<Self, String> {
        let edits = keys
            .iter()
            .map(|key| Command::SetTemporalMode {
                id: key.id,
                property: key.property,
                frame: key.frame,
                mode,
            })
            .collect();
        Self::build(
            project,
            keys,
            Some(Command::Batch(edits)),
            keys.to_vec(),
            None,
        )
    }
    pub fn ease(
        project: &Project,
        keys: &[KeyRef],
        incoming: bool,
        outgoing: bool,
    ) -> Result<Self, String> {
        Self::build(
            project,
            keys,
            super::super::key_easing::selected(project, keys, incoming, outgoing)?,
            keys.to_vec(),
            None,
        )
    }
}
fn remap_active(source: &[KeyRef], moved: &[KeyRef], active: Option<KeyRef>) -> Option<KeyRef> {
    active.and_then(|active| {
        source
            .iter()
            .position(|key| *key == active)
            .map(|i| moved[i])
    })
}

/// Complete mouse-down context shared by scalar and multi-lane graph drafts.
#[derive(Clone)]
pub(super) struct FrozenContext {
    pub source: Arc<Project>,
    revision: u64,
    selected: BTreeSet<KeyRef>,
    active_key: Option<KeyRef>,
    primary: Option<LayerId>,
    active: Option<GraphChannel>,
    included: Vec<GraphChannel>,
    channels: crate::view_state::GraphChannels,
    view: crate::view_state::GraphView,
    time: (u32, f32),
    frame: u32,
    tool: Tool,
    transport: u64,
    graph_open: bool,
    bounds: Bounds<Pixels>,
}
impl FrozenContext {
    pub fn new(state: &EditorState, bounds: Bounds<Pixels>) -> Result<Self, String> {
        if !Self::eligible(state) {
            return Err("Finish the active editor operation before editing graph keys".into());
        }
        Ok(Self {
            source: Arc::new(state.editor.project().clone()),
            revision: state.document_revision,
            selected: state.selected_keys.clone(),
            active_key: state.graph_key,
            primary: state.editor.selected(),
            active: state.graph_active_channel(),
            included: state.graph_included_channels(),
            channels: state.graph_channels.clone(),
            view: state.graph_view.clone(),
            time: (state.timeline_start, state.timeline_zoom),
            frame: state.frame,
            tool: state.tool,
            transport: state.transport_generation(),
            graph_open: state.graph_open,
            bounds,
        })
    }
    fn eligible(state: &EditorState) -> bool {
        !state.playing
            && !state.fonts_open
            && !state.media_open
            && !state.queue_open
            && state.colors.session.is_none()
            && state.gradient_editor.is_none()
            && state.vertex_editor.is_none()
            && state.text_session.is_none()
            && !state.new_composition_requested
    }
    pub fn current(&self, state: &EditorState, bounds: Option<Bounds<Pixels>>) -> bool {
        Self::eligible(state)
            && self.source.as_ref() == state.editor.project()
            && self.revision == state.document_revision
            && self.selected == state.selected_keys
            && self.active_key == state.graph_key
            && self.primary == state.editor.selected()
            && self.active == state.graph_active_channel()
            && self.included == state.graph_included_channels()
            && self.channels == state.graph_channels
            && self.view == state.graph_view
            && self.time == (state.timeline_start, state.timeline_zoom)
            && self.frame == state.frame
            && self.tool == state.tool
            && self.transport == state.transport_generation()
            && self.graph_open == state.graph_open
            && bounds == Some(self.bounds)
    }
}

#[derive(Clone)]
pub(super) struct TimeGesture {
    pub context: FrozenContext,
    pub preview: Result<EditPlan, String>,
    pub guides: snapping::Guides,
    pub moved: bool,
    keys: Vec<KeyRef>,
    start: Point<Pixels>,
    view: View,
    bounds: Bounds<Pixels>,
    edge: Option<i8>,
    snap: bool,
    targets: Vec<i64>,
    occupied: BTreeSet<KeyRef>,
}
impl TimeGesture {
    pub fn new(
        state: &EditorState,
        view: View,
        bounds: Bounds<Pixels>,
        start: Point<Pixels>,
        edge: Option<i8>,
    ) -> Result<Self, String> {
        let keys = selection::included(state);
        let project = state.editor.project();
        validate(project, &keys)?;
        let (first, last) = time_bounds(&keys).unwrap();
        if edge.is_some() && first == last {
            return Err("Select keys at two distinct times to scale time".into());
        }
        let selected: BTreeSet<_> = keys.iter().copied().collect();
        let mut targets = super::super::timeline_snap::targets(
            project.composition(),
            Some(state.frame),
            &BTreeSet::new(),
            &selected,
        );
        let included = channels::all_keys(project, &state.graph_included_channels());
        targets.extend(included.difference(&selected).map(|k| i64::from(k.frame)));
        targets.sort_unstable();
        targets.dedup();
        let occupied = included.difference(&selected).copied().collect();
        Ok(Self {
            context: FrozenContext::new(state, bounds)?,
            preview: EditPlan::translate(project, &keys, 0, state.graph_key),
            guides: Default::default(),
            moved: false,
            keys,
            start,
            view,
            bounds,
            edge,
            snap: state.snapping,
            targets,
            occupied,
        })
    }
    /// A clicked key may seek before the draft is created. Keep snapping tied to
    /// the mouse-down playhead while guarding the post-click selection/context.
    pub fn set_snap_playhead(&mut self, frame: u32) {
        let selected: BTreeSet<_> = self.keys.iter().copied().collect();
        self.targets = super::super::timeline_snap::targets(
            self.context.source.composition(),
            Some(frame),
            &BTreeSet::new(),
            &selected,
        );
        self.targets
            .extend(self.occupied.iter().map(|key| i64::from(key.frame)));
        self.targets.sort_unstable();
        self.targets.dedup();
    }
    pub fn is_current(&self, state: &EditorState, bounds: Option<Bounds<Pixels>>) -> bool {
        self.context.current(state, bounds)
    }
    fn valid_destinations(&self, mut frame: impl FnMut(u32) -> Option<u32>) -> bool {
        let mut seen = BTreeSet::new();
        self.keys.iter().all(|key| {
            frame(key.frame).is_some_and(|frame| {
                let to = KeyRef { frame, ..*key };
                !self.occupied.contains(&to) && seen.insert(to)
            })
        })
    }
    pub fn update_pointer(&mut self, end: Point<Pixels>, center: bool, control: bool) {
        let dx = f32::from(end.x - self.start.x) as f64;
        if !self.moved && dx.abs() < 3.0 {
            return;
        }
        self.moved = true;
        self.guides = Default::default();
        let raw = dx / f32::from(self.bounds.size.width).max(1.0) as f64 * self.view.span;
        let tolerance = 8.0 * self.view.span / f32::from(self.bounds.size.width).max(1.0) as f64;
        let (first, last) = time_bounds(&self.keys).unwrap();
        let project = &self.context.source;
        let duration = project.composition().duration();
        // Alt bypasses translation snapping; selection-box scaling reserves Alt
        // for its centered pivot while Ctrl still inverts the frozen snap switch.
        let enabled = (self.snap ^ control) && (self.edge.is_some() || !center);
        if let Some(edge) = self.edge {
            let origin = if center {
                (first as f64 + last as f64) / 2.0
            } else if edge < 0 {
                last as f64
            } else {
                first as f64
            };
            let edge = if edge < 0 { first as f64 } else { last as f64 };
            let wanted = edge + raw;
            let mut factor = 1.0 + raw / (edge - origin);
            if enabled && raw != 0.0 && factor.is_finite() && factor > 0.0 {
                let mut candidates: Vec<_> = self
                    .targets
                    .iter()
                    .copied()
                    .filter(|t| (*t as f64 - wanted).abs() <= tolerance)
                    .collect();
                candidates.sort_by(|a, b| {
                    (*a as f64 - wanted)
                        .abs()
                        .total_cmp(&(*b as f64 - wanted).abs())
                        .then_with(|| a.cmp(b))
                });
                for target in candidates {
                    let candidate = (target as f64 - origin) / (edge - origin);
                    let scale = KeyScale {
                        time_origin: origin,
                        time_scale: candidate,
                        value_origin: 0.0,
                        value_scale: 1.0,
                    };
                    if self.valid_destinations(|f| scale.frame(f, duration).ok())
                        && EditPlan::scale_time(
                            project,
                            &self.keys,
                            origin,
                            candidate,
                            self.context.active_key,
                        )
                        .is_ok()
                    {
                        factor = candidate;
                        self.guides.frame = u32::try_from(target).ok();
                        break;
                    }
                }
            }
            self.preview =
                EditPlan::scale_time(project, &self.keys, origin, factor, self.context.active_key);
        } else {
            let limits = (-(first as i64), duration as i64 - 1 - last as i64);
            let mut delta = (raw.round() as i64).clamp(limits.0, limits.1);
            if enabled && raw != 0.0 {
                let mut best: Option<(f64, i64, i64)> = None;
                for key in &self.keys {
                    let wanted = key.frame as f64 + raw;
                    let at = self
                        .targets
                        .partition_point(|t| (*t as f64) < wanted - tolerance);
                    for &target in self.targets[at..]
                        .iter()
                        .take_while(|t| (**t as f64) <= wanted + tolerance)
                    {
                        let candidate = target - key.frame as i64;
                        if candidate < limits.0
                            || candidate > limits.1
                            || !self
                                .valid_destinations(|f| u32::try_from(f as i64 + candidate).ok())
                        {
                            continue;
                        }
                        let distance = (candidate as f64 - raw).abs();
                        if best.is_none_or(|b| (distance, target) < (b.0, b.2)) {
                            best = Some((distance, candidate, target));
                        }
                    }
                }
                if let Some((_, candidate, target)) = best {
                    delta = candidate;
                    self.guides.frame = u32::try_from(target).ok();
                }
            }
            self.preview = EditPlan::translate(project, &self.keys, delta, self.context.active_key);
        }
        if self.preview.is_err() {
            self.guides = Default::default();
        }
    }
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
