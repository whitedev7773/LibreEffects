use super::*;
use crate::audio_playback::{Phase, Range, Session, Status};
use std::time::Duration;

/// Waiting never creates time debt that could skip frames when rendering catches up.
#[derive(Default)]
pub(super) struct Clock {
    ready: bool,
    deadline: Option<Instant>,
    pub(super) intervals: u64,
    period: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_frame_loop_rearms_the_render_receipt_each_interval() {
        let mut state = EditorState::default();
        state.playing = true;
        state.work_start = 10;
        state.work_end = 11;
        state.frame = 10;
        for interval in 1..=5 {
            state.preview_render_receipt(10, state.playback_generation, true);
            state.tick_playback(state.playback_clock.deadline.unwrap());
            assert_eq!(state.frame, 10);
            assert_eq!(state.playback_clock.intervals, interval);
            assert!(state.preview_buffering());
            assert!(state.playing);
        }
    }

    #[test]
    fn prefetched_frames_keep_cadence_but_render_stalls_reset_it() {
        let now = Instant::now();
        let period = Duration::from_nanos(16_666_667);
        let mut clock = Clock::default();
        clock.receipt(true, now, period);
        for interval in 1..100 {
            let due = now + period * interval;
            assert!(clock.due(due));
            clock.advance(due + Duration::from_millis(2));
            clock.receipt(true, due + Duration::from_millis(2), period);
            assert_eq!(clock.deadline, Some(due + period));
        }
        let missing = now + Duration::from_secs(3);
        clock.receipt(false, missing, period);
        assert!(!clock.due(missing + Duration::from_secs(60)));
        clock.receipt(true, missing + Duration::from_secs(60), period);
        assert_eq!(
            clock.deadline,
            Some(missing + Duration::from_secs(60) + period)
        );
    }

    #[test]
    fn slow_render_waits_without_time_debt_or_frame_skips() {
        let mut state = EditorState::default();
        state.playing = true;
        state.frame = 10;
        state.work_start = 10;
        state.work_end = 14;
        state.preview_loop = false;
        let now = Instant::now();
        state.tick_playback(now + Duration::from_secs(100));
        assert_eq!(state.frame, 10);
        for expected in 10..14 {
            state.preview_render_receipt(expected, state.playback_generation, true);
            let deadline = state.playback_clock.deadline.unwrap();
            state.tick_playback(deadline - Duration::from_micros(1));
            assert_eq!(state.frame, expected);
            state.tick_playback(deadline + Duration::from_secs(10));
            assert_eq!(state.frame, (expected + 1).min(13));
            state.tick_playback(deadline + Duration::from_secs(100));
            assert_eq!(state.frame, (expected + 1).min(13));
        }
        assert!(!state.playing);
    }

    #[test]
    fn loop_and_stale_receipts_never_unlock_an_unrendered_frame() {
        let mut state = EditorState::default();
        state.playing = true;
        state.frame = 12;
        state.work_start = 10;
        state.work_end = 13;
        let generation = state.playback_generation;
        state.preview_render_receipt(11, generation, true);
        state.preview_render_receipt(12, generation.wrapping_add(1), true);
        assert!(state.preview_buffering());
        state.preview_render_receipt(12, generation, true);
        state.tick_playback(state.playback_clock.deadline.unwrap());
        assert_eq!(state.frame, 10);
        assert!(state.preview_buffering());
        state.preview_render_receipt(12, generation, true);
        assert!(state.preview_buffering());
        state.stop();
        state.preview_render_receipt(10, generation, true);
        assert!(!state.playback_clock.ready);
    }

    #[test]
    fn every_speed_has_a_full_interval_and_invalidated_frames_wait_again() {
        let now = Instant::now();
        for quarters in [1, 2, 4, 6, 8] {
            let period = Duration::from_secs_f64(4.0 / (60.0 * f64::from(quarters)));
            let mut clock = Clock::default();
            clock.receipt(true, now, period);
            assert!(!clock.due(now + period / 2));
            clock.receipt(true, now + period / 2, period);
            assert!(clock.due(now + period));
            clock.receipt(false, now + period, period);
            assert!(!clock.due(now + Duration::from_secs(5)));
            let resumed = now + Duration::from_secs(5);
            clock.receipt(true, resumed, period);
            assert!(!clock.due(resumed));
            assert!(clock.due(resumed + period));
        }
    }
}
impl Clock {
    fn receipt(&mut self, ready: bool, now: Instant, period: Duration) {
        self.period = period;
        self.ready = ready;
        if !ready {
            self.deadline = None;
        } else if self.deadline.is_none() {
            self.deadline = Some(now + period);
        }
    }
    fn due(&self, now: Instant) -> bool {
        self.ready && self.deadline.is_some_and(|deadline| now >= deadline)
    }
    fn advance(&mut self, now: Instant) {
        self.intervals += 1;
        self.ready = false;
        // Preserve cadence when the next frame is already prefetched. A missing
        // receipt clears this deadline; a delayed UI tick cannot create catch-up debt.
        self.deadline = self.deadline.map(|previous| {
            let next = previous + self.period;
            if next <= now { now + self.period } else { next }
        });
    }
}

