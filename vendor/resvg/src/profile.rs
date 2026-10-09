//! Opt-in, thread-local inclusive render-operation timings.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
thread_local! {
    static SAMPLES: RefCell<Option<BTreeMap<&'static str, (u64, f64)>>> = const { RefCell::new(None) };
}
/// Starts a fresh optional synchronous render profile on this thread.
pub fn reset_render_profile(enabled: bool) {
    SAMPLES.with(|slot| *slot.borrow_mut() = enabled.then(BTreeMap::new));
}
/// Inclusive milliseconds and call counts; nested operations overlap.
pub fn render_profile() -> Vec<(&'static str, u64, f64)> {
    SAMPLES.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|samples| {
                samples
                    .iter()
                    .map(|(&name, &(count, ms))| (name, count, ms))
                    .collect()
            })
            .unwrap_or_default()
    })
}
pub(crate) struct Timer(&'static str, Option<Instant>);
pub(crate) fn time(name: &'static str) -> Timer {
    Timer(
        name,
        SAMPLES.with(|slot| slot.borrow().is_some().then(Instant::now)),
    )
}
impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(start) = self.1 {
            SAMPLES.with(|slot| {
                if let Some(samples) = slot.borrow_mut().as_mut() {
                    let entry = samples.entry(self.0).or_default();
                    entry.0 += 1;
                    entry.1 += start.elapsed().as_secs_f64() * 1000.0;
                }
            });
        }
    }
}
