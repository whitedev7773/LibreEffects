//! Local-only compound key press/preview/release state. No project/VIEW writes
//! occur until release, and a source/domain receipt is never rebased mid-drag.
use super::*;
use gpui::{Bounds, Pixels};

const DRAG_THRESHOLD: f32 = 4.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PointerGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) start: Frame,
    pub(crate) visible: Frame,
}
impl PointerGeometry {
    pub(super) fn valid(self, state: &EditorState) -> bool {
        self.start == state.timeline_start
            && self.visible == state.visible_frames()
            && self.visible > 0
            && f32::from(self.bounds.left()).is_finite()
            && f32::from(self.bounds.size.width).is_finite()
            && f32::from(self.bounds.size.width) > 0.
            && f32::from(self.bounds.top()).is_finite()
            && f32::from(self.bounds.size.height).is_finite()
            && f32::from(self.bounds.size.height) > 0.
            && f32::from(self.bounds.right()).is_finite()
            && f32::from(self.bounds.bottom()).is_finite()
    }
}

pub(super) struct PointerDrag {
    pub(super) target: Target,
    // Keep detail rows stable during a press, including first selection of a
    // different paint. Only glyph selection/ghosts change before release.
    pub(super) previous_selection: Option<Selection>,
    pub(super) key: Frame,
    pub(super) origin_x: f32,
    pub(super) geometry: PointerGeometry,
    pub(super) shift: bool,
    pub(super) crossed: bool,
    pub(super) delta: i64,
    pub(super) snapped: Option<Frame>,
    pub(super) invalid: bool,
    snapping: bool,
    targets: Vec<i64>,
}
impl PointerDrag {
    pub(crate) fn current(
        &self,
        input: &Input,
        state: &EditorState,
        geometry: PointerGeometry,
    ) -> bool {
        self.geometry == geometry
            && geometry.valid(state)
            && self.snapping == state.snapping
            && input.current(&self.target, state)
    }
    fn update(&mut self, state: &EditorState, x: f32, alt: bool) {
        let pixels = x - self.origin_x;
        self.crossed |= pixels.abs() >= DRAG_THRESHOLD;
        self.delta = 0;
        self.snapped = None;
        self.invalid = false;
        if self.shift || !self.crossed {
            return;
        }
        let selected = self.target.selection.as_ref().unwrap();
        let anchors: Vec<_> = selected.frames.iter().map(|f| i64::from(*f)).collect();
        let limits = (
            -anchors[0],
            i64::from(state.editor.project().composition().duration() - 1)
                - anchors.last().unwrap(),
        );
        let raw = f64::from(pixels) * f64::from(self.geometry.visible)
            / f64::from(f32::from(self.geometry.bounds.size.width));
        // A return to the pressed frame is an exact no-op, even if another key
        // or the playhead is within snap tolerance of the original selection.
        let rounded = (raw.round() as i64).clamp(limits.0, limits.1);
        let (delta, snapped) = if rounded != 0 && self.snapping && !alt {
            let tolerance = 8. * f64::from(self.geometry.visible)
                / f64::from(f32::from(self.geometry.bounds.size.width));
            let mut best: Option<(f64, i64, i64, i64)> = None;
            for &anchor in &anchors {
                let wanted = anchor as f64 + raw;
                let at = self.targets.partition_point(|t| (*t as f64) < wanted);
                for &target in self
                    .targets
                    .get(at)
                    .into_iter()
                    .chain(at.checked_sub(1).and_then(|i| self.targets.get(i)))
                {
                    let delta = target - anchor;
                    let candidate = ((delta as f64 - raw).abs(), anchor, target, delta);
                    if candidate.0 <= tolerance
                        && (limits.0..=limits.1).contains(&delta)
                        && best.is_none_or(|old| candidate < old)
                    {
                        best = Some(candidate);
                    }
                }
            }
            best.map_or((rounded, None), |(_, _, target, delta)| {
                (delta, Some(target as Frame))
            })
        } else {
            (rounded, None)
        };
        self.delta = delta.clamp(limits.0, limits.1);
        self.snapped = snapped;
        let to = (i64::from(selected.anchor()) + self.delta) as Frame;
        self.invalid = move_command(&self.target, state, &to.to_string()).is_err();
    }
}

