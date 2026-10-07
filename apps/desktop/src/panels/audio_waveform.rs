use crate::{
    audio::{BINS_PER_SECOND, CHUNK_SECONDS, Key, Peak, Waveforms},
    ui,
};
use gpui::{prelude::*, *};
use libre_effects_core::{FrameRate, Layer};

/// One decoder per timeline, shared across all instances and compositions.
pub(super) struct AudioWaveforms {
    cache: Waveforms,
}
impl AudioWaveforms {
    pub fn new() -> Self {
        Self {
            cache: Waveforms::default(),
        }
    }
    pub fn row(
        &mut self,
        layer: &Layer,
        fps: FrameRate,
        start: f64,
        end: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((path, audio)) = layer.content().audio() else {
            return div().into_any_element();
        };
        let key = Key::source(path, audio);
        let mut request = None;
        let mut columns = Vec::new();
        let mut message = None;
        let mut needed = std::collections::BTreeSet::new();
        // Limit each visible waveform to ten source minutes. Large zooms ask for
        // closer inspection instead of showing a misleading sparse overview.
        for x in 0..512 {
            let a = layer
                .audio_source_seconds(start + (end - start) * f64::from(x) / 512.0, fps)
                .unwrap();
            let b = layer
                .audio_source_seconds(start + (end - start) * f64::from(x + 1) / 512.0, fps)
                .unwrap();
            let lo = a.min(b).max(0.0);
            let hi = a.max(b).min(audio.duration - 1e-9);
            if lo > hi {
                columns.push(Some(Peak::default()));
                continue;
            }
            let first = (lo / CHUNK_SECONDS).floor() as u32;
            let last = (hi / CHUNK_SECONDS).floor() as u32;
            if last.saturating_sub(first) > 60 {
                message = Some("Zoom in for waveform".to_owned());
                break;
            }
            let mut peak = Peak::default();
            let mut ready = true;
            for chunk in first..=last {
                needed.insert(chunk);
                if needed.len() > 60 {
                    message = Some("Zoom in for waveform".to_owned());
                    break;
                }
                let key = key.chunk(chunk);
                match self.cache.get(&key) {
                    Some(Ok(bins)) => {
                        let offset = f64::from(chunk) * CHUNK_SECONDS;
                        let from =
                            ((lo - offset).max(0.0) * BINS_PER_SECOND as f64).floor() as usize;
                        let to = (((hi - offset).max(0.0) * BINS_PER_SECOND as f64).floor()
                            as usize
                            + 1)
                        .min(bins.len());
                        for bin in bins.get(from..to).unwrap_or(&[]) {
                            peak.min = peak.min.min(bin.min);
                            peak.max = peak.max.max(bin.max);
                        }
                    }
                    Some(Err(error)) => {
                        message = Some(format!("Waveform unavailable: {error}"));
                        break;
                    }
                    None => {
                        ready = false;
                        if request.is_none() {
                            request = Some(key);
                        }
                    }
                }
            }
            if message.is_some() {
                break;
            }
            columns.push(ready.then_some(peak));
        }
        if message.is_none() && self.cache.pending.is_none() {
            if let Some(key) = request {
                self.cache.pending = Some(key.clone());
                let (path, audio) = (path.to_owned(), audio.clone());
                cx.spawn(async move |entity, cx| {
                    let chunk = key.chunk;
                    let result = cx
                        .background_executor()
                        .spawn(async move { crate::audio::decode_chunk(&path, &audio, chunk) })
                        .await;
                    let _ = entity.update(cx, |this, cx| {
                        this.cache.insert(key, result);
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        if let Some(message) = message {
            return div()
                .absolute()
                .size_full()
                .px_2()
                .text_size(px(10.0))
                .text_color(rgb(ui::TEXT))
                .overflow_hidden()
                .child(message)
                .into_any_element();
        }
        let loading = columns.iter().any(Option::is_none);
        div()
            .absolute()
            .size_full()
            .when(loading, |d| {
                d.child(
                    div()
                        .px_2()
                        .text_size(px(10.0))
                        .text_color(rgb(ui::TEXT))
                        .child("Loading waveform…"),
                )
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let width = f32::from(bounds.size.width) / 512.0;
                        let height = f32::from(bounds.size.height);
                        for (x, value) in columns.iter().enumerate() {
                            let Some(value) = value else {
                                continue;
                            };
                            let top = height * (0.5 - value.max.clamp(-1.0, 1.0) * 0.45);
                            let bottom = height * (0.5 - value.min.clamp(-1.0, 1.0) * 0.45);
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        bounds.left() + px(x as f32 * width),
                                        bounds.top() + px(top),
                                    ),
                                    size(px(width.max(1.0)), px((bottom - top).max(1.0))),
                                ),
                                rgb(0x172b38),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }
}
