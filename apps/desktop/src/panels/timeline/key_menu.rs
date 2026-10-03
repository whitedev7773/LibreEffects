use super::*;
use gpui::{AnyElement, Point, anchored, deferred};
use libre_effects_core::{Interpolation, Project, TemporalMode};

#[derive(Clone, Copy)]
enum Edit {
    Ease(bool, bool),
    Mode(TemporalMode),
    Segment(Interpolation),
    Graph,
    Delete,
}
const ENTRIES: [(&str, &str, Edit); 10] = [
    ("Easy Ease", "F9", Edit::Ease(true, true)),
    ("Easy Ease In", "Shift+F9", Edit::Ease(true, false)),
    ("Easy Ease Out", "Ctrl+Shift+F9", Edit::Ease(false, true)),
    ("Auto Bezier", "", Edit::Mode(TemporalMode::Auto)),
    (
        "Continuous Bezier",
        "",
        Edit::Mode(TemporalMode::Continuous),
    ),
    (
        "Independent Bezier",
        "",
        Edit::Mode(TemporalMode::Independent),
    ),
    (
        "Linear outgoing segment",
        "",
        Edit::Segment(Interpolation::Linear),
    ),
    (
        "Hold outgoing segment",
        "",
        Edit::Segment(Interpolation::Hold),
    ),
    ("Show in Graph Editor", "", Edit::Graph),
    ("Delete keyframes", "Delete", Edit::Delete),
];
pub(super) struct Menu {
    position: Point<Pixels>,
    keys: Vec<KeyRef>,
    anchor: KeyRef,
    revision: u64,
    current: usize,
}
impl Menu {
    pub fn valid(&self, s: &EditorState) -> bool {
        !s.graph_open
            && s.document_revision == self.revision
            && s.selected_keys
                .iter()
                .copied()
                .eq(self.keys.iter().copied())
    }
}
fn target_selection(selected: &BTreeSet<KeyRef>, clicked: &[KeyRef]) -> BTreeSet<KeyRef> {
    if clicked.iter().any(|k| selected.contains(k)) {
        selected.clone()
    } else {
        clicked.iter().copied().collect()
    }
}
fn command(project: &Project, keys: &[KeyRef], edit: Edit) -> Result<Option<Command>, String> {
    if keys.is_empty() {
        return Ok(None);
    }
    if let Edit::Ease(incoming, outgoing) = edit {
        return super::super::key_easing::selected(project, keys, incoming, outgoing);
    }
    let mut commands = Vec::new();
    for key in keys {
        let layer = project
            .composition()
            .layer(key.id)
            .ok_or("Selected layer no longer exists")?;
        let track = layer
            .track(key.property)
            .ok_or("Selected property no longer exists")?;
        if !track.keys().contains_key(&key.frame) {
            return Err("Selected keyframe no longer exists".into());
        }
        if !matches!(edit, Edit::Graph) && layer.locked() {
            return Err("Unlock the selected layers before editing keyframes".into());
        }
        if matches!(edit, Edit::Mode(_) | Edit::Graph)
            && matches!(key.property, PropertyPath::Path(_))
        {
            return Err("This command requires scalar keys".into());
        }
        match edit {
            Edit::Mode(mode) => commands.push(Command::SetTemporalMode {
                id: key.id,
                property: key.property,
                frame: key.frame,
                mode,
            }),
            Edit::Segment(interpolation) => commands.push(Command::EditTrack {
                id: key.id,
                property: key.property,
                edit: TrackEdit::Interpolate {
                    frame: key.frame,
                    interpolation,
                },
            }),
            _ => {}
        }
    }
    Ok(Some(match edit {
        Edit::Delete => Command::DeleteKeys(keys.to_vec()),
        _ => Command::Batch(commands),
    }))
}
fn navigate(current: usize, direction: i32, enabled: &[bool]) -> usize {
    (1..=enabled.len())
        .map(|step| {
            (current as i32 + direction * step as i32).rem_euclid(enabled.len() as i32) as usize
        })
        .find(|&index| enabled[index])
        .unwrap_or(current)
}
impl Timeline {
    pub(super) fn open_key_menu(
        &mut self,
        clicked: Option<&[KeyRef]>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag = None;
        self.bar_drag = None;
        self.marquee = None;
        self.parent_open = None;
        window.focus(&self.focus);
        if let Some(clicked) = clicked {
            self.state.update(cx, |s, cx| {
                s.selected_keys = target_selection(&s.selected_keys, clicked);
                cx.notify();
            });
        }
        let s = self.state.read(cx);
        if s.selected_keys.is_empty() || s.graph_open {
            return;
        }
        let keys: Vec<_> = s.selected_keys.iter().copied().collect();
        let enabled = ENTRIES.map(|(_, _, edit)| {
            command(s.editor.project(), &keys, edit).is_ok_and(|c| c.is_some())
        });
        self.key_menu = Some(Menu {
            position,
            anchor: clicked
                .and_then(|clicked| clicked.iter().find(|k| keys.contains(k)))
                .copied()
                .unwrap_or(keys[0]),
            keys,
            revision: s.document_revision,
            current: enabled.iter().position(|e| *e).unwrap_or(0),
        });
        cx.notify();
    }
    fn activate_key_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.key_menu.as_ref() else {
            return;
        };
        let s = self.state.read(cx);
        if !menu.valid(s) {
            self.key_menu = None;
            cx.notify();
            return;
        }
        let edit = ENTRIES[index].2;
        let Ok(Some(command)) = command(s.editor.project(), &menu.keys, edit) else {
            return;
        };
        let keys = menu.keys.clone();
        let anchor = menu.anchor;
        self.key_menu = None;
        self.state.update(cx, |s, cx| {
            if matches!(edit, Edit::Graph) {
                let first = anchor;
                s.dispatch(&Action::GraphProperty(first.id, first.property), window, cx);
                s.selected_keys = keys.into_iter().collect();
                s.graph_key = Some(first);
                let included = s.graph_included_channels();
                s.selected_keys
                    .retain(|key| included.contains(&crate::view_state::GraphChannel::from(*key)));
                cx.notify();
            } else {
                s.dispatch(&Action::Edit(command), window, cx);
            }
        });
        if matches!(edit, Edit::Graph) {
            self.graph.read(cx).focus(window);
        } else {
            window.focus(&self.focus);
        }
        cx.notify();
    }
    pub(super) fn key_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if self.key_menu.is_none() {
            if key == "f10" && event.keystroke.modifiers.shift && self.focus.is_focused(window) {
                let selected = &self.state.read(cx).selected_keys;
                let position = self
                    .hit_keys
                    .borrow()
                    .iter()
                    .find(|(k, _)| selected.contains(k))
                    .map(|(_, b)| b.bottom_right());
                if let Some(position) = position {
                    self.open_key_menu(None, position, window, cx);
                }
                return true;
            }
            return false;
        }
        let menu = self.key_menu.as_mut().unwrap();
        let enabled = ENTRIES.map(|(_, _, edit)| {
            command(self.state.read(cx).editor.project(), &menu.keys, edit)
                .is_ok_and(|c| c.is_some())
        });
        match key {
            "escape" => self.key_menu = None,
            "up" => menu.current = navigate(menu.current, -1, &enabled),
            "down" => menu.current = navigate(menu.current, 1, &enabled),
            "home" => menu.current = enabled.iter().position(|e| *e).unwrap_or(menu.current),
            "end" => menu.current = enabled.iter().rposition(|e| *e).unwrap_or(menu.current),
            "enter" if !event.is_held => {
                let current = menu.current;
                self.activate_key_menu(current, window, cx);
            }
            _ => {}
        }
        cx.notify();
        true
    }
    pub(super) fn render_key_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.key_menu.as_ref()?;
        let project = self.state.read(cx).editor.project();
        let mut content = div()
            .id("timeline-key-menu")
            .occlude()
            .w(px(282.0))
            .py_1()
            .bg(rgb(0x292929))
            .border_1()
            .border_color(rgb(0x555555))
            .shadow_md()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.key_menu = None;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(22.0))
                    .px_2()
                    .text_color(rgb(ui::MUTED))
                    .child(format!("{} selected keyframes", menu.keys.len())),
            );
        for (index, (label, shortcut, edit)) in ENTRIES.into_iter().enumerate() {
            if [3, 6, 8, 9].contains(&index) {
                content = content.child(div().h(px(1.0)).my_1().bg(rgb(0x414141)));
            }
            let result = command(project, &menu.keys, edit);
            let enabled = result.as_ref().is_ok_and(|c| c.is_some());
            let tip = result.err().unwrap_or_else(|| label.into());
            content = content.child(
                div()
                    .id(("timeline-key-menu-row", index))
                    .h(px(25.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when(!enabled, |d| d.opacity(0.4))
                    .when(enabled && index == menu.current, |d| d.bg(rgb(0x3b5068)))
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if enabled
                            && let Some(menu) = &mut this.key_menu
                            && menu.current != index
                        {
                            menu.current = index;
                            cx.notify();
                        }
                    }))
                    .tooltip(move |_, cx| cx.new(|_| ui::Tip(tip.clone().into())).into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_key_menu(index, window, cx);
                        cx.stop_propagation();
                    }))
                    .child(label)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(shortcut),
                    ),
            );
        }
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(content),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> (EditorState, Vec<KeyRef>) {
        let mut s = EditorState::default();
        s.editor.execute(Command::AddRectangle).unwrap();
        let mut keys = Vec::new();
        for p in [Property::PositionX, Property::PositionY] {
            for (frame, value) in [(10, 600.0), (30, 900.0), (60, 720.0)] {
                for edit in [
                    TrackEdit::ToggleKey { frame },
                    TrackEdit::Value { frame, value },
                ] {
                    s.editor
                        .execute(Command::EditTrack {
                            id: 1,
                            property: p.into(),
                            edit,
                        })
                        .unwrap();
                }
            }
            keys.push(KeyRef {
                id: 1,
                property: p.into(),
                frame: 30,
            });
        }
        s.selected_keys = keys.iter().copied().collect();
        (s, keys)
    }
    #[test]
    fn context_selection_preserves_existing_group_or_selects_clicked_combined_row() {
        let (_, keys) = scene();
        let selected: BTreeSet<_> = [keys[0]].into();
        assert_eq!(target_selection(&selected, &keys), selected);
        let other = KeyRef {
            frame: 60,
            ..keys[0]
        };
        assert_eq!(target_selection(&selected, &[other]), [other].into());
        assert_eq!(
            target_selection(&BTreeSet::new(), &keys),
            keys.into_iter().collect()
        );
    }
    #[test]
    fn menu_rejects_stale_state_and_navigation_skips_disabled_rows() {
        let (mut s, keys) = scene();
        let menu = Menu {
            position: point(px(0.0), px(0.0)),
            anchor: keys[0],
            keys,
            revision: s.document_revision,
            current: 0,
        };
        assert!(menu.valid(&s));
        s.frame = 42;
        assert!(menu.valid(&s));
        s.document_revision += 1;
        assert!(!menu.valid(&s));
        s.document_revision -= 1;
        s.graph_open = true;
        assert!(!menu.valid(&s));
        s.graph_open = false;
        s.selected_keys.clear();
        assert!(!menu.valid(&s));
        assert_eq!(navigate(0, 1, &[true, false, true]), 2);
        assert_eq!(navigate(0, -1, &[true, false, true]), 2);
        assert_eq!(navigate(2, 1, &[true, false, true]), 0);
        assert_eq!(navigate(1, 1, &[false, false, false]), 1);
    }
    #[test]
    fn menu_edits_are_atomic_and_saved_curves_match_preview_and_output() {
        for edit in [
            Edit::Ease(true, false),
            Edit::Mode(TemporalMode::Auto),
            Edit::Mode(TemporalMode::Continuous),
            Edit::Mode(TemporalMode::Independent),
            Edit::Segment(Interpolation::Hold),
            Edit::Segment(Interpolation::Linear),
            Edit::Delete,
        ] {
            let (mut s, keys) = scene();
            let before = s.editor.project().clone();
            s.editor
                .execute(command(&before, &keys, edit).unwrap().unwrap())
                .unwrap();
            let after = Project::from_json(&s.editor.project().to_json().unwrap()).unwrap();
            if !matches!(
                edit,
                Edit::Mode(TemporalMode::Independent) | Edit::Segment(Interpolation::Linear)
            ) {
                assert_ne!(before, after);
                s.editor.undo();
                assert_eq!(s.editor.project(), &before);
                s.editor.redo();
                assert_eq!(s.editor.project(), &after);
            }
            let r = crate::rendering::Renderer::new();
            for frame in [10, 20, 30, 45, 60] {
                assert_eq!(
                    r.render(&after, frame, 384).unwrap(),
                    r.render_output(&after, frame, 384, 216).unwrap()
                );
            }
        }
    }
    #[test]
    fn invalid_or_locked_selection_cannot_partially_apply_but_graph_remains_available() {
        let (mut s, keys) = scene();
        let missing = KeyRef {
            frame: 31,
            ..keys[0]
        };
        for (_, _, edit) in ENTRIES {
            assert!(command(s.editor.project(), &[keys[0], missing], edit).is_err());
            assert!(command(s.editor.project(), &[], edit).unwrap().is_none());
        }
        s.editor.execute(Command::ToggleLocked(1)).unwrap();
        let before = s.editor.project().clone();
        for (_, _, edit) in ENTRIES {
            assert_eq!(
                command(&before, &keys, edit).is_ok(),
                matches!(edit, Edit::Graph)
            );
        }
        assert_eq!(s.editor.project(), &before);
    }
}