impl Input {
    /// A press on a selected key preserves the group until a click-only release.
    /// Shift is deliberately selection-only: its movement never retimes keys.
    pub(crate) fn begin_pointer(
        &mut self,
        target: &Target,
        state: &EditorState,
        key: Frame,
        x: f32,
        shift: bool,
        geometry: PointerGeometry,
    ) -> bool {
        self.cancel_pointer();
        if !self.current(target, state)
            || !geometry.valid(state)
            || !x.is_finite()
            || target.owner.graph_open
            || !target.owner.expanded
            || target.command(state, Control::Select(key)).is_none()
        {
            return false;
        }
        let previous_selection = self.selected.clone();
        if !shift
            && !self
                .selected
                .as_ref()
                .is_some_and(|s| s.owns(target) && s.frames.contains(&key))
        {
            self.select_frames(state, target.item, [key].into());
        }
        // Shift's pending selection-only press also owns the compound domain.
        // Generic Delete/Cut/Duplicate must never mistake it for layer selection.
        if !self.domain_owned {
            self.domain_owned = true;
            if let Some(ownership) = &self.ownership {
                ownership.set(true);
            }
            self.serial = self.serial.wrapping_add(1);
        }
        let target = self.target(target.item).unwrap();
        let animation = target
            .node(state)
            .unwrap()
            .kind
            .gradient()
            .unwrap()
            .colors_animation()
            .unwrap();
        let mut targets: BTreeSet<_> = animation
            .keys()
            .keys()
            .filter(|f| {
                !target
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.owns(&target) && s.frames.contains(f))
            })
            .map(|f| i64::from(*f))
            .collect();
        targets.insert(i64::from(state.frame));
        self.pointer = Some(PointerDrag {
            target,
            previous_selection,
            key,
            origin_x: x,
            geometry,
            shift,
            crossed: false,
            delta: 0,
            snapped: None,
            invalid: false,
            snapping: state.snapping,
            targets: targets.into_iter().collect(),
        });
        true
    }
    pub(crate) fn update_pointer(
        &mut self,
        state: &EditorState,
        geometry: PointerGeometry,
        x: f32,
        alt: bool,
    ) -> bool {
        let Some(mut drag) = self.pointer.take() else {
            return false;
        };
        if x.is_finite() && drag.current(self, state, geometry) {
            drag.update(state, x, alt);
            self.pointer = Some(drag);
        }
        true
    }
    /// Consumes the receipt before invoking application code, so duplicate or
    /// stale releases cannot replay the commit or restore retired selection.
    pub(crate) fn finish_pointer(
        &mut self,
        state: &mut EditorState,
        geometry: PointerGeometry,
        x: f32,
        alt: bool,
        inside_pressed_key: bool,
        mut apply: impl FnMut(&mut EditorState, Action) -> bool,
    ) -> bool {
        let Some(mut drag) = self.pointer.take() else {
            return false;
        };
        if !x.is_finite() || !drag.current(self, state, geometry) {
            return true;
        }
        drag.update(state, x, alt);
        if !drag.crossed {
            if inside_pressed_key {
                // Preserve the established key click/Shift-click seek behavior,
                // but defer every VIEW mutation until the click is established.
                state.selected_keys.clear();
                state.graph_key = None;
                if apply(state, Action::Seek(drag.key)) {
                    self.select_key(state, drag.target.item, drag.key, drag.shift);
                }
            }
            return true;
        }
        if drag.shift || drag.delta == 0 {
            return true;
        }
        let selection = drag.target.selection.as_ref().unwrap();
        let to = (i64::from(selection.anchor()) + drag.delta) as Frame;
        match move_command(&drag.target, state, &to.to_string()) {
            Ok(Some((command, _))) => {
                if apply(state, Action::Edit(command)) {
                    let frames = selection
                        .frames
                        .iter()
                        .map(|f| (i64::from(*f) + drag.delta) as Frame)
                        .collect();
                    state.selected_keys.clear();
                    state.graph_key = None;
                    self.observe(state);
                    self.select_frames(state, selection.item, frames);
                }
            }
            Err(error) => state.status = error,
            Ok(None) => {}
        }
        true
    }
    pub(crate) fn cancel_pointer(&mut self) -> bool {
        let pointer = self.pointer.take().is_some();
        self.marquee.take().is_some() || pointer
    }
    #[cfg(test)]
    pub(crate) fn selected_frames(&self) -> BTreeSet<Frame> {
        self.selected
            .as_ref()
            .map(|s| s.frames.clone())
            .unwrap_or_default()
    }
}