impl EditorState {
    pub(crate) fn preview_buffering(&self) -> bool {
        self.playing && !self.playback_clock.ready
    }
    pub(crate) fn preview_render_receipt(&mut self, frame: Frame, generation: u64, ready: bool) {
        if !self.playing || self.frame != frame || self.playback_generation != generation {
            return;
        }
        let period = Duration::from_secs_f64(
            4.0 / (self.editor.project().composition().fps().as_f64()
                * f64::from(self.preview_speed_quarters)),
        );
        self.playback_clock.receipt(ready, Instant::now(), period);
        if ready {
            if let Some(session) = &self.audio_session {
                session.permit_frames(self.playback_clock.intervals + 1);
            }
        }
    }
    pub(super) fn tick_playback(&mut self, now: Instant) {
        if !self.playback_clock.ready {
            return;
        }
        let due = if let Some(session) = &self.audio_session {
            self.audio_status.played
                >= session
                    .range
                    .frame_boundary(self.playback_clock.intervals + 1)
        } else {
            self.playback_clock.due(now)
        };
        if due {
            let range = self
                .playback_range
                .clone()
                .unwrap_or_else(|| self.preview_frame_range());
            let end = range.end;
            if self.frame + 1 < end {
                self.frame += 1;
            } else if self.preview_loop {
                self.frame = range.start;
            } else {
                self.stop();
                return;
            }
            self.playback_clock.advance(now);
        }
    }
    pub(crate) fn start_playback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let preview_range = if self.preview_play_after_cache {
            self.playback_range
                .clone()
                .unwrap_or_else(|| self.preview_frame_range())
        } else {
            self.preview_frame_range()
        };
        self.stop();
        if self.preview_from_start || !preview_range.contains(&self.frame) {
            self.frame = preview_range.start;
        }
        self.playback_range = Some(preview_range.clone());
        let comp = self.editor.project().composition();
        let range = Range::new(
            comp.fps(),
            preview_range.start,
            preview_range.end,
            self.frame,
            self.preview_loop,
        );
        let session = range.and_then(|mut range| {
            range.speed_quarters = self.preview_speed_quarters;
            range.render_gated = true;
            if self.preview_audio {
                Session::start(self.editor.project(), range)
            } else {
                Ok(None)
            }
        });
        match session {
            Ok(session) => {
                self.audio_session = session;
                self.playing = true;
                self.audio_status = if self.audio_session.is_some() {
                    Status::default()
                } else {
                    Status {
                        phase: Phase::Ended,
                        ..Default::default()
                    }
                };
                self.schedule_frame(self.playback_generation, window, cx);
            }
            Err(error) => {
                self.status = format!("Preview audio: {error}");
                self.audio_status.phase = Phase::Failed(error);
            }
        }
    }
    pub(super) fn poll_audio(&mut self) {
        let Some(session) = &self.audio_session else {
            return;
        };
        let status = session.status();
        match &status.phase {
            Phase::Failed(error) => {
                self.status = format!("Preview audio: {error}");
                self.stop();
            }
            Phase::Ended => self.stop(),
            _ => {}
        }
        self.audio_status = status;
    }
    pub(super) fn queue_scrub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_audio && self.preview_scrub {
            self.wait_scrub(self.playback_generation, Instant::now(), window, cx);
        }
    }
    fn wait_scrub(
        &self,
        generation: u64,
        start: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.on_next_frame(window, move |state, window, cx| {
            if generation != state.playback_generation || state.playing {
                return;
            }
            if start.elapsed() < std::time::Duration::from_millis(75) {
                state.wait_scrub(generation, start, window, cx);
                return;
            }
            let comp = state.editor.project().composition();
            let result = Range::new(comp.fps(), state.frame, comp.duration(), state.frame, false)
                .and_then(|mut range| {
                    range.limit = Some(4800);
                    Session::start(state.editor.project(), range)
                });
            match result {
                Ok(Some(session)) => {
                    state.audio_session = Some(session);
                    state.audio_status = Status::default();
                    state.schedule_frame(generation, window, cx);
                }
                Ok(None) => {}
                Err(error) => {
                    state.status = format!("Audio scrub: {error}");
                    state.audio_status.phase = Phase::Failed(error);
                }
            }
            cx.notify();
        });
    }
}
