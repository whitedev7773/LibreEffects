//! Paint-local box selection. Empty-lane click clears; Shift adds (never toggles)
//! only keys from the pressed paint. Preview and selection are transient: no
//! seek, source command, VIEW write or history entry is made, including empty
//! boxes. Empty results retain shortcut ownership until explicit navigation.
use super::*;
use gpui::{Bounds, Pixels, Point, point};

pub(super) struct PointerMarquee {
    pub(super) target: Target,
    pub(super) geometry: PointerGeometry,
    pub(super) shift: bool,
    origin: Point<Pixels>,
    end: Point<Pixels>,
    crossed: bool,
}
impl PointerMarquee {
    pub(super) fn current(
        &self,
        input: &Input,
        state: &EditorState,
        geometry: PointerGeometry,
    ) -> bool {
        self.geometry == geometry && geometry.valid(state) && input.current(&self.target, state)
    }
    fn rect(&self) -> Bounds<Pixels> {
        let lane = self.geometry.bounds;
        Bounds::from_corners(
            point(
                self.origin.x.min(self.end.x).max(lane.left()),
                self.origin.y.min(self.end.y).max(lane.top()),
            ),
            point(
                self.origin.x.max(self.end.x).min(lane.right()),
                self.origin.y.max(self.end.y).min(lane.bottom()),
            ),
        )
    }
    fn update(&mut self, position: Point<Pixels>) {
        self.crossed |= f32::from(position.x - self.origin.x).abs() >= 4.
            || f32::from(position.y - self.origin.y).abs() >= 4.;
        self.end = position;
    }
    fn frames(&self, state: &EditorState) -> BTreeSet<Frame> {
        let mut frames = if self.shift {
            self.target
                .selection
                .as_ref()
                .filter(|s| s.owns(&self.target))
                .map(|s| s.frames.clone())
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
        if !self.crossed {
            return frames;
        }
        let Some(animation) = self
            .target
            .node(state)
            .and_then(|n| n.kind.gradient())
            .and_then(|g| g.colors_animation())
        else {
            return frames;
        };
        let rect = self.rect();
        let geometry = self.geometry;
        for &frame in animation.keys().keys() {
            if frame < geometry.start || frame > geometry.start.saturating_add(geometry.visible) {
                continue;
            }
            // The 15px-wide button starts 7px before its time position, with
            // a 2px top inset and 25px height. Match its actual visible center.
            let center = point(
                geometry.bounds.left()
                    + geometry.bounds.size.width
                        * ((frame - geometry.start) as f32 / geometry.visible as f32)
                    + px(0.5),
                geometry.bounds.top() + px(14.5),
            );
            if geometry.bounds.contains(&center) && rect.contains(&center) {
                frames.insert(frame);
            }
        }
        frames
    }
}

fn finite(position: Point<Pixels>) -> bool {
    f32::from(position.x).is_finite() && f32::from(position.y).is_finite()
}

pub(super) fn modifiers_allowed(modifiers: gpui::Modifiers, shift: bool) -> bool {
    !modifiers.control
        && !modifiers.platform
        && !modifiers.function
        && !modifiers.alt
        && modifiers.shift == shift
}

impl Input {
    pub(super) fn pointer_active(&self) -> bool {
        self.pointer.is_some() || self.marquee.is_some()
    }
    pub(super) fn begin_marquee(
        &mut self,
        target: &Target,
        state: &EditorState,
        position: Point<Pixels>,
        shift: bool,
        geometry: PointerGeometry,
    ) -> bool {
        self.cancel_pointer();
        if !self.current(target, state)
            || !geometry.valid(state)
            || !finite(position)
            || !geometry.bounds.contains(&position)
            || target.owner.graph_open
            || !target.owner.expanded
            || target
                .node(state)
                .and_then(|n| n.kind.gradient())
                .and_then(|g| g.colors_animation())
                .is_none()
        {
            return false;
        }
        // Own the domain from press, even if the box never acquires a key.
        if !self.domain_owned {
            self.domain_owned = true;
            self.ownership.as_ref().unwrap().set(true);
            self.serial = self.serial.wrapping_add(1);
        }
        self.marquee = Some(PointerMarquee {
            target: self.target(target.item).unwrap(),
            geometry,
            shift,
            origin: position,
            end: position,
            crossed: false,
        });
        true
    }
    pub(super) fn update_marquee(
        &mut self,
        state: &EditorState,
        geometry: PointerGeometry,
        position: Point<Pixels>,
    ) -> bool {
        let Some(mut marquee) = self.marquee.take() else {
            return false;
        };
        if finite(position) && marquee.current(self, state, geometry) {
            marquee.update(position);
            self.marquee = Some(marquee);
        }
        true
    }
    pub(super) fn finish_marquee(
        &mut self,
        state: &EditorState,
        geometry: PointerGeometry,
        position: Point<Pixels>,
    ) -> bool {
        let Some(mut marquee) = self.marquee.take() else {
            return false;
        };
        if !finite(position) || !marquee.current(self, state, geometry) {
            return true;
        }
        marquee.update(position);
        let frames = marquee.frames(state);
        self.select_frames(state, marquee.target.item, frames);
        self.domain_owned = true;
        self.ownership.as_ref().unwrap().set(true);
        true
    }
    pub(super) fn marquee_preview(
        &self,
        item: u64,
        state: &EditorState,
    ) -> Option<BTreeSet<Frame>> {
        self.marquee
            .as_ref()
            .filter(|p| p.target.item == item)
            .map(|p| p.frames(state))
    }
}

impl TimelineColors {
    pub(super) fn marquee_lane(
        &self,
        lane: gpui::Div,
        target: Option<Target>,
        state: &Entity<EditorState>,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        disabled: Option<InputTarget>,
        identity: (u64, u64),
        cx: &mut Context<super::super::Timeline>,
    ) -> gpui::Stateful<gpui::Div> {
        let name = format!("colors-marquee-{}-{}", identity.0, identity.1);
        let lane = lane.id(SharedString::from(name.clone()));
        let Some(target) = target else {
            return crate::color_edit::input_pointer_key_button_guarded(
                lane,
                name,
                disabled,
                |_, _, _| false,
            )
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            });
        };
        let input = self.input.clone();
        let guard_target = target.clone();
        let lane = crate::color_edit::input_pointer_key_button_guarded(
            lane,
            name,
            Some(target.owner.input.clone()),
            move |state, cx, _| {
                input
                    .borrow()
                    .prepare(&guard_target, state, TextField::active_pending_binding(cx))
            },
        );
        let state = state.clone();
        lane.on_mouse_down(gpui::MouseButton::Left, cx.listener(
            move |this, event: &gpui::MouseDownEvent, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                this.colors.cancel_pointer();
                if !window.is_window_active()
                    || event.click_count != 1
                    || !modifiers_allowed(event.modifiers, event.modifiers.shift)
                    || TextField::is_composing(window, cx)
                    || !this.colors.input.borrow().prepare(
                        &target, state.read(cx), TextField::active_pending_binding(cx),
                    )
                {
                    return;
                }
                let Some(bounds) = bounds.get() else { return; };
                let geometry = PointerGeometry {
                    bounds,
                    start: state.read(cx).timeline_start,
                    visible: state.read(cx).visible_frames(),
                };
                window.focus(&this.focus);
                if this.colors.input.borrow_mut().begin_marquee(
                    &target, state.read(cx), event.position, event.modifiers.shift, geometry,
                ) {
                    this.colors.pointer_generation = Some(crate::color_edit::input_pointer_generation(window, cx));
                    this.scrubbing = false;
                    this.drag = None;
                    this.bar_drag = None;
                    this.marquee = None;
                    this.key_menu = None;
                    cx.notify();
                }
            },
        )).tooltip(|_, cx| {
            cx.new(|_| ui::Tip("Drag empty lane to box-select this paint’s key centers; Shift adds. Empty click clears.".into())).into()
        })
    }
    pub(super) fn marquee_overlay(
        &self,
        lane: gpui::Stateful<gpui::Div>,
        item: u64,
    ) -> gpui::Stateful<gpui::Div> {
        let rect = self
            .input
            .borrow()
            .marquee
            .as_ref()
            .filter(|p| p.target.item == item && p.crossed)
            .map(PointerMarquee::rect);
        lane.when_some(rect, |lane, rect| {
            lane.child(
                gpui::canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        window.paint_quad(gpui::fill(rect, gpui::rgba(0x529bdf35)));
                    },
                )
                .absolute()
                .size_full(),
            )
        })
    }
}

#[cfg(test)]
#[path = "compound_colors_marquee_tests.rs"]
mod tests;