fn pointer_modifiers(modifiers: gpui::Modifiers, shift: bool) -> bool {
    !modifiers.control && !modifiers.platform && !modifiers.function && modifiers.shift == shift
}

impl TimelineColors {
    pub(crate) fn observe_pointer_ui(
        &mut self,
        focus: &gpui::FocusHandle,
        window: &Window,
        cx: &gpui::App,
    ) {
        if self.input.borrow().pointer_active()
            && !self.pointer_valid(window.modifiers(), focus, window, cx)
        {
            self.cancel_pointer();
        }
    }
    pub(crate) fn pointer_modifiers_changed(&mut self, modifiers: gpui::Modifiers) -> bool {
        if self.input.borrow().pointer_active() && !self.active_modifiers_allowed(modifiers) {
            return self.cancel_pointer();
        }
        false
    }
    pub(crate) fn cancel_pointer(&mut self) -> bool {
        self.pointer_generation = None;
        self.pressed_bounds = None;
        self.input.borrow_mut().cancel_pointer()
    }
    fn pointer_geometry(&self, state: &EditorState) -> Option<PointerGeometry> {
        let input = self.input.borrow();
        let target = input
            .pointer
            .as_ref()
            .map(|p| &p.target)
            .or_else(|| input.marquee.as_ref().map(|p| &p.target))?;
        Some(PointerGeometry {
            bounds: self
                .lane_bounds
                .get(&(target.owner.layer, target.item))?
                .get()?,
            start: state.timeline_start,
            visible: state.visible_frames(),
        })
    }
    fn active_modifiers_allowed(&self, modifiers: gpui::Modifiers) -> bool {
        let input = self.input.borrow();
        input
            .pointer
            .as_ref()
            .is_some_and(|p| pointer_modifiers(modifiers, p.shift))
            || input
                .marquee
                .as_ref()
                .is_some_and(|p| pointer_marquee::modifiers_allowed(modifiers, p.shift))
    }

