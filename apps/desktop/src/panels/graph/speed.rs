use libre_effects_core::AnimatedProperty;

/// Separate strokes prevent inventing a ramp across a velocity discontinuity.
pub(super) fn curves(
    track: &AnimatedProperty,
    start: u32,
    span: u32,
    fps: f64,
) -> Vec<Vec<(f64, f64)>> {
    let end = start.saturating_add(span);
    let mut stops = vec![start];
    stops.extend(track.keys().range(start..=end).map(|(&f, _)| f));
    stops.push(end);
    stops.dedup();
    let mut result = vec![];
    for pair in stops.windows(2) {
        let (a, b) = (pair[0] as f64, pair[1] as f64);
        let count = ((b - a) / span.max(1) as f64 * 600.0)
            .ceil()
            .clamp(2.0, 600.0) as usize;
        let mut line = vec![];
        for i in 0..=count {
            let frame = a + (b - a) * i as f64 / count as f64;
            if let Some(v) = track.velocity(frame, i == count) {
                line.push((frame, v * fps));
            } else if !line.is_empty() {
                result.push(std::mem::take(&mut line));
            }
        }
        if !line.is_empty() {
            result.push(line);
        }
    }
    result
}
/// Incoming / outgoing endpoints may have different ordinates at the same key.
pub(super) fn ends(track: &AnimatedProperty, frame: u32, fps: f64) -> Vec<(bool, f64)> {
    [
        (
            false,
            track
                .keys()
                .range(frame.saturating_add(1)..)
                .next()
                .is_some(),
        ),
        (true, track.keys().range(..frame).next_back().is_some()),
    ]
    .into_iter()
    .filter(|(_, exists)| *exists)
    .filter_map(|(incoming, _)| {
        track
            .velocity(frame as f64, incoming)
            .map(|v| (incoming, v * fps))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor, Interpolation, Property};
    fn scene() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddRectangle).unwrap();
        for (frame, value) in [(0, 0.0), (30, 90.0), (60, 30.0)] {
            e.execute(Command::ToggleKeyframe {
                id: 1,
                property: Property::PositionX,
                frame,
            })
            .unwrap();
            e.execute(Command::SetValue {
                id: 1,
                property: Property::PositionX,
                frame,
                value,
            })
            .unwrap();
        }
        e
    }
    #[test]
    fn strokes_split_at_keys_and_endpoints_keep_signed_units_per_second() {
        let e = scene();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        let lines = curves(t, 0, 90, 30.0);
        assert_eq!(lines.len(), 3);
        for (line, want) in lines.iter().zip([90.0, -60.0, 0.0]) {
            assert!(line.iter().all(|(_, v)| *v == want));
        }
        assert_eq!(ends(t, 30, 30.0), vec![(false, -60.0), (true, 90.0)]);
        assert_eq!(ends(t, 60, 30.0), vec![(true, -60.0)]);
        assert_eq!(curves(t, 31, 20, 30.0).len(), 1);
    }
    #[test]
    fn hold_jumps_are_gaps_and_static_properties_have_a_zero_line() {
        let mut e = scene();
        e.execute(Command::SetInterpolation {
            id: 1,
            property: Property::PositionX,
            frame: 0,
            interpolation: Interpolation::Hold,
        })
        .unwrap();
        let t = e.selected_layer().unwrap().property(Property::PositionX);
        let lines = curves(t, 0, 60, 30.0);
        assert!(lines[0].last().unwrap().0 < 30.0);
        assert_eq!(ends(t, 30, 30.0), vec![(false, -60.0)]);
        let static_track = e.selected_layer().unwrap().property(Property::PositionY);
        assert!(
            curves(static_track, 0, 60, 30.0)[0]
                .iter()
                .all(|(_, v)| *v == 0.0)
        );
    }
}
