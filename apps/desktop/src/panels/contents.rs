use crate::{
    components::TextField,
    editor::{Action, EditorState},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, Pixels, Window, anchored, canvas, deferred,
    div, point, prelude::*, px, rgb,
};
use libre_effects_core::{
    Command, Content, ContentsEdit, ContentsKind, ContentsParam, GradientParam, PaintBlend,
    PaintComposite, PathTarget, PropertyPath, ShapeGradient, ShapeKind, ShapeStroke, StrokeCap,
    StrokeJoin, TrackEdit,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub(super) mod gradient_ramp;
mod tree;
mod tree_selection;
use gradient_ramp::RampDrag;
use tree_selection::Selection;

const MOVE_HELP: &str = "Keeps local values; placement and paint scope may change.";

const TRIM_HELP: &str = "Trims each source contour above this item. Start and End are percentages; Offset moves the interval in degrees. Source paths remain editable.";

#[derive(Clone, Copy)]
struct ContentsFieldTarget {
    composition: libre_effects_core::CompositionId,
    layer: u64,
    item: u64,
    revision: u64,
}
impl ContentsFieldTarget {
    fn current(self, state: &EditorState) -> bool {
        state.editor.project().active_composition_id() == self.composition
            && state.editor.selected() == Some(self.layer)
            && state.document_revision == self.revision
            && state.contents_selection == Some((self.composition, self.layer, self.item))
    }
    fn value_command(
        self,
        state: &EditorState,
        parameter: ContentsParam,
        value: f64,
    ) -> Option<Command> {
        self.current(state).then_some(Command::Contents {
            id: self.layer,
            edit: ContentsEdit::Track {
                item: self.item,
                parameter,
                edit: TrackEdit::Value {
                    frame: state.frame,
                    value,
                },
            },
        })
    }
}

struct PaintMenu {
    layer: u64,
    item: u64,
    revision: u64,
    picker: usize,
    cursor: usize,
}
fn paint_options(picker: usize) -> Vec<&'static str> {
    if picker == 0 {
        PaintBlend::ALL.into_iter().map(PaintBlend::label).collect()
    } else {
        vec![
            "Below Previous in Same Group",
            "Above Previous in Same Group",
        ]
    }
}
pub(crate) struct ContentsControls {
    state: Entity<EditorState>,
    owner: Option<(libre_effects_core::CompositionId, u64)>,
    selected: Option<u64>,
    selection: Selection,
    owner_revision: u64,
    move_context: Option<tree::MoveContext>,
    move_input: Option<crate::color_edit::InputTarget>,
    move_serial: u64,
    tree_drag: Option<tree::Drag>,
    tree_focus: FocusHandle,
    tree_rows: Rc<RefCell<std::collections::BTreeMap<u64, tree::RowBounds>>>,
    fields: Vec<(ContentsParam, Entity<TextField>)>,
    name: Option<Entity<TextField>>,
    add_open: bool,
    collapsed: std::collections::BTreeSet<u64>,
    gradient_stop: Option<u64>,
    ramp_drag: Option<RampDrag>,
    ramp_selected: Option<GradientParam>,
    ramp_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    ramp_focus: FocusHandle,
    paint_menu: Option<PaintMenu>,
    paint_focus: [FocusHandle; 2],
    paint_bounds: [Rc<Cell<Option<Bounds<Pixels>>>>; 2],
    paint_watches: Option<Vec<gpui::Subscription>>,
}
impl ContentsControls {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            let s = this.state.read(cx);
            let cancel_ramp = this
                .ramp_drag
                .as_ref()
                .is_some_and(|d| !d.current(s, this.selected));
            let cancel_tree = this
                .tree_drag
                .as_ref()
                .is_some_and(|d| !d.current(s, &this.selection, &this.collapsed));
            let cancel_menu = this.paint_menu.as_ref().is_some_and(|m| {
                m.revision != s.document_revision || s.editor.selected() != Some(m.layer)
            });
            if cancel_tree {
                this.tree_drag = None;
            }
            this.reconcile_tree(cx);
            if cancel_ramp {
                this.cancel_ramp(cx);
            }
            if cancel_menu {
                this.paint_menu = None;
            }
            cx.notify();
        })
        .detach();
        Self {
            state,
            owner: None,
            selected: None,
            selection: Selection::default(),
            owner_revision: 0,
            move_context: None,
            move_input: None,
            move_serial: 0,
            tree_drag: None,
            tree_focus: cx.focus_handle(),
            tree_rows: Default::default(),
            fields: vec![],
            name: None,
            add_open: false,
            collapsed: Default::default(),
            gradient_stop: None,
            ramp_drag: None,
            ramp_selected: None,
            ramp_bounds: Default::default(),
            ramp_focus: cx.focus_handle(),
            paint_menu: None,
            paint_focus: [cx.focus_handle(), cx.focus_handle()],
            paint_bounds: Default::default(),
            paint_watches: None,
        }
    }
    fn select(&mut self, layer: u64, item: u64, cx: &mut Context<Self>) {
        self.paint_menu = None;
        self.cancel_ramp(cx);
        self.ramp_selected = None;
        let composition = self.state.read(cx).editor.project().active_composition_id();
        let revision = self.state.read(cx).document_revision;
        let target = ContentsFieldTarget {
            composition,
            layer,
            item,
            revision,
        };
        if self.owner != Some((composition, layer)) || self.selected != Some(item) {
            self.gradient_stop = None;
        }
        self.owner = Some((composition, layer));
        self.selected = Some(item);
        self.state.update(cx, |s, cx| {
            let identity = Some((composition, layer, item));
            if s.contents_selection != identity
                && matches!(
                    s.gradient_controls,
                    Some(crate::color_edit::GradientTarget::Contents(..))
                )
            {
                s.gradient_controls = None;
            }
            s.contents_selection = identity;
            cx.notify();
        });
        self.add_open = false;
        let params = self
            .state
            .read(cx)
            .editor
            .selected_layer()
            .and_then(|l| match l.content() {
                Content::ShapeContents(c) => c.node(item).map(|n| n.parameter_order()),
                _ => None,
            })
            .unwrap_or_default();
        self.fields = params
            .into_iter()
            .map(|p| {
                let state = self.state.clone();
                (
                    p,
                    cx.new(|cx| {
                        TextField::new(cx, move |text, w, cx| {
                            state.update(cx, |s, cx| {
                                if !target.current(s) {
                                    return;
                                }
                                match text.trim().parse::<f64>() {
                                    Ok(value) => {
                                        if let Some(command) = target.value_command(s, p, value) {
                                            s.dispatch(&Action::Edit(command), w, cx);
                                        }
                                    }
                                    Err(_) => {
                                        s.status = "Enter a finite Contents value".into();
                                        cx.notify();
                                    }
                                }
                            })
                        })
                    }),
                )
            })
            .collect();
        let state = self.state.clone();
        self.name = Some(cx.new(|cx| {
            TextField::new(cx, move |text, w, cx| {
                state.update(cx, |s, cx| {
                    if target.current(s) {
                        s.dispatch(
                            &Action::Edit(Command::Contents {
                                id: layer,
                                edit: ContentsEdit::Rename {
                                    item,
                                    name: text.into(),
                                },
                            }),
                            w,
                            cx,
                        );
                    }
                })
            })
        }));
        cx.notify();
    }
}
impl ContentsControls {
    fn choose_paint(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.paint_menu.take() else {
            return;
        };
        let s = self.state.read(cx);
        if s.document_revision == m.revision
            && s.editor.selected() == Some(m.layer)
            && self.selected == Some(m.item)
        {
            let edit = if m.picker == 0 {
                PaintBlend::ALL
                    .get(index)
                    .map(|&mode| ContentsEdit::Blend { item: m.item, mode })
            } else {
                [PaintComposite::BelowPrevious, PaintComposite::AbovePrevious]
                    .get(index)
                    .map(|&mode| ContentsEdit::Composite { item: m.item, mode })
            };
            if let Some(edit) = edit {
                self.state.update(cx, |s, cx| {
                    s.dispatch(
                        &Action::Edit(Command::Contents { id: m.layer, edit }),
                        w,
                        cx,
                    )
                });
            }
        }
        cx.notify();
    }
}
impl Render for ContentsControls {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.paint_watches.is_none() {
            let mut watches = self
                .paint_focus
                .iter()
                .map(|f| {
                    cx.on_blur(f, w, |this, _, cx| {
                        this.paint_menu = None;
                        cx.notify();
                    })
                })
                .collect::<Vec<_>>();
            watches.push(cx.observe_window_activation(w, |this, w, cx| {
                if !w.is_window_active() {
                    this.paint_menu = None;
                    this.tree_drag = None;
                    this.cancel_ramp(cx);
                    cx.notify();
                }
            }));
            watches.push(cx.on_blur(&self.ramp_focus, w, |this, _, cx| {
                this.cancel_ramp(cx);
                cx.notify();
            }));
            watches.push(cx.on_blur(&self.tree_focus, w, |this, _, cx| {
                this.tree_drag = None;
                cx.notify();
            }));
            // Repaint retained selection when keyboard focus enters/leaves any
            // part of the tree. The singleton Pen target survives these changes.
            watches.push(cx.on_focus_in(&self.tree_focus, w, |_, _, cx| cx.notify()));
            watches.push(cx.on_focus_out(&self.tree_focus, w, |_, _, _, cx| cx.notify()));
            self.paint_watches = Some(watches);
        }
        self.reconcile_tree(cx);
        let mut root = div().flex().flex_col().gap_1();
        let Some(layer) = self.state.read(cx).editor.selected_layer().cloned() else {
            return root;
        };
        let id = layer.id();
        let locked = layer.locked();
        let frame = self.state.read(cx).frame;
        if matches!(layer.content(), Content::Shape(_)) {
            let state = self.state.clone();
            return root.child(ui::text_button("organize-contents","Create Contents Group")
                .tooltip(|_,cx|cx.new(|_|ui::Tip("Organize this shape into separate Path, Stroke and Fill items. Existing animation is retained. Undo restores the original.".into())).into())
                .when(!locked,|b|b.on_click(move|_,w,cx|state.update(cx,|s,cx|s.dispatch(&Action::Edit(Command::Contents{id,edit:ContentsEdit::Promote}),w,cx)))));
        }
        let Content::ShapeContents(contents) = layer.content() else {
            return root;
        };
        let composition = self.state.read(cx).editor.project().active_composition_id();
        let parent = self
            .selected
            .and_then(|item| {
                contents
                    .rows()
                    .into_iter()
                    .find(|(_, _, n)| n.id == item)
                    .map(|(_, p, n)| {
                        if matches!(n.kind, ContentsKind::Group(_)) {
                            item
                        } else {
                            p
                        }
                    })
            })
            .unwrap_or(0);
        root = root.child(
            ui::text_button("contents-add", "Add ▾")
                .when(self.selection.items.len() > 1 || locked, |b| b.opacity(0.4))
                .when(!locked && self.selection.items.len() <= 1, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| {
                        this.tree_drag = None;
                        this.add_open = !this.add_open;
                        cx.notify();
                    }))
                }),
        );
        if self.add_open {
            for (index, kind) in [
                ContentsKind::Group(vec![]),
                ContentsKind::Parametric(ShapeKind::Rectangle),
                ContentsKind::Parametric(ShapeKind::RoundedRectangle),
                ContentsKind::Parametric(ShapeKind::Ellipse),
                ContentsKind::Parametric(ShapeKind::Polygon),
                ContentsKind::Parametric(ShapeKind::Star),
                ContentsKind::Fill { even_odd: false },
                ContentsKind::Stroke(Default::default()),
                ContentsKind::GradientFill {
                    even_odd: false,
                    gradient: ShapeGradient::default(),
                },
                ContentsKind::GradientStroke {
                    style: ShapeStroke::default(),
                    gradient: ShapeGradient::default(),
                },
                ContentsKind::TrimPaths,
            ]
            .into_iter()
            .enumerate()
            {
                let state = self.state.clone();
                let label = kind.label();
                root = root.child(
                    ui::text_button(("contents-add-kind", index), label).on_click(cx.listener(
                        move |this, _, w, cx| {
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::Contents {
                                        id,
                                        edit: ContentsEdit::Add {
                                            parent,
                                            kind: kind.clone(),
                                        },
                                    }),
                                    w,
                                    cx,
                                )
                            });
                            this.add_open = false;
                            cx.notify();
                        },
                    )),
                );
            }
        }
        let tree_active = self.tree_focus.contains_focused(w, cx);
        let visible = tree_selection::visible_rows(contents, &self.collapsed);
        self.tree_rows
            .borrow_mut()
            .retain(|id, _| visible.iter().any(|(_, _, item)| item == id));
        let marker = self
            .tree_drag
            .as_ref()
            .and_then(|d| d.gap)
            .and_then(|g| tree::marker_row(g, &visible));
        let tree_owner = cx.entity();
        let mut tree = div()
            .id("contents-tree")
            .track_focus(&self.tree_focus)
            .tab_index(0)
            .relative()
            .flex()
            .flex_col()
            .min_h(px(26.))
            .on_key_down(cx.listener(Self::tree_key))
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, w, _| {
                        // Register before the first press, so a fast down/up cannot lose release.
                        let moving = tree_owner.clone();
                        w.on_mouse_event(move |e: &gpui::MouseMoveEvent, phase, w, cx| {
                            if phase.bubble() {
                                moving.update(cx, |this, cx| this.tree_move(e, w, cx));
                            }
                        });
                        let ending = tree_owner.clone();
                        w.on_mouse_event(move |e: &gpui::MouseUpEvent, phase, w, cx| {
                            if phase.bubble() && e.button == MouseButton::Left {
                                ending.update(cx, |this, cx| this.tree_up(e.position, w, cx));
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        for &(depth, parent, item) in &visible {
            let node = contents.node(item).unwrap();
            let is_group = matches!(node.kind, ContentsKind::Group(_));
            let collapsed = self.collapsed.contains(&item);
            let state = self.state.clone();
            let enabled = node.enabled;
            let row_bounds = self.tree_rows.clone();
            tree = tree.child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .pl(px(depth as f32 * 10.))
                    .h(px(26.)).flex_none()
                    .when(is_group, |d| {
                        d.child(
                            ui::tool(
                                ("contents-expand", item),
                                if collapsed { "chevron-right" } else { "chevron-down" },
                                "Expand or collapse group",
                                false,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tree_collapse(item, cx);
                                cx.stop_propagation();
                            })),
                        )
                    })
                    .child(
                        ui::tool(("contents-visible", item), "eye", "Toggle item visibility", enabled)
                        .when(!locked, |b| {
                            b.on_click(move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(&Action::Edit(Command::Contents { id, edit: ContentsEdit::Enabled { item, enabled: !enabled } }), w, cx)
                                });
                                cx.stop_propagation();
                            })
                        }),
                    )
                    .child(
                        div().id(("contents-item", item))
                            .px_2().h(px(25.)).flex().items_center().cursor_pointer()
                            .hover(|b| b.bg(rgb(0x353535)))
                            .when(is_group, |b| b.tooltip(|_, cx| cx.new(|_| ui::Tip("Select this group to draw new Pen paths inside it. Drag its label to move the whole subtree between siblings. Order can change paint scope and overlap.".into())).into()))
                            .flex_1().min_w_0().justify_start()
                            .when(self.selection.items.contains(&item), |b| b.bg(rgb(if tree_active { 0x164a7b } else { 0x34383f })))
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event, w, cx| this.tree_down(id, parent, item, event, w, cx)))
                            .child(node.name.clone()),
                    )
                    .child(canvas(|_, _, _| (), move |bounds, _, w, _| {
                        row_bounds.borrow_mut().insert(item, tree::RowBounds { parent, bounds, visible: bounds.intersect(&w.content_mask().bounds) });
                    }).absolute().top_0().left_0().size_full())
                    .when(marker.is_some_and(|(id, _)| id == item), |row| {
                        row.child(div().absolute().left_0().right_0().h(px(2.)).bg(rgb(ui::BLUE))
                            .when(marker.is_some_and(|(_, after)| after), |line| line.bottom_0())
                            .when(marker.is_some_and(|(_, after)| !after), |line| line.top_0()))
                    }),
            );
        }
        tree = tree.child(self.hierarchy_actions(cx));
        root = root
            .child(tree)
            .child(div().text_size(px(11.)).child(format!(
                "{} selected · tree {}",
                self.selection.items.len(),
                if tree_active { "focused" } else { "inactive" }
            )))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(ui::MUTED))
                    .child("Ctrl-click toggles siblings · Shift-click selects a range"),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(ui::MUTED))
                    .child("Drag labels between siblings · order may change paint scope"),
            );
        if !self.selection.items.is_empty() {
            root = root.child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(ui::MUTED))
                    .child(MOVE_HELP),
            );
        }
        let Some(item) = self.selected else {
            return root.child(
                div()
                    .text_size(px(11.))
                    .child(if self.selection.items.len() > 1 {
                        "Select one item for editing controls"
                    } else {
                        "Select a Contents item to edit"
                    }),
            );
        };
        let Some(node) = contents.node(item) else {
            return root;
        };
        if !self
            .fields
            .iter()
            .map(|(p, _)| *p)
            .eq(node.parameter_order())
        {
            self.select(id, item, cx);
        }
        if let Some(name) = &self.name {
            name.update(cx, |f, _| {
                f.sync(format!("contents-name-{id}-{item}"), node.name.clone(), w)
            });
            root = root.child(
                div()
                    .mt_2()
                    .when(!locked, |d| d.child(name.clone()))
                    .when(locked, |d| d.child(node.name.clone())),
            );
        }
        let (_, parent, _) = contents
            .rows()
            .into_iter()
            .find(|(_, _, n)| n.id == item)
            .unwrap();
        let siblings = if parent == 0 {
            &contents.items
        } else {
            let ContentsKind::Group(v) = &contents.node(parent).unwrap().kind else {
                unreachable!()
            };
            v
        };
        let index = siblings.iter().position(|n| n.id == item).unwrap();
        let mut actions = div().flex().gap_1();
        for (key, icon, label, edit, allowed) in [
            (
                0usize,
                "arrow-up",
                "Move earlier",
                ContentsEdit::Move {
                    item,
                    parent,
                    index: index.saturating_sub(1),
                },
                index > 0,
            ),
            (
                1,
                "arrow-down",
                "Move later",
                ContentsEdit::Move {
                    item,
                    parent,
                    index: index + 1,
                },
                index + 1 < siblings.len(),
            ),
            (
                2,
                "copy",
                "Duplicate item",
                ContentsEdit::Duplicate(item),
                true,
            ),
            (
                3,
                "trash-bin",
                "Delete item",
                ContentsEdit::Remove(item),
                true,
            ),
        ] {
            let state = self.state.clone();
            actions = actions.child(ui::tool(("contents-action", key), icon, label, false).when(
                !locked && allowed,
                |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: edit.clone(),
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                },
            ));
        }
        root = root.child(actions);
        if matches!(node.kind, ContentsKind::TrimPaths) {
            root = root.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_size(px(11.))
                    .child("Each source contour")
                    .child(div().text_color(rgb(ui::MUTED)).child(TRIM_HELP)),
            );
        }
        if node.kind.is_paint() {
            for picker in 0..2 {
                let current = if picker == 0 {
                    PaintBlend::ALL
                        .iter()
                        .position(|m| *m == node.blend)
                        .unwrap()
                } else {
                    usize::from(node.composite == PaintComposite::AbovePrevious)
                };
                let label = if picker == 0 {
                    node.blend.label()
                } else if current == 0 {
                    "Below Previous"
                } else {
                    "Above Previous"
                };
                let bounds = self.paint_bounds[picker].clone();
                root = root.child(
                    div()
                        .flex()
                        .items_center()
                        .h(px(27.))
                        .child(div().w(px(70.)).child(if picker == 0 {
                            "Blend Mode"
                        } else {
                            "Composite"
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .relative()
                                .child(
                                    ui::text_button(
                                        ("contents-paint-picker", picker),
                                        format!("{label} ▾"),
                                    )
                                    .track_focus(&self.paint_focus[picker])
                                    .w_full()
                                    .justify_start()
                                    .when(locked, |b| b.opacity(0.4))
                                    .on_click(cx.listener(move |this, e, w, cx| {
                                        if locked {
                                            return;
                                        }
                                        TextField::commit_active(w, cx);
                                        w.focus(&this.paint_focus[picker]);
                                        if let Some(m) = &this.paint_menu {
                                            if m.picker == picker {
                                                if matches!(e, gpui::ClickEvent::Keyboard(_)) {
                                                    this.choose_paint(m.cursor, w, cx);
                                                } else {
                                                    this.paint_menu = None;
                                                }
                                                cx.notify();
                                                return;
                                            }
                                        }
                                        this.paint_menu = Some(PaintMenu {
                                            layer: id,
                                            item,
                                            revision: this.state.read(cx).document_revision,
                                            picker,
                                            cursor: current,
                                        });
                                        cx.notify();
                                        cx.stop_propagation();
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, e: &gpui::KeyDownEvent, _, cx| {
                                            if locked || e.keystroke.modifiers.modified() {
                                                return;
                                            }
                                            let key = e.keystroke.key.as_str();
                                            if key == "escape" {
                                                this.paint_menu = None;
                                            } else if ["up", "down", "home", "end"].contains(&key) {
                                                let m = this.paint_menu.get_or_insert_with(|| {
                                                    PaintMenu {
                                                        layer: id,
                                                        item,
                                                        revision: this
                                                            .state
                                                            .read(cx)
                                                            .document_revision,
                                                        picker,
                                                        cursor: current,
                                                    }
                                                });
                                                let count = paint_options(picker).len();
                                                m.cursor = match key {
                                                    "up" => (m.cursor + count - 1) % count,
                                                    "down" => (m.cursor + 1) % count,
                                                    "home" => 0,
                                                    _ => count - 1,
                                                };
                                            } else {
                                                return;
                                            }
                                            cx.stop_propagation();
                                            cx.notify();
                                        },
                                    )),
                                )
                                .child(
                                    canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| ())
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full(),
                                ),
                        ),
                );
            }
        }
        if matches!(node.kind, ContentsKind::Parametric(_)) {
            let state = self.state.clone();
            root = root.child(
                ui::text_button("contents-to-path", "Convert To Bezier Path").when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::ConvertPath {
                                        item,
                                        frame: s.frame,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        if matches!(node.kind, ContentsKind::Path { .. }) {
            root = root.child(super::path_controls::row(
                &self.state,
                &layer,
                PathTarget::Contents(item),
                frame,
            ));
        }
        if let ContentsKind::Fill { even_odd } | ContentsKind::GradientFill { even_odd, .. } =
            node.kind
        {
            let state = self.state.clone();
            root = root.child(
                ui::text_button(
                    "contents-fill-rule",
                    if even_odd {
                        "Fill rule: Even-Odd"
                    } else {
                        "Fill rule: Non-Zero"
                    },
                )
                .when(!locked, |b| {
                    b.on_click(move |_, w, cx| {
                        state.update(cx, |s, cx| {
                            s.dispatch(
                                &Action::Edit(Command::Contents {
                                    id,
                                    edit: ContentsEdit::FillRule {
                                        item,
                                        even_odd: !even_odd,
                                    },
                                }),
                                w,
                                cx,
                            )
                        })
                    })
                }),
            );
        }
        if let Some(color) = node.paint_color_at(frame) {
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child("Color")
                    .child(super::color_picker::swatch(
                        "contents-paint-color",
                        color,
                        crate::color_edit::Target::Contents(id, item),
                        locked,
                        &self.state,
                    )),
            );
        }
        if let Some(gradient) = node.kind.gradient() {
            let target = crate::color_edit::GradientTarget::Contents(composition, id, item);
            let active = self.state.read(cx).gradient_controls == Some(target);
            root = root.child(
                ui::text_button("contents-gradient-points", "Edit gradient in Composition")
                    .when(active, |b| b.bg(rgb(0x164a7b)))
                    .when(locked || !node.enabled, |b| b.opacity(0.4))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        TextField::commit_active(w, cx);
                        this.cancel_ramp(cx);
                        if this.ramp_node(cx).is_none() {
                            return;
                        }
                        this.state.update(cx, |s, cx| {
                            s.dispatch(&Action::Seek(s.frame), w, cx);
                            s.tool = crate::editor::Tool::Select;
                            s.gradient_controls = if s.gradient_controls == Some(target) {
                                None
                            } else {
                                Some(target)
                            };
                            cx.notify();
                        });
                    })),
            );
            let state = self.state.clone();
            root = root.child(
                ui::text_button("open-gradient-editor", "Edit Gradient…")
                    .when(locked, |b| b.opacity(0.4))
                    .when(!locked, |b| {
                        b.on_click(move |_, w, cx| {
                            TextField::commit_active(w, cx);
                            state
                                .update(cx, |s, cx| s.dispatch(&Action::OpenGradient(item), w, cx));
                        })
                    }),
            );
            root = root.child(self.gradient_ramp(node, frame, locked, cx));
            let mut types = div().flex().gap_1().child("Type");
            for (index, radial) in [false, true].into_iter().enumerate() {
                let state = self.state.clone();
                types = types.child(
                    ui::text_button(
                        ("gradient-type", index),
                        if radial { "Radial" } else { "Linear" },
                    )
                    .when(gradient.radial == radial, |b| b.bg(rgb(0x164a7b)))
                    .when(!locked, |b| {
                        b.on_click(move |_, w, cx| {
                            TextField::commit_active(w, cx);
                            state.update(cx, |s, cx| {
                                s.dispatch(
                                    &Action::Edit(Command::Contents {
                                        id,
                                        edit: ContentsEdit::GradientType { item, radial },
                                    }),
                                    w,
                                    cx,
                                )
                            });
                        })
                    }),
                );
            }
            root = root.child(types);
            let ids = gradient
                .colors
                .iter()
                .chain(&gradient.opacities)
                .copied()
                .collect::<Vec<_>>();
            if self.gradient_stop.is_none_or(|id| !ids.contains(&id)) {
                self.gradient_stop = ids.first().copied();
            }
            for (opacity, stops) in [(false, &gradient.colors), (true, &gradient.opacities)] {
                let mut row = div().flex().flex_wrap().gap_1();
                for &stop in stops {
                    row = row.child(
                        ui::text_button(
                            gpui::SharedString::from(format!("gradient-stop-{stop}")),
                            stop.to_string(),
                        )
                        .when(self.gradient_stop == Some(stop), |b| b.bg(rgb(0x164a7b)))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            TextField::commit_active(w, cx);
                            this.gradient_stop = Some(stop);
                            this.ramp_selected = None;
                            cx.notify();
                        })),
                    );
                }
                let state = self.state.clone();
                row = row.child(
                    ui::text_button(
                        if opacity {
                            "gradient-add-opacity"
                        } else {
                            "gradient-add-color"
                        },
                        "+",
                    )
                    .when(
                        !locked && stops.len() < ShapeGradient::MAX_STOPS,
                        |b| {
                            b.on_click(cx.listener(move |this, _, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: ContentsEdit::AddGradientStop {
                                                item,
                                                opacity,
                                                position: 50.,
                                                frame: s.frame,
                                            },
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                                let layer = state.read(cx).editor.selected_layer();
                                if let Some(Content::ShapeContents(c)) = layer.map(|l| l.content())
                                {
                                    if let Some(g) = c.node(item).and_then(|n| n.kind.gradient()) {
                                        this.gradient_stop = if opacity {
                                            g.opacities.last().copied()
                                        } else {
                                            g.colors.last().copied()
                                        };
                                    }
                                }
                                cx.notify();
                            }))
                        },
                    ),
                );
                root = root.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(if opacity {
                            "Opacity stops"
                        } else {
                            "Color stops"
                        })
                        .child(row),
                );
            }
            if let Some(stop) = self.gradient_stop {
                if let Some(color) = gradient.color_at(node, stop, frame) {
                    root = root.child(super::color_picker::swatch(
                        "gradient-stop-color",
                        color,
                        crate::color_edit::Target::GradientStop(id, item, stop),
                        locked,
                        &self.state,
                    ));
                }
                let state = self.state.clone();
                let can_remove = if gradient.colors.contains(&stop) {
                    gradient.colors.len() > 2
                } else {
                    gradient.opacities.len() > 2
                };
                root = root.child(
                    ui::text_button("gradient-remove-stop", "Remove selected stop")
                        .when(locked || !can_remove, |b| b.opacity(0.4))
                        .when(!locked && can_remove, |b| {
                            b.on_click(move |_, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: ContentsEdit::RemoveGradientStop { item, stop },
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                            })
                        }),
                );
            }
        }
        if let Some(style) = node.kind.stroke() {
            for (cap_row, label) in [(true, "Line Cap"), (false, "Line Join")] {
                let mut options = div().flex().gap_1();
                for index in 0..3 {
                    let (name, active, edit) = if cap_row {
                        let cap = StrokeCap::ALL[index];
                        (
                            cap.label(),
                            style.cap == cap,
                            ContentsEdit::StrokeCap { item, cap },
                        )
                    } else {
                        let join = StrokeJoin::ALL[index];
                        (
                            join.label(),
                            style.join == join,
                            ContentsEdit::StrokeJoin { item, join },
                        )
                    };
                    let state = self.state.clone();
                    options = options.child(
                        ui::text_button(
                            (
                                if cap_row {
                                    "contents-cap"
                                } else {
                                    "contents-join"
                                },
                                index,
                            ),
                            name,
                        )
                        .when(active, |b| b.bg(rgb(0x164a7b)))
                        .when(!locked, |b| {
                            b.on_click(move |_, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: edit.clone(),
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                            })
                        }),
                    );
                }
                root = root.child(div().flex().flex_col().gap_1().child(label).child(options));
            }
            let mut dashes = div().flex().items_center().gap_1().child("Dashes");
            for (key, icon, label, edit, enabled) in [
                (
                    0usize,
                    "plus",
                    "Add dash or gap",
                    ContentsEdit::AddDash(item),
                    style.dashes.len() < ShapeStroke::MAX_DASHES,
                ),
                (
                    1,
                    "minus",
                    "Remove last dash or gap",
                    ContentsEdit::RemoveDash(item),
                    !style.dashes.is_empty(),
                ),
            ] {
                let state = self.state.clone();
                dashes = dashes.child(
                    ui::tool(("contents-dashes", key), icon, label, false)
                        .when(locked || !enabled, |b| b.opacity(0.4))
                        .when(!locked && enabled, |b| {
                            b.on_click(move |_, w, cx| {
                                TextField::commit_active(w, cx);
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::Contents {
                                            id,
                                            edit: edit.clone(),
                                        }),
                                        w,
                                        cx,
                                    )
                                });
                            })
                        }),
                );
            }
            root = root.child(dashes);
        }
        for (p, field) in &self.fields {
            if let ContentsParam::Gradient(param) = p {
                if param
                    .stop()
                    .is_some_and(|stop| self.gradient_stop != Some(stop))
                {
                    continue;
                }
                if matches!(
                    param,
                    GradientParam::HighlightLength | GradientParam::HighlightAngle
                ) && node.kind.gradient().is_some_and(|g| !g.radial)
                {
                    continue;
                }
            }
            let Some(track) = node.parameters.get(p) else {
                continue;
            };
            let property = PropertyPath::Contents {
                item,
                parameter: *p,
            };
            let value = node.value_at(*p, frame).to_string();
            field.update(cx, |f, _| {
                f.sync(
                    format!("contents-{id}-{item}-{p:?}-{frame}"),
                    value.clone(),
                    w,
                )
            });
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(27.))
                    .child(ui::action_tool(
                        gpui::SharedString::from(format!("contents-watch-{p:?}")),
                        "stopwatch",
                        "Toggle animation",
                        &self.state,
                        Action::Edit(Command::EditTrack {
                            id,
                            property,
                            edit: TrackEdit::ToggleAnimation { frame },
                        }),
                        !track.keys().is_empty(),
                    ))
                    .child(
                        ui::text_button(
                            gpui::SharedString::from(format!("contents-label-{p:?}")),
                            p.label(),
                        )
                        .flex_1()
                        .min_w_0()
                        .justify_start()
                        .on_click({
                            let state = self.state.clone();
                            move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(&Action::GraphProperty(id, property), w, cx)
                                })
                            }
                        }),
                    )
                    .child(
                        div()
                            .w(px(72.))
                            .when(!locked, |d| d.child(field.clone()))
                            .when(locked, |d| d.child(value)),
                    )
                    .when(matches!(p, ContentsParam::Trim(_)), |row| {
                        row.child(ui::action_tool(
                            gpui::SharedString::from(format!("contents-key-{p:?}")),
                            "diamond",
                            "Add or remove key at playhead",
                            &self.state,
                            Action::Edit(Command::EditTrack {
                                id,
                                property,
                                edit: TrackEdit::ToggleKey { frame },
                            }),
                            track.keys().contains_key(&frame),
                        ))
                    }),
            );
        }
        if let Some(m) = &self.paint_menu {
            let position = self.paint_bounds[m.picker]
                .get()
                .map_or(point(px(0.), px(0.)), |b| point(b.left(), b.bottom()));
            let mut menu = div()
                .id("contents-paint-menu")
                .w(px(230.))
                .py_1()
                .bg(rgb(ui::PANEL))
                .border_1()
                .border_color(rgb(ui::BLUE))
                .occlude()
                .on_mouse_down(gpui::MouseButton::Left, |_, w, cx| {
                    w.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.paint_menu = None;
                    cx.notify();
                }));
            for (index, label) in paint_options(m.picker).into_iter().enumerate() {
                menu = menu.child(
                    div()
                        .id(("contents-paint-option", index))
                        .h(px(23.))
                        .px_2()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child(label)
                        .when(index == m.cursor, |b| b.bg(rgb(0x344455)))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.choose_paint(index, w, cx);
                            cx.stop_propagation();
                        })),
                );
            }
            root = root.child(deferred(
                anchored()
                    .position(position)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            ));
        }
        root
    }
}

