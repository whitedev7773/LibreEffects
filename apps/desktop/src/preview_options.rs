//! Preview and time navigation policy, separate from authored composition settings.
use libre_effects_core::{Composition, FrameRate};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PreviewRange {
    #[default]
    WorkArea,
    Composition,
    AroundTime,
}
impl PreviewRange {
    pub const ALL: [Self; 3] = [Self::WorkArea, Self::Composition, Self::AroundTime];
    pub fn label(self) -> &'static str {
        match self {
            Self::WorkArea => "Work area",
            Self::Composition => "Entire composition",
            Self::AroundTime => "Around current time (±2s)",
        }
    }
}
pub(crate) fn dimension(comp: &Composition, resolution: u32) -> u32 {
    let native = comp.width().max(comp.height());
    if resolution == 0 {
        native.min(1280)
    } else {
        (native / resolution).max(1)
    }
}
pub(crate) fn range(comp: &Composition, kind: PreviewRange, current: u32) -> std::ops::Range<u32> {
    match kind {
        PreviewRange::WorkArea => comp.work_area().start..comp.work_area().end,
        PreviewRange::Composition => 0..comp.duration(),
        PreviewRange::AroundTime => {
            let radius = (comp.fps().as_f64() * 2.0).round() as u32;
            current.saturating_sub(radius)..current.saturating_add(radius + 1).min(comp.duration())
        }
    }
}
/// Bare numbers are absolute composition frames. Timecodes use the display origin.
/// +20 / -20 are relative frames; +1s and +00:00:01:00 are relative durations.
pub(crate) fn seek(value: &str, comp: &Composition, current: u32) -> Result<u32, String> {
    let value = value.trim();
    let relative = value.starts_with('+') || value.starts_with('-');
    let negative = value.starts_with('-') || value.starts_with("+-");
    let magnitude = value.trim_start_matches('+').trim_start_matches('-');
    let parsed = comp.fps().parse_duration(magnitude)?;
    let frame = if relative {
        i64::from(current)
            + if negative {
                -i64::from(parsed)
            } else {
                i64::from(parsed)
            }
    } else if magnitude.contains(':') {
        i64::from(parsed) - i64::from(comp.display_start())
    } else {
        i64::from(parsed)
    };
    if frame < 0 || frame >= i64::from(comp.duration()) {
        return Err(format!(
            "Time must be within frames 0–{}",
            comp.duration() - 1
        ));
    }
    Ok(frame as u32)
}
pub(crate) fn ruler_ticks(
    start: u32,
    visible: u32,
    fps: FrameRate,
    origin: u32,
) -> Vec<(u32, String)> {
    let nominal = fps.nominal();
    let target = visible.div_ceil(8).max(1);
    let mut steps = vec![1, 2, 5, 10, 15, 20, 30];
    steps.extend(
        [
            1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600, 7200, 21600, 86400,
        ]
        .map(|s| s * nominal),
    );
    steps.sort_unstable();
    steps.dedup();
    let step = steps.into_iter().find(|n| *n >= target).unwrap_or(target);
    let display_start = start.saturating_add(origin);
    let first = display_start.div_ceil(step) * step;
    (0..=8)
        .filter_map(|i| {
            let display = first.checked_add(i * step)?;
            let frame = display.checked_sub(origin)?;
            if frame >= start.saturating_add(visible) {
                return None;
            }
            let label = if step < nominal {
                fps.timecode(u64::from(display))
            } else {
                let seconds = display / nominal;
                if seconds >= 3600 {
                    format!(
                        "{}:{:02}:{:02}",
                        seconds / 3600,
                        seconds / 60 % 60,
                        seconds % 60
                    )
                } else {
                    format!("{}:{:02}", seconds / 60, seconds % 60)
                }
            };
            Some((frame, label))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor};
    #[test]
    fn time_input_rejects_invalid_positions_and_handles_origins_and_relative_frames() {
        let mut e = Editor::default();
        let c = e.project().composition();
        e.execute(Command::ConfigureCompositionRate {
            name: c.name().into(),
            width: c.width(),
            height: c.height(),
            fps: 60.into(),
            duration: 150,
            display_start: 3600,
        })
        .unwrap();
        let c = e.project().composition();
        assert_eq!(seek("00:01:00:12", c, 0), Ok(12));
        assert_eq!(seek("+20", c, 12), Ok(32));
        assert_eq!(seek("+-20", c, 32), Ok(12));
        assert_eq!(seek("+1s", c, 12), Ok(72));
        assert!(seek("-20", c, 12).is_err());
        assert!(seek("00:01:00:60", c, 0).is_err());
        assert!(seek("4294967296", c, 0).is_err());
        assert!(seek("", c, 0).is_err());
    }
    #[test]
    fn native_resolution_and_ruler_follow_composition_units() {
        let e = Editor::default();
        let c = e.project().composition();
        assert_eq!(dimension(c, 1), c.width().max(c.height()));
        assert_eq!(dimension(c, 2), c.width().max(c.height()) / 2);
        assert!(dimension(c, 0) <= 1280);
        assert_eq!(
            ruler_ticks(0, 36000, 60.into(), 0)[1],
            (7200, "2:00".into())
        );
        assert!(
            ruler_ticks(0, 10, 60.into(), 0)
                .iter()
                .all(|(_, label)| label.contains(":"))
        );
    }
}
