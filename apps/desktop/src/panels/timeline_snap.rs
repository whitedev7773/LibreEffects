//! Screen-distance snapping in composition frames; a single offset preserves selection spacing.
use libre_effects_core::{Composition, KeyRef, LayerId};
use std::collections::BTreeSet;

pub(super) fn targets(
    comp: &Composition,
    playhead: Option<u32>,
    moving_layers: &BTreeSet<LayerId>,
    moving_keys: &BTreeSet<KeyRef>,
) -> Vec<i64> {
    let mut frames: BTreeSet<i64> = [
        0,
        i64::from(comp.duration()),
        i64::from(comp.work_area().start),
        i64::from(comp.work_area().end),
    ]
    .into();
    if let Some(playhead) = playhead {
        frames.insert(i64::from(playhead));
    }
    for m in comp.markers() {
        frames.extend([i64::from(m.frame()), i64::from(m.end())]);
    }
    for l in comp.layers() {
        if moving_layers.contains(&l.id()) || (comp.hide_shy() && l.shy()) {
            continue;
        }
        frames.extend([
            i64::from(l.in_frame()),
            i64::from(l.out_frame(comp.duration())),
        ]);
        for m in l.markers() {
            frames.extend([i64::from(m.frame()), i64::from(m.end())]);
        }
        for path in l.track_paths() {
            for frame in l.track(path).into_iter().flat_map(|t| t.keys().keys()) {
                if !moving_keys.contains(&KeyRef {
                    id: l.id(),
                    property: path,
                    frame: *frame,
                }) {
                    frames.insert(i64::from(*frame));
                }
            }
        }
    }
    frames.into_iter().collect()
}

pub(super) fn snap_delta(
    delta: f64,
    anchors: &[i64],
    targets: &[i64],
    visible: u32,
    width: f32,
    limits: (i64, i64),
) -> (i64, Option<u32>) {
    if width <= 0.0 || !width.is_finite() || limits.0 > limits.1 {
        return (delta.round() as i64, None);
    }
    let tolerance = 8.0 * f64::from(visible) / f64::from(width);
    let mut best: Option<(f64, i64, i64)> = None;
    for anchor in anchors {
        let wanted = *anchor as f64 + delta;
        let at = targets.partition_point(|t| (*t as f64) < wanted);
        for target in targets
            .get(at)
            .into_iter()
            .chain(at.checked_sub(1).and_then(|n| targets.get(n)))
        {
            let adjusted = *target - *anchor;
            let distance = (adjusted as f64 - delta).abs();
            if distance <= tolerance
                && adjusted >= limits.0
                && adjusted <= limits.1
                && best.is_none_or(|b| (distance, *target) < (b.0, b.2))
            {
                best = Some((distance, adjusted, *target));
            }
        }
    }
    best.map_or((delta.round() as i64, None), |(_, d, t)| {
        (d, u32::try_from(t).ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapping_uses_screen_distance_and_a_shared_offset_for_multiple_anchors() {
        assert_eq!(
            snap_delta(9.0, &[20, 80], &[30, 100], 150, 1000.0, (-20, 50)),
            (10, Some(30))
        );
        assert_eq!(
            snap_delta(8.0, &[20, 80], &[30, 100], 150, 1000.0, (-20, 50)),
            (8, None)
        );
        assert_eq!(
            snap_delta(9.0, &[20], &[30], 30, 1000.0, (-20, 50)),
            (9, None)
        );
        assert_eq!(
            snap_delta(-9.0, &[40], &[30], 150, 1000.0, (-40, 50)),
            (-10, Some(30))
        );
        assert_eq!(
            snap_delta(9.0, &[20], &[30], 150, 1000.0, (-20, 9)),
            (9, None)
        );
        // A pointer seven pixels from frame 45 still snaps when each frame is wider
        // than the tolerance: do not round the pointer to frame 44 beforehand.
        assert_eq!(
            snap_delta(44.4, &[0], &[45], 90, 1053.0, (0, 89)),
            (45, Some(45))
        );
        assert_eq!(
            snap_delta(44.2, &[0], &[45], 90, 1053.0, (0, 89)),
            (44, None)
        );
    }
    #[test]
    fn targets_include_markers_and_effect_keys_but_exclude_moving_keys_and_shy_layers() {
        use libre_effects_core::*;
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        e.execute(Command::Effect {
            id: 1,
            edit: EffectEdit::Add(EffectKind::GaussianBlur),
        })
        .unwrap();
        let p = PropertyPath::Effect {
            effect: 1,
            parameter: EffectParam::Radius,
        };
        e.execute(Command::EditTrack {
            id: 1,
            property: p,
            edit: TrackEdit::ToggleKey { frame: 25 },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Layer(1),
            edit: MarkerEdit::Add { frame: 55 },
        })
        .unwrap();
        e.execute(Command::Marker {
            target: MarkerTarget::Composition,
            edit: MarkerEdit::Add { frame: 65 },
        })
        .unwrap();
        let t = targets(
            e.project().composition(),
            Some(45),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(t.contains(&25) && t.contains(&55) && t.contains(&65) && t.contains(&45));
        let t = targets(
            e.project().composition(),
            Some(45),
            &BTreeSet::new(),
            &[KeyRef {
                id: 1,
                property: p,
                frame: 25,
            }]
            .into(),
        );
        assert!(!t.contains(&25));
        assert!(t.contains(&55));
        let t = targets(
            e.project().composition(),
            Some(45),
            &[1].into(),
            &BTreeSet::new(),
        );
        assert!(!t.contains(&25) && !t.contains(&55) && t.contains(&65));
        e.execute(Command::SetLayerSwitch {
            id: 1,
            switch: LayerSwitch::Shy,
            enabled: true,
        })
        .unwrap();
        e.execute(Command::SetHideShy(true)).unwrap();
        let t = targets(
            e.project().composition(),
            Some(45),
            &BTreeSet::new(),
            &BTreeSet::new(),
        );
        assert!(!t.contains(&25) && !t.contains(&55) && t.contains(&65));
    }
}