#[cfg(test)]
mod trim_controls_tests {
    use super::*;
    use libre_effects_core::{ContentsNode, TrimParam};

    fn fixture() -> (EditorState, ContentsFieldTarget) {
        let mut state = EditorState::default();
        state
            .editor
            .execute(Command::AddContent {
                content: Content::Shape(Default::default()),
                width: 200.,
                height: 120.,
                name: "Trim controls fixture".into(),
            })
            .unwrap();
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Promote,
            })
            .unwrap();
        state
            .editor
            .execute(Command::Contents {
                id: 1,
                edit: ContentsEdit::Add {
                    parent: 1,
                    kind: ContentsKind::TrimPaths,
                },
            })
            .unwrap();
        let Content::ShapeContents(contents) = state.editor.selected_layer().unwrap().content()
        else {
            panic!("expected Contents");
        };
        let item = contents
            .rows()
            .into_iter()
            .find(|(_, _, node)| matches!(node.kind, ContentsKind::TrimPaths))
            .unwrap()
            .2
            .id;
        let target = ContentsFieldTarget {
            composition: state.editor.project().active_composition_id(),
            layer: 1,
            item,
            revision: state.document_revision,
        };
        state.contents_selection = Some((target.composition, target.layer, target.item));
        (state, target)
    }

    fn node(state: &EditorState, item: u64) -> &ContentsNode {
        let Content::ShapeContents(contents) = state.editor.selected_layer().unwrap().content()
        else {
            panic!("expected Contents");
        };
        contents.node(item).unwrap()
    }

    #[test]
    fn trim_controls_order_and_selection_keep_source_path_controls_separate() {
        let (state, target) = fixture();
        let node = node(&state, target.item);
        assert_eq!(
            node.parameter_order(),
            [TrimParam::Start, TrimParam::End, TrimParam::Offset].map(ContentsParam::Trim)
        );
        assert_eq!(
            node.parameter_order()
                .into_iter()
                .map(|p| node.value_at(p, 0))
                .collect::<Vec<_>>(),
            [0., 100., 0.]
        );
        assert!(!matches!(node.kind, ContentsKind::Path { .. }));
        assert!(node.path_at(0).is_none());
        assert!(
            state
                .editor
                .selected_layer()
                .unwrap()
                .track(PropertyPath::Path(PathTarget::Contents(target.item)))
                .is_none()
        );
        let Content::ShapeContents(contents) = state.editor.selected_layer().unwrap().content()
        else {
            unreachable!();
        };
        let mut selection = Selection::default();
        selection.one(1, target.item);
        selection.reconcile(contents, &Default::default());
        assert_eq!(selection.singleton(), Some(target.item));
        assert_eq!(selection.parent, Some(1));
        assert!(TRIM_HELP.contains("each source contour"));
        assert!(TRIM_HELP.contains("Source paths remain editable"));
    }

    #[test]
    fn trim_field_commands_reject_stale_selection_and_use_the_current_frame() {
        let (mut state, target) = fixture();
        let parameter = ContentsParam::Trim(TrimParam::End);
        for stale in [
            ContentsFieldTarget {
                composition: target.composition + 1,
                ..target
            },
            ContentsFieldTarget {
                layer: 999,
                ..target
            },
            ContentsFieldTarget {
                item: target.item + 1,
                ..target
            },
            ContentsFieldTarget {
                revision: target.revision + 1,
                ..target
            },
        ] {
            assert!(stale.value_command(&state, parameter, 50.).is_none());
        }
        state.contents_selection = None;
        assert!(target.value_command(&state, parameter, 50.).is_none());
        state.contents_selection = Some((target.composition, target.layer, target.item));
        state.frame = 13;
        assert!(matches!(
            target.value_command(&state, parameter, 50.),
            Some(Command::Contents {
                id: 1,
                edit: ContentsEdit::Track {
                    edit: TrackEdit::Value {
                        frame: 13,
                        value: 50.
                    },
                    ..
                }
            })
        ));
        state.editor.clear_selection();
        assert!(target.value_command(&state, parameter, 50.).is_none());
    }

    #[test]
    fn same_displayed_trim_field_value_preserves_bytes_redo_and_in_between_keys() {
        for (parameter, end) in [
            (TrimParam::Start, 40.),
            (TrimParam::End, 60.),
            (TrimParam::Offset, 720.),
        ] {
            let (mut state, target) = fixture();
            let parameter = ContentsParam::Trim(parameter);
            let property = PropertyPath::Contents {
                item: target.item,
                parameter,
            };
            state
                .editor
                .execute(Command::EditTrack {
                    id: target.layer,
                    property,
                    edit: TrackEdit::ToggleAnimation { frame: 0 },
                })
                .unwrap();
            state
                .editor
                .execute(Command::EditTrack {
                    id: target.layer,
                    property,
                    edit: TrackEdit::Value {
                        frame: 20,
                        value: end,
                    },
                })
                .unwrap();
            state.editor.clear_history();
            state
                .editor
                .execute(Command::RenameLayer {
                    id: target.layer,
                    name: "Undo this rename".into(),
                })
                .unwrap();
            state.editor.undo();
            state.frame = 10;
            let displayed = node(&state, target.item).value_at(parameter, state.frame);
            let source = state.editor.project().to_json().unwrap();
            let command = target.value_command(&state, parameter, displayed).unwrap();
            state.editor.execute(command).unwrap();
            assert_eq!(state.editor.project().to_json().unwrap(), source);
            assert!(!state.editor.can_undo());
            assert!(state.editor.can_redo());
            assert!(
                !node(&state, target.item).parameters[&parameter]
                    .keys()
                    .contains_key(&10)
            );
            // The explicit diamond intentionally creates the key at the same value.
            state
                .editor
                .execute(Command::EditTrack {
                    id: target.layer,
                    property,
                    edit: TrackEdit::ToggleKey { frame: state.frame },
                })
                .unwrap();
            assert!(
                node(&state, target.item).parameters[&parameter]
                    .keys()
                    .contains_key(&10)
            );
            assert_eq!(node(&state, target.item).value_at(parameter, 10), displayed);
            assert!(state.editor.can_undo());
            assert!(!state.editor.can_redo());
            state.editor.undo();
            assert_eq!(state.editor.project().to_json().unwrap(), source);
        }
    }

    #[test]
    fn trim_pending_value_then_stopwatch_or_key_uses_normal_transactions() {
        for edit in [
            TrackEdit::ToggleAnimation { frame: 7 },
            TrackEdit::ToggleKey { frame: 7 },
        ] {
            let (mut state, target) = fixture();
            let parameter = ContentsParam::Trim(TrimParam::End);
            state.frame = 7;
            let command = target.value_command(&state, parameter, 25.).unwrap();
            state.editor.execute(command).unwrap();
            state.document_revision += 1;
            assert!(target.value_command(&state, parameter, 40.).is_none());
            // Existing action_tool routing has no stale field-revision gate.
            state
                .editor
                .execute(Command::EditTrack {
                    id: target.layer,
                    property: PropertyPath::Contents {
                        item: target.item,
                        parameter,
                    },
                    edit,
                })
                .unwrap();
            let track = &node(&state, target.item).parameters[&parameter];
            assert_eq!(track.keys().len(), 1);
            assert!(track.keys().contains_key(&7));
            assert_eq!(track.value_at(7), 25.);
        }
    }

    #[test]
    fn trim_field_values_still_reach_core_lock_range_and_frame_guards() {
        let (mut state, target) = fixture();
        let parameter = ContentsParam::Trim(TrimParam::Start);
        for value in [-1., 101., f64::NAN, f64::INFINITY] {
            let source = state.editor.project().clone();
            let command = target.value_command(&state, parameter, value).unwrap();
            assert!(state.editor.execute(command).is_err());
            assert_eq!(state.editor.project(), &source);
        }
        state.frame = state.editor.project().composition().duration();
        let source = state.editor.project().clone();
        let command = target.value_command(&state, parameter, 0.).unwrap();
        assert!(state.editor.execute(command).is_err());
        assert_eq!(state.editor.project(), &source);
        state.frame = 0;
        state
            .editor
            .execute(Command::ToggleLocked(target.layer))
            .unwrap();
        let source = state.editor.project().clone();
        let command = target.value_command(&state, parameter, 0.).unwrap();
        assert!(state.editor.execute(command).is_err());
        assert_eq!(state.editor.project(), &source);
    }
}