    fn pointer_valid(
        &self,
        modifiers: gpui::Modifiers,
        focus: &gpui::FocusHandle,
        window: &Window,
        cx: &gpui::App,
    ) -> bool {
        window.is_window_active()
            && focus.is_focused(window)
            && !TextField::is_composing(window, cx)
            && !TextField::active_has_focus(window, cx)
            && TextField::active_pending_binding(cx).is_none()
            && self.pointer_generation
                == Some(crate::color_edit::input_pointer_generation(window, cx))
            && self.active_modifiers_allowed(modifiers)
    }
    pub(crate) fn pointer_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        state: &Entity<EditorState>,
        focus: &gpui::FocusHandle,
        window: &Window,
        cx: &mut Context<super::super::Timeline>,
    ) {
        if !self.input.borrow().pointer_active() {
            return;
        }
        if event.pressed_button != Some(gpui::MouseButton::Left)
            || !self.pointer_valid(event.modifiers, focus, window, cx)
        {
            self.cancel_pointer();
        } else if let Some(geometry) = self.pointer_geometry(state.read(cx)) {
            let mut input = self.input.borrow_mut();
            if input.marquee.is_some() {
                input.update_marquee(state.read(cx), geometry, event.position);
            } else {
                input.update_pointer(
                    state.read(cx),
                    geometry,
                    f32::from(event.position.x),
                    event.modifiers.alt,
                );
            }
        } else {
            self.cancel_pointer();
        }
        cx.notify();
    }
    pub(crate) fn pointer_up(
        &mut self,
        event: &gpui::MouseUpEvent,
        state: &Entity<EditorState>,
        focus: &gpui::FocusHandle,
        window: &mut Window,
        cx: &mut Context<super::super::Timeline>,
    ) {
        if !self.input.borrow().pointer_active() {
            return;
        }
        let geometry = self.pointer_geometry(state.read(cx));
        if event.button != gpui::MouseButton::Left
            || event.click_count != 1
            || !self.pointer_valid(event.modifiers, focus, window, cx)
            || geometry.is_none()
        {
            self.cancel_pointer();
        } else {
            let inside = self
                .pressed_bounds
                .is_some_and(|b| b.contains(&event.position));
            state.update(cx, |state, cx| {
                let mut input = self.input.borrow_mut();
                if input.marquee.is_some() {
                    input.finish_marquee(state, geometry.unwrap(), event.position);
                } else {
                    input.finish_pointer(
                        state,
                        geometry.unwrap(),
                        f32::from(event.position.x),
                        event.modifiers.alt,
                        inside,
                        |state, action| {
                            let edit = matches!(action, Action::Edit(_));
                            state.dispatch(&action, window, cx);
                            !edit || state.status == "Edited"
                        },
                    );
                }
                cx.notify();
            });
            self.pointer_generation = None;
            self.pressed_bounds = None;
        }
        // This global capture owns release even outside Timeline. A second
        // delivery through its normal bubble handlers cannot replay anything.
        cx.stop_propagation();
        cx.notify();
    }
    pub(crate) fn key_button(
        &self,
        target: Option<Target>,
        key: Frame,
        label: &'static str,
        active: bool,
        state: &Entity<EditorState>,
        lane: Rc<Cell<Option<Bounds<Pixels>>>>,
        disabled: Option<InputTarget>,
        identity: (u64, u64),
        cx: &mut Context<super::super::Timeline>,
    ) -> gpui::Stateful<gpui::Div> {
        let name = format!("colors-pointer-{}-{}-{key}", identity.0, identity.1);
        let bounds = Rc::new(Cell::new(None));
        let measure = bounds.clone();
        let widget = ui::text_button(SharedString::from(name.clone()), label)
            .text_size(px(10.))
            .when(active, |d| d.text_color(rgb(ui::BLUE)))
            .child(
                gpui::canvas(move |b, _, _| measure.set(Some(b)), |_, _, _, _| ())
                    .absolute()
                    .size_full(),
            );
        let Some(target) = target else {
            return crate::color_edit::input_pointer_button_guarded(
                widget.opacity(0.4),
                name,
                disabled,
                |_, _, _| false,
            )
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation());
        };
        let input = self.input.clone();
        let guard_target = target.clone();
        let widget = crate::color_edit::input_pointer_key_drag_guarded(
            widget,
            name,
            Some(target.owner.input.clone()),
            move |state, cx, _| {
                input
                    .borrow()
                    .prepare(&guard_target, state, TextField::active_pending_binding(cx))
            },
        );
        let keyboard_target = target.clone();
        let state = state.clone();
        widget
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    this.colors.cancel_pointer();
                    if !window.is_window_active()
                        || event.click_count != 1
                        || !pointer_modifiers(event.modifiers, event.modifiers.shift)
                        || TextField::is_composing(window, cx)
                        || !this.colors.input.borrow().prepare(
                            &target,
                            state.read(cx),
                            TextField::active_pending_binding(cx),
                        )
                    {
                        return;
                    }
                    let (Some(lane), Some(pressed)) = (lane.get(), bounds.get()) else {
                        return;
                    };
                    let geometry = PointerGeometry {
                        bounds: lane,
                        start: state.read(cx).timeline_start,
                        visible: state.read(cx).visible_frames(),
                    };
                    window.focus(&this.focus);
                    if this.colors.input.borrow_mut().begin_pointer(
                        &target,
                        state.read(cx),
                        key,
                        f32::from(event.position.x),
                        event.modifiers.shift,
                        geometry,
                    ) {
                        this.colors.pointer_generation =
                            Some(crate::color_edit::input_pointer_generation(window, cx));
                        this.colors.pressed_bounds = Some(pressed);
                        this.scrubbing = false;
                        this.drag = None;
                        this.bar_drag = None;
                        this.marquee = None;
                        this.key_menu = None;
                        cx.notify();
                    }
                }),
            )
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    // Mouse activation is exclusively owned by the captured release.
                    // Retain Enter/Space accessibility without a click-after-drag path.
                    if !matches!(event, gpui::ClickEvent::Keyboard(_)) {
                        return;
                    }
                    cx.stop_propagation();
                    if !window.is_window_active()
                        || !modifiers_allowed(event.modifiers(), Control::Select(key))
                        || TextField::is_composing(window, cx)
                        || TextField::active_has_focus(window, cx)
                        || !this.colors.input.borrow().prepare(
                            &keyboard_target,
                            this.state.read(cx),
                            TextField::active_pending_binding(cx),
                        )
                    {
                        return;
                    }
                    let input = this.colors.input.clone();
                    this.state.update(cx, |state, cx| {
                        state.selected_keys.clear();
                        state.graph_key = None;
                        state.dispatch(&Action::Seek(key), window, cx);
                        input.borrow_mut().select_key(
                            state,
                            keyboard_target.item,
                            key,
                            event.modifiers().shift,
                        );
                        cx.notify();
                    });
                    window.focus(&this.focus);
                    cx.notify();
                }),
            )
    }
}
