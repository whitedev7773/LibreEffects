//! Independent signed layer origins and timeline-only indexed labels.
use super::*;

const MAX_ORIGIN: u64 = 100_000_000;

pub(super) fn content_origin(content: &Content) -> i64 {
    match content {
        Content::Video { start_frame, .. }
        | Content::Audio { start_frame, .. }
        | Content::ImageSequence { start_frame, .. }
        | Content::Composition { start_frame, .. } => *start_frame,
        _ => 0,
    }
}

impl Layer {
    /// Independent origin in composition frames. Legacy timed layers retain
    /// their source-clock origin; legacy static layers start at zero, not inPoint.
    pub fn start_frame(&self) -> i64 {
        self.start_frame
            .unwrap_or_else(|| content_origin(&self.content))
    }

    pub fn label_index(&self) -> u8 {
        self.label_index.unwrap_or(0)
    }

    /// Fixed timeline label palette. This never changes the rendered fill.
    /// None means an untouched legacy layer should keep its existing swatch.
    pub fn label_color(&self) -> Option<u32> {
        const COLORS: [u32; 17] = [
            0x808080, 0xb53838, 0xe4d84c, 0xa9cbc7, 0xe5bcc9, 0xa9a9ca, 0xe7c19e, 0xb3c7b3,
            0x677de0, 0x4aa44c, 0x8e2c9a, 0xe8920d, 0x7f452a, 0xf46dd6, 0x3da2a5, 0xa89677,
            0x1e401e,
        ];
        self.label_index
            .and_then(|index| COLORS.get(usize::from(index)).copied())
    }

    /// Retain sparse legacy storage when its fallback still expresses the same
    /// origin. Otherwise pin the logical origin before another source rebase.
    pub(super) fn preserve_start_frame(&mut self, origin: i64) {
        if self.start_frame.is_some() || origin != content_origin(&self.content) {
            self.start_frame = Some(origin);
        }
    }
}

pub(super) fn materialized(project: &Project) -> bool {
    project.compositions().into_iter().any(|(_, comp)| {
        comp.layers
            .iter()
            .any(|layer| layer.start_frame.is_some() || layer.label_index.is_some())
    })
}

pub(super) fn validate(layer: &Layer, version: u32) -> Result<(), String> {
    if (layer.start_frame.is_some() || layer.label_index.is_some()) && version < 64 {
        return Err(
            "Independent layer origins and indexed labels require project version 64".into(),
        );
    }
    if layer
        .start_frame
        .is_some_and(|origin| origin.unsigned_abs() > MAX_ORIGIN)
        || layer.label_index.is_some_and(|index| index > 16)
    {
        return Err(
            "Layer origin must be within ±100000000 frames and label index must be 0–16".into(),
        );
    }
    Ok(())
}

pub(super) fn edits_only(command: &Command) -> bool {
    match command {
        Command::SetLayerStart { .. }
        | Command::SetLayerLabel { .. }
        | Command::ShiftLayer { .. } => true,
        Command::Batch(commands) => !commands.is_empty() && commands.iter().all(edits_only),
        _ => false,
    }
}

pub(super) fn apply(state: &mut Snapshot, command: &Command) -> Option<Result<(), String>> {
    match command {
        Command::SetLayerStart { id, frame } => Some((|| {
            if frame.unsigned_abs() > MAX_ORIGIN {
                return Err("Layer origin must be within ±100000000 frames".into());
            }
            let duration = state.project.composition.duration;
            let layer = editing::editable(state, *id)?;
            if *frame == layer.start_frame() {
                return Ok(());
            }
            let delta = frame
                .checked_sub(layer.start_frame())
                .ok_or("Layer timing overflow")?;
            shift(layer, delta, duration)?;
            layer.start_frame = Some(*frame);
            Ok(())
        })()),
        Command::SetLayerLabel { id, index } => Some((|| {
            if *index > 16 {
                return Err("Layer label index must be 0–16".into());
            }
            let layer = editing::editable(state, *id)?;
            if layer.label_index() != *index {
                layer.label_index = Some(*index);
            }
            Ok(())
        })()),
        _ => None,
    }
}

/// The current frame model admits keys/ranges only inside the composition.
/// Reject an unrepresentable move as a whole, never clip keys or alias inPoint.
/// Callers apply this only to a detached candidate, so every failure is atomic.
pub(super) fn shift(layer: &mut Layer, delta: i64, duration: Frame) -> Result<(), String> {
    if delta == 0 {
        return Ok(());
    }
    let origin = layer
        .start_frame()
        .checked_add(delta)
        .filter(|origin| origin.unsigned_abs() <= MAX_ORIGIN)
        .ok_or("Layer origin must be within ±100000000 frames")?;
    let shifted = |frame: Frame, endpoint: bool| -> Result<Frame, String> {
        let next = i64::from(frame)
            .checked_add(delta)
            .ok_or("Layer timing overflow")?;
        if next < 0 || next >= i64::from(duration) + i64::from(endpoint) {
            return Err(
                "Layer move would place a range, key or marker outside the composition".into(),
            );
        }
        Ok(next as Frame)
    };
    let spatial = layer
        .spatial_position
        .as_ref()
        .map(|track| {
            let shifted_track = track
                .retimed(0.0, 1.0, delta as f64)
                .map_err(|error| error.to_string())?;
            if shifted_track.keys.keys().any(|frame| *frame >= duration) {
                return Err("Spatial key would leave the composition".to_string());
            }
            Ok(shifted_track)
        })
        .transpose()?;
    let planar = layer
        .planar_position
        .as_ref()
        .map(|track| {
            let shifted = track
                .retimed(0.0, 1.0, delta as f64)
                .map_err(|e| e.to_string())?;
            if shifted.keys.keys().any(|f| *f >= duration) {
                return Err("Planar key would leave the composition".to_string());
            }
            Ok(shifted)
        })
        .transpose()?;
    opacity_timing::shift(layer, delta, duration)?;
    if let Some(range) = &mut layer.precise_range {
        range[0] += delta as f64;
        range[1] += delta as f64;
        layer.project_precise_range(duration);
    } else {
        let end = layer.out_frame(duration);
        layer.in_frame = shifted(layer.in_frame, false)?;
        layer.out_frame = Some(shifted(end, true)?);
    }
    if let Content::Video { start_frame, .. }
    | Content::Audio { start_frame, .. }
    | Content::ImageSequence { start_frame, .. }
    | Content::Composition { start_frame, .. } = &mut layer.content
    {
        *start_frame = start_frame
            .checked_add(delta)
            .filter(|origin| origin.unsigned_abs() <= MAX_ORIGIN)
            .ok_or("Source timing exceeds ±100000000 frames")?;
    }
    layer.preserve_start_frame(origin);
    layer.spatial_position = spatial;
    layer.planar_position = planar;
    layer.markers.shift(delta, duration)?;
    if let Content::ShapeContents(contents) = &mut layer.content {
        contents.map_gradient_frames(|frame| shifted(frame, false))?;
    }
    for track in layer.all_tracks_mut() {
        track.keys = track
            .keys
            .iter()
            .map(|(frame, key)| Ok((shifted(*frame, false)?, key.clone())))
            .collect::<Result<_, String>>()?;
    }
    Ok(())
}
