use super::*;
use crate::audio_playback::{Phase, Range, Session, Status};

impl EditorState {
    pub(super) fn start_playback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.stop();
        if self.frame < self.work_start || self.frame >= self.work_end {
            self.frame = self.work_start;
        }
        let comp = self.editor.project().composition();
        let range = Range::new(
            comp.fps(),
            self.work_start,
            self.work_end,
            self.frame,
            self.preview_loop,
        );
        let session = range.and_then(|range| {
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
                self.playback_origin = self
                    .audio_session
                    .is_none()
                    .then(|| (Instant::now(), self.frame));
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
        if self.playing {
            self.frame = session
                .range
                .frame(status.played)
                .min(session.range.end - 1);
        }
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
