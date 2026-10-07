pub(crate) mod compound_colors;
mod filter_safety;
mod key_menu;
mod layer_filter;
mod layer_navigation;
mod layer_rename;
use super::parent_drag::ParentDrag;
use crate::color_edit::InputTarget;
use crate::timeline_filter::LayerTypeFilter;
use crate::view_state::GraphChannel;
use crate::{
    components::TextField,
    editor::{Action, EditorState, PropertyFilter},
    ui,
};
use gpui::{
    Bounds, Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels,
    SharedString, Window, canvas, div, fill, point, prelude::*, px, relative, rgb, size,
};
use libre_effects_core::{
    Command, KeyRef, LayerId, LayerSwitch, Property, PropertyPath, TextPaint, TextParam,
    TextSelectorParam, TrackEdit,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::{cell::Cell, rc::Rc};

fn parse_scalar(text: &str) -> Result<f64, std::num::ParseFloatError> {
    text.trim().parse::<f64>()
}

fn text_groups(layer: &libre_effects_core::Layer) -> Vec<(String, Vec<PropertyPath>)> {
    if !matches!(layer.content(), libre_effects_core::Content::Text { .. }) {
        return vec![];
    }
    let mut groups = vec![
        ("Source Text · Hold".into(), vec![PropertyPath::SourceText]),
        (
            "Fill Color".into(),
            TextPaint::Fill.channels().map(PropertyPath::Text).to_vec(),
        ),
        (
            "Stroke Color".into(),
            TextPaint::Stroke
                .channels()
                .map(PropertyPath::Text)
                .to_vec(),
        ),
        (
            "Stroke Width".into(),
            vec![PropertyPath::Text(TextParam::StrokeWidth)],
        ),
        (
            "Font Size".into(),
            vec![PropertyPath::Text(TextParam::FontSize)],
        ),
        (
            "Tracking".into(),
            vec![PropertyPath::Text(TextParam::Tracking)],
        ),
        (
            "Leading".into(),
            vec![PropertyPath::Text(TextParam::Leading)],
        ),
        (
            "Fill Opacity".into(),
            vec![PropertyPath::Text(TextParam::FillOpacity)],
        ),
        (
            "Stroke Opacity".into(),
            vec![PropertyPath::Text(TextParam::StrokeOpacity)],
        ),
        (
            "Range Start".into(),
            vec![PropertyPath::Text(TextParam::AnimatorStart)],
        ),
        (
            "Range End".into(),
            vec![PropertyPath::Text(TextParam::AnimatorEnd)],
        ),
        (
            "Range Offset".into(),
            vec![PropertyPath::Text(TextParam::AnimatorOffset)],
        ),
        (
            "Amount".into(),
            vec![PropertyPath::Text(TextParam::AnimatorAmount)],
        ),
        (
            "Scale X".into(),
            vec![PropertyPath::Text(TextParam::AnimatorScaleX)],
        ),
        (
            "Scale Y".into(),
            vec![PropertyPath::Text(TextParam::AnimatorScaleY)],
        ),
        (
            "Rotation".into(),
            vec![PropertyPath::Text(TextParam::AnimatorRotation)],
        ),
        (
            "Position X".into(),
            vec![PropertyPath::Text(TextParam::AnimatorPositionX)],
        ),
        (
            "Position Y".into(),
            vec![PropertyPath::Text(TextParam::AnimatorPositionY)],
        ),
        (
            "Opacity".into(),
            vec![PropertyPath::Text(TextParam::AnimatorOpacity)],
        ),
    ];
    for selector in layer.text_range_selectors() {
        groups.extend(TextSelectorParam::ALL.map(|parameter| {
            (
                parameter.label().to_string(),
                vec![PropertyPath::TextSelector {
                    selector: selector.id,
                    parameter,
                }],
            )
        }));
    }
    for animator in layer.text_animators() {
        groups.extend(super::text_animator::ANIMATOR_PARAMETERS.map(|parameter| {
            (
                parameter
                    .label()
                    .trim_start_matches("Animator · ")
                    .to_string(),
                vec![PropertyPath::TextAnimator {
                    animator: animator.id,
                    parameter,
                }],
            )
        }));
    }
    groups
}

fn text_channel_label(parameter: TextParam) -> &'static str {
    match parameter {
        TextParam::FillOpacity
        | TextParam::StrokeOpacity
        | TextParam::AnimatorStart
        | TextParam::AnimatorEnd
        | TextParam::AnimatorOffset
        | TextParam::AnimatorAmount
        | TextParam::AnimatorScaleX
        | TextParam::AnimatorScaleY
        | TextParam::AnimatorOpacity => "%",
        TextParam::AnimatorRotation => "°",
        TextParam::Tracking => "‰ em",
        TextParam::Leading => "×",
        _ => TextPaint::component_label(parameter).unwrap_or("px"),
    }
}

fn animator_section_label(layer: &libre_effects_core::Layer) -> String {
    let selector = layer.text_selector();
    format!(
        "Text Animator · {} · {}",
        selector.units.label(),
        selector.shape.label()
    )
}

fn group_animated(layer: &libre_effects_core::Layer, properties: &[PropertyPath]) -> bool {
    properties
        .iter()
        .any(|p| layer.track(*p).is_some_and(|t| !t.keys().is_empty()))
}
fn group_visible(
    layer: &libre_effects_core::Layer,
    properties: &[PropertyPath],
    filter: Option<PropertyFilter>,
) -> bool {
    // Native Position/Opacity timing has no editable legacy scalar lanes. Never
    // expose a partial group that could edit only the surviving components.
    if properties.iter().any(|property| {
        matches!(property, PropertyPath::Transform(_)) && layer.track(*property).is_none()
    }) {
        return false;
    }
    filter.is_none_or(|f| {
        properties.iter().any(|p| {
            (match p {
                PropertyPath::Transform(p) => f.includes(*p),
                _ => f == PropertyFilter::Animated,
            }) && (f != PropertyFilter::Animated
                || layer.track(*p).is_some_and(|t| !t.keys().is_empty()))
        })
    })
}

fn joined_position_visible(
    layer: &libre_effects_core::Layer,
    filter: Option<PropertyFilter>,
) -> bool {
    layer
        .spatial_position()
        .map(|p| p.keys.len())
        .or_else(|| layer.planar_position().map(|p| p.keys.len()))
        .is_some_and(|keys| {
            filter.is_none_or(|filter| {
                if filter == PropertyFilter::Animated {
                    keys != 0
                } else {
                    filter.includes(Property::PositionX)
                }
            })
        })
}
fn native_opacity_visible(
    layer: &libre_effects_core::Layer,
    filter: Option<PropertyFilter>,
) -> bool {
    layer.has_opacity_timing()
        && filter.is_none_or(|filter| {
            if filter == PropertyFilter::Animated {
                layer.opacity_key_count() != 0
            } else {
                filter.includes(Property::Opacity)
            }
        })
}
fn group_watch(
    layer: &libre_effects_core::Layer,
    properties: &[PropertyPath],
    frame: u32,
) -> Command {
    let animated = group_animated(layer, properties);
    Command::Batch(
        properties
            .iter()
            .filter(|p| layer.track(**p).is_none_or(|t| t.keys().is_empty()) == !animated)
            .map(|p| Command::EditTrack {
                id: layer.id(),
                property: *p,
                edit: TrackEdit::ToggleAnimation { frame },
            })
            .collect(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceTextControl {
    Animation,
    Key,
    Edit,
    Previous,
    Next,
}

fn source_text_summary(layer: &libre_effects_core::Layer, frame: u32) -> String {
    let Some(text) = layer.source_text_at(frame) else {
        return String::new();
    };
    if text.is_empty() {
        return "(empty)".into();
    }
    let mut chars = text.chars();
    let mut summary: String = chars
        .by_ref()
        .take(24)
        .map(|c| match c {
            '\r' | '\n' => '↵',
            '\t' => '⇥',
            other => other,
        })
        .collect();
    if chars.next().is_some() {
        summary.push('…');
    }
    summary
}

fn source_text_action(
    layer: &libre_effects_core::Layer,
    frame: u32,
    control: SourceTextControl,
) -> Option<Action> {
    if layer.locked() || layer.source_text_at(frame).is_none() {
        return None;
    }
    let track = layer.track(PropertyPath::SourceText)?;
    Some(match control {
        SourceTextControl::Animation | SourceTextControl::Key => Action::Edit(Command::EditTrack {
            id: layer.id(),
            property: PropertyPath::SourceText,
            edit: if control == SourceTextControl::Animation {
                TrackEdit::ToggleAnimation { frame }
            } else {
                TrackEdit::ToggleKey { frame }
            },
        }),
        SourceTextControl::Edit => Action::BeginText(Some(layer.id()), [0.0; 2]),
        SourceTextControl::Previous => Action::Seek(*track.keys().range(..frame).next_back()?.0),
        SourceTextControl::Next => Action::Seek(
            *track
                .keys()
                .range((std::ops::Bound::Excluded(frame), std::ops::Bound::Unbounded))
                .next()?
                .0,
        ),
    })
}

/// Source Text never exposes its opaque pool value as a numeric field. Buttons
/// share Properties' pointer-before-blur guard, then replan from the committed
/// current sample. Navigation and edits cannot reuse a stale frame/layer draft.
pub(super) fn source_text_control(
    state: &Entity<EditorState>,
    layer: &libre_effects_core::Layer,
    frame: u32,
    kind: SourceTextControl,
    target: Option<InputTarget>,
    scope: &'static str,
) -> impl IntoElement {
    let id = layer.id();
    let control = format!("{scope}-source-text-{id}-{kind:?}");
    let state = state.clone();
    let track = layer.track(PropertyPath::SourceText);
    let (icon, tip, active) = match kind {
        SourceTextControl::Animation => (
            "stopwatch",
            "Toggle Source Text animation · Hold only",
            track.is_some_and(|t| !t.keys().is_empty()),
        ),
        SourceTextControl::Key => (
            "diamond",
            "Add / remove Source Text key · Hold only",
            track.is_some_and(|t| t.keys().contains_key(&frame)),
        ),
        SourceTextControl::Edit => (
            "text",
            "Edit current Source Text on canvas · Hold only",
            false,
        ),
        SourceTextControl::Previous => ("arrow-left", "Previous Source Text key", false),
        SourceTextControl::Next => ("arrow-right", "Next Source Text key", false),
    };
    let button = if kind == SourceTextControl::Edit {
        ui::text_button(
            SharedString::from(control.clone()),
            source_text_summary(layer, frame),
        )
        .w(px(100.0))
        .min_w_0()
        .overflow_hidden()
        .justify_start()
        .text_size(px(10.0))
        .tooltip(move |_, cx| cx.new(|_| ui::Tip(tip.into())).into())
    } else {
        ui::tool(SharedString::from(control.clone()), icon, tip, active)
    };
    crate::color_edit::input_pointer_button(button, control.clone(), target.clone()).on_click(
        move |event, window, cx| {
            cx.stop_propagation();
            let Some(target) =
                crate::color_edit::input_click_target(&control, event, &target, &state, window, cx)
            else {
                return;
            };
            if state.read(cx).editor.selected() != Some(id) {
                return;
            }
            TextField::commit_active(window, cx);
            state.update(cx, |s, cx| {
                if !target.same_context(s) {
                    return;
                }
                s.finish_text(true, cx);
                if !target.same_context(s) {
                    return;
                }
                let action = s
                    .editor
                    .selected_layer()
                    .and_then(|layer| source_text_action(layer, s.frame, kind));
                if let Some(action) = action {
                    if kind == SourceTextControl::Edit {
                        s.graph_open = false;
                    }
                    s.dispatch(&action, window, cx);
                }
            });
        },
    )
}

const LEFT: f32 = 560.0;
#[derive(Clone)]
struct KeyDrag {
    from: u32,
    to: u32,
}
pub(crate) struct Timeline {
    waveforms: Entity<super::audio_waveform::AudioWaveforms>,
    matte_pickers: BTreeMap<
        LayerId,
        (
            Entity<super::matte::MattePicker>,
            Entity<super::matte::MattePicker>,
        ),
    >,
    blend_pickers: BTreeMap<LayerId, Entity<super::blend::BlendPicker>>,
    left: f32,
    resizing: bool,
    fields:
        BTreeMap<(LayerId, PropertyPath), (Entity<TextField>, Rc<RefCell<Option<InputTarget>>>)>,
    input_source: Option<InputTarget>,
    animator: super::text_animator::TimelineAnimator,
    colors: compound_colors::TimelineColors,
    parent_open: Option<LayerId>,
    bar_drag: Option<(Vec<LayerId>, i32, f64, i64)>,
    marquee: Option<(gpui::Point<Pixels>, gpui::Point<Pixels>)>,
    marquee_additive: bool,
    hit_keys: Rc<RefCell<Vec<(KeyRef, Bounds<Pixels>)>>>,
    hit_layers: Rc<RefCell<Vec<(LayerId, Bounds<Pixels>)>>>,
    search: Entity<TextField>,
    layer_type: LayerTypeFilter,
    selected_only: bool,
    type_open: bool,
    type_cursor: usize,
    type_focus: FocusHandle,
    type_scroll: gpui::ScrollHandle,
    filter_context: Option<(u64, libre_effects_core::CompositionId)>,
    filter_signature: (String, LayerTypeFilter, bool),
    visible_layers: BTreeSet<LayerId>,
    layer_navigation: libre_effects_editor_model::timeline_navigation::LayerNavigation,
    row_scroll: gpui::ScrollHandle,
    reveal_layer: Option<LayerId>,
    rename: Option<layer_rename::RenameInput>,
    graph: Entity<super::graph::Graph>,
    marker_editor: Entity<super::markers::MarkerEditor>,
    state: Entity<EditorState>,
    ruler: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    scrubbing: bool,
    snapped_to: Option<u32>,
    drag: Option<KeyDrag>,
    selected_key: Option<(LayerId, PropertyPath, u32)>,
    key_menu: Option<key_menu::Menu>,
    menu_focus_watch: Option<[gpui::Subscription; 3]>,
}
fn frame_at(x: f32, left: f32, width: f32, start: u32, visible: u32, duration: u32) -> f64 {
    (f64::from(start)
        + ((f64::from(x) - f64::from(left)) / f64::from(width.max(1.0))).clamp(0.0, 1.0)
            * f64::from(visible))
    .min(f64::from(duration - 1))
}
impl Timeline {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |this, _, cx| {
            if this
                .key_menu
                .as_ref()
                .is_some_and(|menu| !menu.valid(this.state.read(cx)))
            {
                this.key_menu = None;
            }
            cx.notify();
        })
        .detach();
        let focus = cx.focus_handle();
        let search = cx.new(|cx| TextField::new(cx, |_, _, _| {}).return_focus(focus.clone()));
        cx.observe(&search, |this, _, cx| {
            this.cancel_filtered_gestures();
            cx.notify();
        })
        .detach();
        let waveforms = cx.new(|_| super::audio_waveform::AudioWaveforms::new());
        cx.observe(&waveforms, |_, _, cx| cx.notify()).detach();
        Self {
            waveforms,
            blend_pickers: BTreeMap::new(),
            matte_pickers: BTreeMap::new(),
            left: LEFT,
            resizing: false,
            fields: BTreeMap::new(),
            input_source: None,
            animator: Default::default(),
            colors: Default::default(),
            parent_open: None,
            bar_drag: None,
            marquee: None,
            marquee_additive: false,
            hit_keys: Default::default(),
            hit_layers: Default::default(),
            search,
            layer_type: LayerTypeFilter::All,
            selected_only: false,
            type_open: false,
            type_cursor: 0,
            type_focus: cx.focus_handle(),
            type_scroll: gpui::ScrollHandle::new(),
            filter_context: None,
            filter_signature: (String::new(), LayerTypeFilter::All, false),
            visible_layers: BTreeSet::new(),
            layer_navigation: Default::default(),
            row_scroll: gpui::ScrollHandle::new(),
            reveal_layer: None,
            rename: None,
            graph: cx.new(|cx| super::graph::Graph::new(state.clone(), cx)),
            marker_editor: cx.new(|cx| super::markers::MarkerEditor::new(state.clone(), cx)),
            state,
            key_menu: None,
            menu_focus_watch: None,
            ruler: Rc::new(Cell::new(None)),
            focus,
            scrubbing: false,
            snapped_to: None,
            drag: None,
            selected_key: None,
        }
    }
    fn seek_position(&self, x: Pixels, cx: &Context<Self>) -> Option<f64> {
        let bounds = self.ruler.get()?;
        let state = self.state.read(cx);
        Some(frame_at(
            f32::from(x),
            f32::from(bounds.left()),
            f32::from(bounds.size.width),
            state.timeline_start,
            state.visible_frames(),
            state.editor.project().composition().duration(),
        ))
    }
    fn moving(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        if self.resizing {
            self.left = f32::from(event.position.x).clamp(380.0, 800.0);
            self.state.update(cx, |s, cx| {
                s.workspace.timeline_left = self.left;
                cx.notify();
            });
            cx.notify();
            return;
        }
        if let Some((_, end)) = &mut self.marquee {
            *end = event.position;
            cx.notify();
            return;
        }
        let Some(position) = self.seek_position(event.position.x, cx) else {
            return;
        };
        self.snapped_to = None;
        let state = self.state.read(cx);
        let comp = state.editor.project().composition();
        let snapping = state.snapping && !event.modifiers.alt;
        let width = self.ruler.get().map_or(0.0, |b| f32::from(b.size.width));
        if let Some((ids, edge, origin, delta)) = &mut self.bar_drag {
            let raw_delta = position - *origin;
            *delta = raw_delta.round() as i64;
            if snapping {
                let anchors: Vec<_> = ids
                    .iter()
                    .filter_map(|id| comp.layer(*id))
                    .flat_map(|l| {
                        let a = i64::from(l.in_frame());
                        let b = i64::from(l.out_frame(comp.duration()));
                        match *edge {
                            -1 => vec![a],
                            1 => vec![b],
                            _ => vec![a, b],
                        }
                    })
                    .collect();
                let min = anchors.iter().min().copied().unwrap_or(0);
                let max = anchors.iter().max().copied().unwrap_or(0);
                let moving = if *edge == 0 {
                    ids.iter().copied().collect()
                } else {
                    BTreeSet::new()
                };
                let targets = super::timeline_snap::targets(
                    comp,
                    Some(state.frame),
                    &moving,
                    &BTreeSet::new(),
                );
                let (d, at) = super::timeline_snap::snap_delta(
                    raw_delta,
                    &anchors,
                    &targets,
                    state.visible_frames(),
                    width,
                    (-min, i64::from(comp.duration()) - max),
                );
                *delta = d;
                self.snapped_to = at;
            }
        }
        if let Some(drag) = &mut self.drag {
            let delta = position - f64::from(drag.from);
            let anchors: Vec<_> = state
                .selected_keys
                .iter()
                .map(|k| i64::from(k.frame))
                .collect();
            let min = anchors.iter().min().copied().unwrap_or(0);
            let max = anchors.iter().max().copied().unwrap_or(0);
            let targets = super::timeline_snap::targets(
                comp,
                Some(state.frame),
                &BTreeSet::new(),
                &state.selected_keys,
            );
            let (delta, at) = if snapping && !anchors.is_empty() {
                super::timeline_snap::snap_delta(
                    delta,
                    &anchors,
                    &targets,
                    state.visible_frames(),
                    width,
                    (-min, i64::from(comp.duration() - 1) - max),
                )
            } else {
                (delta.round() as i64, None)
            };
            drag.to =
                (i64::from(drag.from) + delta).clamp(0, i64::from(comp.duration() - 1)) as u32;
            self.snapped_to = at;
        }
        if self.scrubbing {
            let targets =
                super::timeline_snap::targets(comp, None, &BTreeSet::new(), &BTreeSet::new());
            let (at, snap) = if snapping {
                super::timeline_snap::snap_delta(
                    position,
                    &[0],
                    &targets,
                    state.visible_frames(),
                    width,
                    (0, i64::from(comp.duration() - 1)),
                )
            } else {
                (position.round() as i64, None)
            };
            self.snapped_to = snap;
            self.state
                .update(cx, |s, cx| s.dispatch(&Action::Seek(at as u32), window, cx));
        }
        cx.notify();
    }

    fn up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = false;
        self.resizing = false;
        self.snapped_to = None;
        if let Some((a, b)) = self.marquee.take() {
            let area = Bounds::from_corners(
                point(a.x.min(b.x), a.y.min(b.y)),
                point(a.x.max(b.x), a.y.max(b.y)),
            );
            let intersects = |bounds: Bounds<Pixels>| {
                bounds.right() >= area.left()
                    && bounds.left() <= area.right()
                    && bounds.bottom() >= area.top()
                    && bounds.top() <= area.bottom()
            };
            let keys: Vec<_> = self
                .hit_keys
                .borrow()
                .iter()
                .filter(|(_, b)| intersects(*b))
                .map(|(k, _)| *k)
                .collect();
            let layers: Vec<_> = self
                .hit_layers
                .borrow()
                .iter()
                .filter(|(_, b)| intersects(*b))
                .map(|(id, _)| *id)
                .collect();
            self.state.update(cx, |s, cx| {
                if !self.marquee_additive {
                    s.selected_keys.clear();
                }
                if keys.is_empty() && !self.marquee_additive {
                    s.selected_layers.clear();
                }
                s.selected_layers.extend(keys.iter().map(|k| k.id));
                s.selected_keys.extend(keys);
                s.selected_layers.extend(layers);
                if let Some(id) = s
                    .selected_layers
                    .first()
                    .copied()
                    .or_else(|| s.selected_keys.first().map(|k| k.id))
                {
                    s.editor.select(id);
                } else {
                    s.editor.clear_selection();
                }
                cx.notify();
            });
        }
        let visible = self.visible_layer_ids(cx);
        if self
            .bar_drag
            .as_ref()
            .is_some_and(|(ids, _, _, _)| ids.iter().any(|id| !visible.contains(id)))
        {
            self.bar_drag = None;
            self.blocked_filter_edit(cx);
        }
        if self.drag.is_some() && self.scope_blocked(filter_safety::TargetScope::Keys, cx) {
            self.drag = None;
            self.blocked_filter_edit(cx);
        }
        if let Some((ids, edge, _, delta)) = self.bar_drag.take()
            && delta != 0
        {
            self.state.update(cx, |s, cx| {
                let comp = s.editor.project().composition();
                let commands = ids
                    .iter()
                    .filter_map(|id| comp.layer(*id))
                    .map(|l| {
                        if edge == 0 {
                            Command::ShiftLayer { id: l.id(), delta }
                        } else {
                            Command::SetLayerRange {
                                id: l.id(),
                                start: if edge < 0 {
                                    (l.in_frame() as i64 + delta)
                                        .clamp(0, comp.duration() as i64 - 1)
                                        as u32
                                } else {
                                    l.in_frame()
                                },
                                end: if edge > 0 {
                                    (l.out_frame(comp.duration()) as i64 + delta)
                                        .clamp(1, comp.duration() as i64)
                                        as u32
                                } else {
                                    l.out_frame(comp.duration())
                                },
                            }
                        }
                    })
                    .collect();
                s.dispatch(&Action::Edit(Command::Batch(commands)), window, cx);
            });
        }
        if let Some(drag) = self.drag.take()
            && drag.to != drag.from
        {
            self.state.update(cx, |s, cx| {
                let keys: Vec<_> = s.selected_keys.iter().copied().collect();
                let delta = drag.to as i64 - drag.from as i64;
                s.dispatch(
                    &Action::Edit(Command::MoveKeys {
                        keys: keys.clone(),
                        delta,
                    }),
                    window,
                    cx,
                );
                if s.status.starts_with("Edited") {
                    s.selected_keys = keys
                        .into_iter()
                        .map(|mut k| {
                            k.frame = (k.frame as i64 + delta) as u32;
                            k
                        })
                        .collect();
                    s.graph_key = s
                        .selected_keys
                        .iter()
                        .find(|key| {
                            key.property == s.graph_property && s.editor.selected() == Some(key.id)
                        })
                        .copied();
                }
            });
        }
        cx.notify();
    }
}
fn grid(start: u32, visible: u32, frame: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for tick in 0..=10 {
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(f32::from(bounds.size.width) * tick as f32 / 10.0),
                            bounds.top(),
                        ),
                        size(px(1.0), bounds.size.height),
                    ),
                    rgb(0x2b2b2b),
                ));
            }
            if frame >= start && frame <= start + visible {
                let x = (frame - start) as f32 / visible as f32;
                window.paint_quad(fill(
                    Bounds::new(
                        point(
                            bounds.left() + px(f32::from(bounds.size.width) * x),
                            bounds.top(),
                        ),
                        size(px(1.0), bounds.size.height),
                    ),
                    rgb(ui::BLUE),
                ));
            }
        },
    )
    .absolute()
    .size_full()
}
impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.menu_focus_watch.is_none() {
            self.menu_focus_watch = Some([
                cx.observe_window_bounds(window, |this, _, cx| {
                    if this.colors.cancel_pointer() {
                        cx.notify();
                    }
                }),
                cx.on_blur(&self.focus.clone(), window, |this, _, cx| {
                    this.layer_navigation.reset();
                    this.reveal_layer = None;
                    this.key_menu = None;
                    this.colors.cancel_pointer();
                    cx.notify();
                }),
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        this.key_menu = None;
                        this.colors.cancel_pointer();
                        cx.notify();
                    }
                }),
            ]);
        }
        self.prepare_layer_filters(window, cx);
        self.prepare_layer_rename(window, cx);
        self.reveal_project_usage(window, cx);
        let layer_filters = self.render_layer_filters(cx);
        let key_menu = self.render_key_menu(cx);
        self.left = self.state.read(cx).workspace.timeline_left;
        self.hit_keys.borrow_mut().clear();
        self.hit_layers.borrow_mut().clear();
        let left = self.left;
        let show_modes = left >= 540.0;
        let show_mattes = left >= 750.0;
        let state = self.state.read(cx);
        InputTarget::refresh(&mut self.input_source, state);
        self.animator.observe(state);
        self.colors.observe(state);
        self.colors.observe_pointer_ui(&self.focus, window, cx);
        let input_binding = self
            .input_source
            .as_ref()
            .map(InputTarget::binding)
            .unwrap_or_default();
        let cached_ranges = state.preview_cache.ranges.clone();
        let selected_layers = state.selected_layers.clone();
        let selected_keys = state.selected_keys.clone();
        let comp = state.editor.project().composition().clone();
        self.matte_pickers.retain(|id, _| comp.layer(*id).is_some());
        self.blend_pickers.retain(|id, _| comp.layer(*id).is_some());
        self.fields.retain(|(id, p), _| {
            comp.layer(*id)
                .is_some_and(|l| l.track_value(*p, state.frame).is_some())
        });
        let graph_open = state.graph_open;
        let marker_open = state.selected_marker().is_some();
        let graph_property = state.graph_property;
        let pinned_channels = state
            .graph_channels
            .pinned
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let frame = state.frame;
        let start = state.timeline_start;
        let visible = state.visible_frames();
        let selected = state.editor.selected();
        // Changing layers must not leave Delete targeting a key on the old layer.
        if self
            .selected_key
            .is_some_and(|(id, _, _)| selected != Some(id))
        {
            self.selected_key = None;
            self.drag = None;
        }
        let expanded = state.expanded;
        let filter = state.property_filter;
        let zoom = state.timeline_zoom;
        let work_start = state.work_start;
        let work_end = state.work_end;
        let bounds = self.ruler.clone();
        let parent_drags: BTreeMap<_, _> = comp
            .layers()
            .iter()
            .map(|layer| {
                (
                    layer.id(),
                    ParentDrag::new(state, layer.id(), layer.name().to_string()),
                )
            })
            .collect();
        let reorder_allowed = self.layer_reorder_allowed(cx);
        let mut rows = div().flex().flex_col().min_w_0();
        let mut row_index = 0;
        let reveal_layer = self.reveal_layer.take();
        for (index, layer) in comp.layers().iter().enumerate() {
            if !self.visible_layers.contains(&layer.id()) {
                continue;
            }
            let id = layer.id();
            let selected_row = selected_layers.contains(&id);
            let control_id = |suffix: &str| SharedString::from(format!("layer-{id}-{suffix}"));
            let mut controls = div()
                .flex()
                .items_center()
                .h_full()
                .w(px(left))
                .flex_none()
                .overflow_hidden()
                .child(ui::action_tool(
                    control_id("visible"),
                    if layer.visible() { "eye" } else { "eye-slash" },
                    "Toggle layer visibility",
                    &self.state,
                    Action::Edit(Command::ToggleVisible(id)),
                    false,
                ))
                .child(div().w(px(22.0)).flex_none().when(layer.can_audio(), |d| {
                    d.child(
                        ui::action_tool(
                            control_id("audio"),
                            if layer.audio_enabled() {
                                "volume"
                            } else {
                                "volume-xmark"
                            },
                            "Enable or mute layer audio",
                            &self.state,
                            Action::Edit(Command::SetAudioEnabled {
                                id,
                                enabled: !layer.audio_enabled(),
                            }),
                            false,
                        )
                        .w(px(22.0))
                        .h(px(22.0)),
                    )
                }))
                .child(ui::action_tool(
                    control_id("lock"),
                    if layer.locked() { "lock" } else { "lock-open" },
                    "Toggle layer lock",
                    &self.state,
                    Action::Edit(Command::ToggleLocked(id)),
                    layer.locked(),
                ))
                .children(
                    [
                        (
                            "solo",
                            "target",
                            "Solo: isolate this layer in preview and output",
                            LayerSwitch::Solo,
                            layer.solo(),
                        ),
                        (
                            "shy",
                            "eye-slash",
                            "Shy: hide this row when Hide Shy is enabled",
                            LayerSwitch::Shy,
                            layer.shy(),
                        ),
                        (
                            "guide",
                            "square-dashed",
                            "Guide: preview only, excluded from output and nesting",
                            LayerSwitch::Guide,
                            layer.guide(),
                        ),
                    ]
                    .into_iter()
                    .map(|(key, icon, label, switch, enabled)| {
                        ui::action_tool(
                            control_id(key),
                            icon,
                            label,
                            &self.state,
                            Action::Edit(Command::SetLayerSwitch {
                                id,
                                switch,
                                enabled: !enabled,
                            }),
                            enabled,
                        )
                        .w(px(22.0))
                        .h(px(22.0))
                    }),
                )
                .child(
                    ui::tool(
                        control_id("expand"),
                        if selected_row && expanded {
                            "chevron-down"
                        } else {
                            "chevron-right"
                        },
                        "Reveal transform properties",
                        false,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected_key = None;
                        this.state.update(cx, |state, cx| {
                            if state.editor.selected() == Some(id) {
                                state.dispatch(&Action::ToggleExpanded, window, cx);
                            } else {
                                state.dispatch(&Action::Select(id), window, cx);
                                state.expanded = true;
                            }
                        });
                    })),
                )
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(14.0))
                        .mr_2()
                        .bg(rgb(layer.label_color().unwrap_or_else(|| layer.color()))),
                )
                .child(
                    div()
                        .w(px(20.0))
                        .text_size(px(10.0))
                        .text_color(rgb(ui::MUTED))
                        .child((index + 1).to_string()),
                )
                .child(
                    if let Some(input) = self
                        .rename
                        .as_ref()
                        .filter(|input| input.target.layer() == id)
                    {
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(input.field.clone())
                            .into_any_element()
                    } else {
                        ui::text_button(control_id("name"), layer.name().to_string())
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .overflow_hidden()
                            .tooltip(|_, cx| {
                                cx.new(|_| {
                                    ui::Tip("Double-click or press F2 to rename this layer".into())
                                })
                                .into()
                            })
                            .drag_over::<ParentDrag>({
                                let state = self.state.clone();
                                move |style, drag, _, cx| {
                                    if drag.command(state.read(cx), id).is_some() {
                                        style.bg(rgb(0x164a7b))
                                    } else {
                                        style
                                    }
                                }
                            })
                            .on_drop(cx.listener(move |this, drag: &ParentDrag, window, cx| {
                                let command = drag.command(this.state.read(cx), id);
                                if let Some(command) = command {
                                    this.parent_open = None;
                                    this.state.update(cx, |s, cx| {
                                        s.dispatch(&Action::Edit(command), window, cx)
                                    });
                                }
                                cx.stop_propagation();
                            }))
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, window, cx| {
                                    if event.click_count() == 2 && !event.modifiers().modified() {
                                        this.begin_layer_rename(id, window, cx);
                                        cx.stop_propagation();
                                        return;
                                    }
                                    window.focus(&this.focus);
                                    this.select_visible_layer(
                                        id,
                                        event.modifiers().control,
                                        event.modifiers().shift,
                                        window,
                                        cx,
                                    );
                                },
                            ))
                            .into_any_element()
                    },
                );
            if show_modes {
                let picker = self.blend_pickers.entry(id).or_insert_with(|| {
                    cx.new(|cx| super::blend::BlendPicker::new(self.state.clone(), id, cx))
                });
                controls = controls.child(div().w(px(88.0)).flex_none().child(picker.clone()));
            }
            if show_mattes {
                let (source, mode) = self.matte_pickers.entry(id).or_insert_with(|| {
                    (
                        cx.new(|cx| {
                            super::matte::MattePicker::new(self.state.clone(), id, true, cx)
                        }),
                        cx.new(|cx| {
                            super::matte::MattePicker::new(self.state.clone(), id, false, cx)
                        }),
                    )
                });
                controls = controls
                    .child(div().w(px(120.0)).flex_none().child(source.clone()))
                    .child(div().w(px(80.0)).flex_none().child(mode.clone()));
            }
            let parent_name = layer
                .parent()
                .and_then(|id| comp.layer(id))
                .map_or("None".to_string(), |l| format!("{} · {}", l.id(), l.name()));
            controls = controls.child(
                div()
                    .relative()
                    .w(px(135.0))
                    .flex_none()
                    .child(
                        div().flex().items_center().child(
                            ui::tool(control_id("pick-whip"),"circle-link",
                                "Parent Pick Whip: drag to a layer name; click for the parent menu",false)
                                .w(px(22.0)).h(px(22.0))
                                .when(layer.locked(),|b|b.opacity(0.35))
                                .when(!layer.locked(),|b| b.on_drag(
                                    parent_drags[&id].clone(),
                                    |drag,_,_,cx|cx.new(|_|drag.clone())))
                                .on_click(cx.listener(move|this,_,_,cx| {
                                    this.parent_open=if this.parent_open==Some(id) {None} else {Some(id)};
                                    cx.notify();
                                }))
                        ).child(
                        ui::text_button(control_id("parent"), format!("{parent_name} ▾"))
                            .flex_1().min_w_0()
                            .overflow_hidden()
                            .justify_start()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.parent_open = if this.parent_open == Some(id) {
                                    None
                                } else {
                                    Some(id)
                                };
                                cx.notify();
                            }))),
                    )
                    .when(self.parent_open == Some(id), |s| {
                        let choices = std::iter::once((None, "None".to_string())).chain(
                            comp.layers()
                                .iter()
                                .filter(|l| comp.can_parent(id, Some(l.id())))
                                .map(|l| (Some(l.id()), l.name().to_string())),
                        );
                        let mut menu = div()
                            .id(control_id("parent-menu"))
                            .w(px(190.0))
                            .max_h(px(180.0))
                            .overflow_y_scroll()
                            .bg(rgb(ui::PANEL))
                            .border_1()
                            .border_color(rgb(ui::BLUE));
                        for (parent, label) in choices {
                            menu = menu.child(
                                ui::text_button(
                                    gpui::SharedString::from(format!("parent-{id}-{parent:?}")),
                                    label,
                                )
                                .w_full()
                                .justify_start()
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.parent_open = None;
                                        this.state.update(cx, |s, cx| {
                                            s.dispatch(
                                                &Action::Edit(Command::SetParent {
                                                    id,
                                                    parent,
                                                    frame,
                                                }),
                                                window,
                                                cx,
                                            )
                                        });
                                    },
                                )),
                            );
                        }
                        s.child(
                            gpui::deferred(menu.absolute().top(px(22.0)).left_0().occlude())
                                .with_priority(2),
                        )
                    }),
            );
            controls = controls.children(
                [
                    (-1, "up", "arrow-up", "Move layer up"),
                    (1, "down", "arrow-down", "Move layer down"),
                ]
                .into_iter()
                .map(|(direction, suffix, icon, label)| {
                    ui::tool(
                        control_id(suffix),
                        icon,
                        if reorder_allowed {
                            label
                        } else {
                            "Clear layer filters and Hide Shy before reordering"
                        },
                        false,
                    )
                    .when(!reorder_allowed, |d| d.opacity(0.35))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.move_visible_layer(id, direction, window, cx)
                    }))
                }),
            );
            let bar_offset = self
                .bar_drag
                .as_ref()
                .filter(|(ids, _, _, _)| ids.contains(&id));
            let bar_start = bar_offset
                .map_or(layer.in_frame() as i64, |(_, edge, _, d)| {
                    layer.in_frame() as i64 + if *edge <= 0 { *d } else { 0 }
                })
                .max(0) as u32;
            let bar_end = bar_offset
                .map_or(
                    layer.out_frame(comp.duration()) as i64,
                    |(_, edge, _, d)| {
                        layer.out_frame(comp.duration()) as i64 + if *edge >= 0 { *d } else { 0 }
                    },
                )
                .max(0) as u32;
            let bar_left =
                (bar_start.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
            let right = (bar_end.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
            let bar_visible =
                layer.out_frame(comp.duration()) > start && layer.in_frame() < start + visible;
            let layer_hits = self.hit_layers.clone();
            let bar_bounds = Rc::new(Cell::new(None));
            let bar_measure = bar_bounds.clone();
            let waveform = if bar_visible && layer.content().audio().is_some() {
                let shift = bar_offset
                    .filter(|(_, edge, _, _)| *edge == 0)
                    .map_or(0, |(_, _, _, delta)| *delta);
                let a = i64::from(bar_start.max(start)) - shift;
                let b = i64::from(bar_end.min(start + visible)) - shift;
                Some(self.waveforms.update(cx, |waves, cx| {
                    waves.row(layer, comp.fps(), a as f64, b as f64, cx)
                }))
            } else {
                None
            };
            let time_area = div()
                .relative()
                .flex_1()
                .h_full()
                .overflow_hidden()
                .child(grid(start, visible, frame))
                .child(
                    canvas(
                        move |b, _, _| layer_hits.borrow_mut().push((id, b)),
                        |_, _, _, _| (),
                    )
                    .absolute()
                    .size_full(),
                )
                .when(bar_visible, |area| {
                    area.child(
                        div()
                            .id(control_id("bar"))
                            .absolute()
                            .left(relative(bar_left))
                            .w(relative((right - bar_left).max(0.0)))
                            .top(px(3.0))
                            .h(px(17.0))
                            .bg(rgb(layer.label_color().unwrap_or_else(|| layer.color())))
                            .opacity(
                                if if matches!(
                                    layer.content(),
                                    libre_effects_core::Content::Audio { .. }
                                ) {
                                    layer.audio_enabled()
                                } else {
                                    layer.visible()
                                } {
                                    0.85
                                } else {
                                    0.25
                                },
                            )
                            .border_1()
                            .border_color(rgb(if selected_row { 0xddd2ff } else { 0x777777 }))
                            .cursor_grab()
                            .children(waveform)
                            .child(
                                canvas(move |b, _, _| bar_measure.set(Some(b)), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        window.focus(&this.focus);
                                        cx.stop_propagation();
                                        let Some(at) = this.seek_position(event.position.x, cx)
                                        else {
                                            return;
                                        };
                                        let edge =
                                            bar_bounds.get().map_or(0, |b: Bounds<Pixels>| {
                                                if event.position.x - b.left() < px(7.0) {
                                                    -1
                                                } else if b.right() - event.position.x < px(7.0) {
                                                    1
                                                } else {
                                                    0
                                                }
                                            });
                                        if !this.state.read(cx).selected_layers.contains(&id) {
                                            this.select_visible_layer(
                                                id,
                                                event.modifiers.control,
                                                event.modifiers.shift,
                                                window,
                                                cx,
                                            );
                                        }
                                        if this
                                            .scope_blocked(filter_safety::TargetScope::Layers, cx)
                                        {
                                            this.blocked_filter_edit(cx);
                                            return;
                                        }
                                        this.state.update(cx, |s, _| s.selected_keys.clear());
                                        this.bar_drag = Some((
                                            this.state
                                                .read(cx)
                                                .selected_layers
                                                .iter()
                                                .copied()
                                                .collect(),
                                            edge,
                                            at,
                                            0,
                                        ));
                                        cx.notify();
                                    },
                                ),
                            ),
                    )
                });
            let time_area = time_area.children(
                layer
                    .markers()
                    .iter()
                    .filter(|m| m.frame() < start.saturating_add(visible) && m.end() >= start)
                    .map(|m| {
                        super::markers::marker_item(
                            m,
                            libre_effects_core::MarkerTarget::Layer(id),
                            start,
                            visible,
                            &self.state,
                        )
                    }),
            );
            if reveal_layer == Some(id) {
                // Reveal the header after the new selection has expanded its
                // lanes, using the current layout rather than stale bounds.
                self.row_scroll.scroll_to_item(row_index);
            }
            row_index += 1;
            rows = rows.child(
                div()
                    .flex()
                    .h(px(23.0))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(0x151515))
                    .bg(rgb(if selected_row { 0x343434 } else { ui::BG }))
                    .child(controls)
                    .when(!graph_open, |s| s.child(time_area)),
            );
            if selected_row && expanded {
                row_index += 1;
                rows = rows.child(
                    self.colors
                        .render_rows(
                            &self.state,
                            layer,
                            left,
                            start,
                            visible,
                            frame,
                            filter,
                            graph_open,
                            self.input_source.clone(),
                            self.focus.clone(),
                            window,
                            cx,
                        )
                        .flex_none(),
                );
                let is_audio = matches!(layer.content(), libre_effects_core::Content::Audio { .. });
                if !is_audio
                    && (filter != Some(PropertyFilter::Animated)
                        || layer
                            .spatial_position()
                            .is_some_and(|position| !position.keys.is_empty())
                        || layer
                            .planar_position()
                            .is_some_and(|position| !position.keys.is_empty())
                        || (layer.has_opacity_timing() && layer.opacity_key_count() != 0)
                        || Property::ALL.into_iter().any(|p| {
                            layer
                                .property(p)
                                .is_some_and(|track| !track.keys().is_empty())
                        }))
                {
                    row_index += 1;
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(24.0))
                            .flex_none()
                            .child(
                                div()
                                    .w(px(left))
                                    .flex_none()
                                    .pl(px(128.0))
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .text_color(rgb(ui::MUTED))
                                    .child(ui::icon("chevron-down"))
                                    .child("Transform"),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .h_full()
                                    .child(grid(start, visible, frame)),
                            ),
                    );
                }
                if !is_audio
                    && joined_position_visible(layer, filter)
                    && let Some((value, keys)) = super::inspector::joined_position_summary(
                        layer,
                        frame,
                        comp.fps().seconds(1),
                    )
                {
                    row_index += 1;
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(42.0))
                            .flex_none()
                            .child(
                                div()
                                    .w(px(left))
                                    .flex_none()
                                    .pl(px(128.0))
                                    .flex()
                                    .flex_col()
                                    .text_size(px(11.0))
                                    .text_color(rgb(ui::MUTED))
                                    .child(format!(
                                        "Position {} · joined · {keys} keys · read-only",
                                        if layer.is_three_d() { "XYZ" } else { "XY" }
                                    ))
                                    .child(value),
                            )
                            .when(!graph_open, |row| {
                                row.child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .h_full()
                                        .child(grid(start, visible, frame)),
                                )
                            }),
                    );
                }
                if !is_audio
                    && native_opacity_visible(layer, filter)
                    && let Some((value, keys)) = super::inspector::native_opacity_summary(
                        layer,
                        frame,
                        comp.fps().seconds(1),
                    )
                {
                    row_index += 1;
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(42.0))
                            .flex_none()
                            .child(
                                div()
                                    .w(px(left))
                                    .flex_none()
                                    .pl(px(128.0))
                                    .flex()
                                    .flex_col()
                                    .text_size(px(11.0))
                                    .text_color(rgb(ui::MUTED))
                                    .child("Opacity · native timing · read-only")
                                    .child(format!("{value} · {keys} keys")),
                            )
                            .when(!graph_open, |row| {
                                row.child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .h_full()
                                        .child(grid(start, visible, frame)),
                                )
                            }),
                    );
                }
                let mut groups: Vec<(String, Vec<PropertyPath>)> = [
                    ("Anchor Point", vec![Property::AnchorX, Property::AnchorY]),
                    ("Position", vec![Property::PositionX, Property::PositionY]),
                    ("Scale", vec![Property::ScaleX, Property::ScaleY]),
                    ("Rotation", vec![Property::Rotation]),
                    ("Opacity", vec![Property::Opacity]),
                ]
                .into_iter()
                .map(|(label, properties)| {
                    (
                        label.to_string(),
                        properties.into_iter().map(Into::into).collect(),
                    )
                })
                .collect();
                if is_audio {
                    groups.clear();
                }
                if layer.can_audio() {
                    for p in libre_effects_core::AudioParam::ALL {
                        groups.push((p.label().into(), vec![PropertyPath::Audio(p)]));
                    }
                }
                if layer.time_remap().is_some() {
                    groups.push(("Time Remap".into(), vec![PropertyPath::TimeRemap]));
                }
                groups.extend(text_groups(layer));
                let shape_path = PropertyPath::Path(libre_effects_core::PathTarget::Shape);
                for path in layer
                    .track_paths()
                    .into_iter()
                    .filter(|p| matches!(p, PropertyPath::Shape(parameter) if libre_effects_core::ShapePaint::from_parameter(*parameter).is_none()))
                {
                    groups.push((layer.track_label(path).unwrap(), vec![path]));
                }
                for (paint, label) in [
                    (libre_effects_core::ShapePaint::Fill, "Fill Color"),
                    (libre_effects_core::ShapePaint::Stroke, "Stroke Color"),
                ] {
                    let channels: Vec<_> = paint
                        .channels()
                        .into_iter()
                        .map(PropertyPath::Shape)
                        .filter(|p| layer.track(*p).is_some())
                        .collect();
                    if !channels.is_empty() {
                        groups.push((label.into(), channels));
                    }
                }
                if layer.track(shape_path).is_some() {
                    groups.push(("Shape Path".into(), vec![shape_path]));
                }
                for path in layer.track_paths().into_iter().filter(|p| {
                    matches!(
                        p,
                        PropertyPath::Contents { .. }
                            | PropertyPath::Path(libre_effects_core::PathTarget::Contents(_))
                    )
                }) {
                    let label = match path {
                        PropertyPath::Contents { parameter, .. } => parameter.label(),
                        _ => "Path".into(),
                    };
                    groups.push((label, vec![path]));
                }
                for mask in layer.path_masks() {
                    let path = PropertyPath::Path(libre_effects_core::PathTarget::Mask(mask.id));
                    groups.push((layer.track_label(path).unwrap(), vec![path]));
                    for parameter in libre_effects_core::MaskParam::ALL {
                        let path = PropertyPath::Mask {
                            mask: mask.id,
                            parameter,
                        };
                        groups.push((layer.track_label(path).unwrap(), vec![path]));
                    }
                }
                for effect in layer.effect_stack() {
                    for param in effect.kind().parameters() {
                        groups.push((
                            param.label.to_string(),
                            vec![PropertyPath::Effect {
                                effect: effect.id(),
                                parameter: param.parameter,
                            }],
                        ));
                    }
                }
                let mut last_section = None;
                for (label, properties) in groups {
                    if !group_visible(layer, &properties, filter) {
                        continue;
                    }
                    let section = match properties[0] {
                        PropertyPath::Contents { item, .. }
                        | PropertyPath::Path(libre_effects_core::PathTarget::Contents(item)) => {
                            let name = match layer.content() {
                                libre_effects_core::Content::ShapeContents(c) => {
                                    c.node(item).map(|n| n.name.clone()).unwrap_or_default()
                                }
                                _ => String::new(),
                            };
                            Some(((3, item), format!("Contents · {name}")))
                        }
                        PropertyPath::Text(parameter) if parameter.is_animator() => {
                            Some(((4, 1), animator_section_label(layer)))
                        }
                        PropertyPath::TextSelector { selector, .. } => {
                            let range = layer
                                .text_range_selectors()
                                .iter()
                                .find(|range| range.id == selector)
                                .unwrap();
                            Some((
                                (5, selector),
                                format!(
                                    "Text Animator · Selector #{selector} · {} · {} · {}",
                                    range.mode.label(),
                                    range.selector.units.label(),
                                    range.selector.shape.label()
                                ),
                            ))
                        }
                        PropertyPath::TextAnimator { animator, .. } => {
                            let item = layer
                                .text_animators()
                                .iter()
                                .find(|item| item.id == animator)
                                .unwrap();
                            Some((
                                (6, animator),
                                format!(
                                    "Text Animator #{animator} · {} · {}",
                                    item.selector.units.label(),
                                    item.selector.shape.label()
                                ),
                            ))
                        }
                        PropertyPath::Text(_) | PropertyPath::SourceText => {
                            Some(((4, 0), "Text".to_string()))
                        }
                        PropertyPath::Shape(_) => Some(((0, 1), "Contents · Shape".to_string())),
                        PropertyPath::Path(libre_effects_core::PathTarget::Shape) => {
                            Some(((0, 0), "Contents · Path".to_string()))
                        }
                        PropertyPath::Path(libre_effects_core::PathTarget::Mask(mask))
                        | PropertyPath::Mask { mask, .. } => Some((
                            (1, mask),
                            format!(
                                "Masks · Mask {}",
                                layer
                                    .path_masks()
                                    .iter()
                                    .position(|m| m.id == mask)
                                    .unwrap()
                                    + 1
                            ),
                        )),
                        PropertyPath::Effect { effect, .. } => Some((
                            (2, effect),
                            format!(
                                "Effects · {}",
                                layer
                                    .effect_stack()
                                    .iter()
                                    .find(|e| e.id() == effect)
                                    .map(|e| e.name())
                                    .unwrap_or("Effect")
                            ),
                        )),
                        _ => None,
                    };
                    if let Some((section_id, name)) = section {
                        if last_section != Some(section_id) {
                            last_section = Some(section_id);
                            row_index += 1;
                            rows = rows.child(
                                div()
                                    .flex()
                                    .h(px(24.0))
                                    .flex_none()
                                    .child(
                                        div()
                                            .w(px(left))
                                            .flex_none()
                                            .pl(px(128.0))
                                            .text_color(rgb(ui::MUTED))
                                            .overflow_hidden()
                                            .child(name),
                                    )
                                    .when(!graph_open, |s| {
                                        s.child(
                                            div()
                                                .relative()
                                                .flex_1()
                                                .h_full()
                                                .child(grid(start, visible, frame)),
                                        )
                                    }),
                            );
                        }
                    }
                    let prop_id = |suffix: &str| {
                        SharedString::from(format!("prop-{id}-{:?}-{suffix}", properties[0]))
                    };
                    let animated = group_animated(layer, &properties);
                    let watch = group_watch(layer, &properties, frame);
                    let channel = properties[0];
                    let channel_state = self.state.clone();
                    let mut controls = div()
                        .flex()
                        .items_center()
                        .w(px(left))
                        .flex_none()
                        .pl(px(128.0))
                        .child(if channel == PropertyPath::SourceText {
                            source_text_control(
                                &self.state,
                                layer,
                                frame,
                                SourceTextControl::Animation,
                                self.input_source.clone(),
                                "timeline",
                            )
                            .into_any_element()
                        } else if let PropertyPath::Text(parameter) = channel
                            && parameter.is_animator()
                        {
                            self.animator
                                .control(
                                    &self.state,
                                    layer,
                                    parameter,
                                    frame,
                                    true,
                                    self.input_source.clone(),
                                )
                                .into_any_element()
                        } else if let PropertyPath::TextAnimator {
                            animator,
                            parameter,
                        } = channel
                        {
                            self.animator
                                .animator_control(
                                    &self.state,
                                    layer,
                                    animator,
                                    parameter,
                                    frame,
                                    true,
                                    self.input_source.clone(),
                                )
                                .into_any_element()
                        } else if let PropertyPath::TextSelector {
                            selector,
                            parameter,
                        } = channel
                        {
                            self.animator
                                .selector_control(
                                    &self.state,
                                    layer,
                                    selector,
                                    parameter,
                                    frame,
                                    true,
                                    self.input_source.clone(),
                                )
                                .into_any_element()
                        } else {
                            ui::action_tool(
                                prop_id("watch"),
                                "stopwatch",
                                "Toggle animation",
                                &self.state,
                                Action::Edit(watch),
                                animated,
                            )
                            .into_any_element()
                        })
                        .child(
                            ui::text_button(prop_id("label"), label.clone())
                                .min_w_0()
                                .overflow_hidden()
                                .flex_1()
                                .justify_start()
                                .text_size(px(11.0))
                                .when(graph_open && properties.contains(&graph_property), |s| {
                                    s.text_color(rgb(ui::BLUE))
                                })
                                .on_click(move |_, _, cx| {
                                    channel_state.update(cx, |s, cx| {
                                        s.graph_activate_property(
                                            GraphChannel {
                                                id,
                                                property: channel,
                                            },
                                            true,
                                        );
                                        if matches!(
                                            channel,
                                            PropertyPath::Path(_) | PropertyPath::SourceText
                                        ) {
                                            s.graph_open = false;
                                            if matches!(channel, PropertyPath::Path(_)) {
                                                s.tool = crate::editor::Tool::Pen;
                                            }
                                        }
                                        s.graph_key = None;
                                        cx.notify();
                                    });
                                }),
                        );
                    for property in properties.iter().copied() {
                        if property == PropertyPath::SourceText {
                            for kind in [
                                SourceTextControl::Previous,
                                SourceTextControl::Edit,
                                SourceTextControl::Next,
                            ] {
                                controls = controls.child(source_text_control(
                                    &self.state,
                                    layer,
                                    frame,
                                    kind,
                                    self.input_source.clone(),
                                    "timeline",
                                ));
                            }
                            continue;
                        }
                        if matches!(property, PropertyPath::Path(_)) {
                            controls = controls.child(ui::action_tool(
                                prop_id("edit-path"),
                                "pen",
                                "Edit path vertices at current time",
                                &self.state,
                                Action::GraphProperty(id, property),
                                false,
                            ));
                            continue;
                        }
                        let animator_channel = matches!(
                            property,
                            PropertyPath::TextSelector { .. } | PropertyPath::TextAnimator { .. }
                        ) || matches!(property, PropertyPath::Text(parameter) if parameter.is_animator());
                        let input = if !animator_channel {
                            let (input, input_target) = self
                                .fields
                                .entry((id, property))
                                .or_insert_with(|| {
                                    let edit = self.state.clone();
                                    let target: Rc<RefCell<Option<InputTarget>>> =
                                        Default::default();
                                    let captured = target.clone();
                                    let input = cx.new(|cx| {
                                        TextField::new(cx, move |text, window, cx| {
                                            edit.update(cx, |s, cx| {
                                                if !captured
                                                    .borrow()
                                                    .as_ref()
                                                    .is_some_and(|t| t.current(s))
                                                    || s.editor.selected() != Some(id)
                                                {
                                                    return;
                                                }
                                                if let Ok(value) = parse_scalar(text) {
                                                    if let PropertyPath::Text(parameter) = property
                                                    {
                                                        s.finish_text(true, cx);
                                                        if !captured
                                                            .borrow()
                                                            .as_ref()
                                                            .is_some_and(|t| t.same_context(s))
                                                        {
                                                            return;
                                                        }
                                                        let command = s
                                                            .editor
                                                            .selected_layer()
                                                            .unwrap()
                                                            .text_value_command(
                                                                parameter, value, s.frame,
                                                            );
                                                        match command {
                                                            Ok(Some(command)) => s.dispatch(
                                                                &Action::Edit(command),
                                                                window,
                                                                cx,
                                                            ),
                                                            Ok(None) => {}
                                                            Err(error) => {
                                                                s.status = error;
                                                                cx.notify();
                                                            }
                                                        }
                                                        return;
                                                    }
                                                    s.dispatch(
                                                        &Action::Edit(Command::EditTrack {
                                                            id,
                                                            property,
                                                            edit: TrackEdit::Value {
                                                                frame: s.frame,
                                                                value,
                                                            },
                                                        }),
                                                        window,
                                                        cx,
                                                    );
                                                } else {
                                                    s.status = "Enter a finite number".into();
                                                    cx.notify();
                                                }
                                            })
                                        })
                                        .numeric()
                                    });
                                    (input, target)
                                })
                                .clone();
                            *input_target.borrow_mut() = self.input_source.clone();
                            input.update(cx, |field, _| {
                                field.sync(
                                    input_binding.clone(),
                                    {
                                        let value = layer
                                            .track_value(property, frame)
                                            .expect("visible property");
                                        if matches!(property, PropertyPath::Text(_)) {
                                            value.to_string()
                                        } else if property == PropertyPath::TimeRemap {
                                            format!("{value:.12}")
                                        } else {
                                            format!("{value:.2}")
                                        }
                                    },
                                    window,
                                )
                            });
                            Some(input)
                        } else {
                            None
                        };
                        controls = controls
                            .child(
                                ui::text_button(
                                    SharedString::from(format!("channel-{id}-{property:?}")),
                                    if let PropertyPath::Text(p) | PropertyPath::TextAnimator { parameter: p, .. } = property {
                                        text_channel_label(p)
                                    } else if matches!(property, PropertyPath::TextSelector { .. }) {
                                        "%"
                                    } else if let PropertyPath::Shape(p) = property {
                                        libre_effects_core::ShapePaint::component_label(p)
                                            .unwrap_or("")
                                    } else if properties.len() == 2 {
                                        if matches!(
                                            property,
                                            PropertyPath::Transform(
                                                Property::PositionX
                                                    | Property::AnchorX
                                                    | Property::ScaleX
                                            )
                                        ) {
                                            "X"
                                        } else {
                                            "Y"
                                        }
                                    } else {
                                        if property == PropertyPath::TimeRemap {
                                            "s"
                                        } else if property == Property::Opacity.into() {
                                            "%"
                                        } else if property == Property::Rotation.into() {
                                            "°"
                                        } else {
                                            ""
                                        }
                                    },
                                )
                                .w(px(19.0))
                                .when(graph_open && graph_property == property, |s| {
                                    s.text_color(rgb(ui::BLUE))
                                })
                                .on_click({
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |s, cx| {
                                            s.graph_activate_property(
                                                GraphChannel { id, property },
                                                true,
                                            );
                                            s.graph_key = None;
                                            cx.notify();
                                        })
                                    }
                                }),
                            )
                            .child(
                                ui::text_button(
                                    SharedString::from(format!("pin-channel-{id}-{property:?}")),
                                    if pinned_channels.contains(&GraphChannel { id, property }) {
                                        "◆"
                                    } else {
                                        "+"
                                    },
                                )
                                .w(px(18.0))
                                .tooltip(|_, cx| {
                                    cx.new(|_| ui::Tip("Pin / unpin this channel in Graph".into()))
                                        .into()
                                })
                                .when(
                                    pinned_channels.contains(&GraphChannel { id, property }),
                                    |s| s.text_color(rgb(ui::BLUE)),
                                )
                                .on_click({
                                    let state = self.state.clone();
                                    move |_, _, cx| {
                                        state.update(cx, |s, cx| {
                                            let channel = GraphChannel { id, property };
                                            if s.graph_channels.is_pinned(channel) {
                                                s.graph_unpin_channel(channel);
                                            } else if let Err(error) = s.graph_pin_channel(channel)
                                            {
                                                s.status = error;
                                            }
                                            cx.notify();
                                        })
                                    }
                                }),
                            )
                            .child(
                                div()
                                    .w(px(60.0))
                                    .when(animator_channel, |s| {
                                        let value = layer.track_value(property, frame).expect("visible property").to_string();
                                        s.child(div().id(SharedString::from(format!("animator-sample-{id}-{property:?}")))
                                            .text_color(rgb(ui::MUTED)).overflow_hidden().child(value.clone())
                                            .tooltip(move |_, cx| cx.new(|_| ui::Tip(format!("{value} · edit in Properties → Text Animator; press Enter before using Timeline buttons").into())).into()))
                                    })
                                    .when(!animator_channel && !layer.locked(), |s| s.child(input.unwrap()))
                                    .when(!animator_channel && layer.locked(), |s| {
                                        s.child(format!(
                                            "{:.1}",
                                            layer
                                                .track_value(property, frame)
                                                .expect("visible property")
                                        ))
                                    }),
                            );
                    }
                    let at_frame: Vec<_> = properties
                        .iter()
                        .filter(|p| {
                            layer
                                .track(**p)
                                .is_some_and(|t| t.keys().contains_key(&frame))
                        })
                        .copied()
                        .collect();
                    let toggle = if at_frame.is_empty() {
                        properties.clone()
                    } else {
                        at_frame.clone()
                    };
                    controls = controls.child(if channel == PropertyPath::SourceText {
                        source_text_control(
                            &self.state,
                            layer,
                            frame,
                            SourceTextControl::Key,
                            self.input_source.clone(),
                            "timeline",
                        )
                        .into_any_element()
                    } else if let PropertyPath::Text(parameter) = channel
                        && parameter.is_animator()
                    {
                        self.animator
                            .control(
                                &self.state,
                                layer,
                                parameter,
                                frame,
                                false,
                                self.input_source.clone(),
                            )
                            .into_any_element()
                    } else if let PropertyPath::TextAnimator {
                        animator,
                        parameter,
                    } = channel
                    {
                        self.animator
                            .animator_control(
                                &self.state,
                                layer,
                                animator,
                                parameter,
                                frame,
                                false,
                                self.input_source.clone(),
                            )
                            .into_any_element()
                    } else if let PropertyPath::TextSelector {
                        selector,
                        parameter,
                    } = channel
                    {
                        self.animator
                            .selector_control(
                                &self.state,
                                layer,
                                selector,
                                parameter,
                                frame,
                                false,
                                self.input_source.clone(),
                            )
                            .into_any_element()
                    } else {
                        ui::action_tool(
                            prop_id("key"),
                            "diamond",
                            "Add / remove keys",
                            &self.state,
                            Action::Edit(Command::Batch(
                                toggle
                                    .into_iter()
                                    .map(|property| Command::EditTrack {
                                        id,
                                        property,
                                        edit: TrackEdit::ToggleKey { frame },
                                    })
                                    .collect(),
                            )),
                            !at_frame.is_empty(),
                        )
                        .into_any_element()
                    });
                    let mut keys = div()
                        .relative()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(grid(start, visible, frame));
                    let frames: BTreeSet<_> = properties
                        .iter()
                        .flat_map(|p| {
                            layer
                                .track(*p)
                                .into_iter()
                                .flat_map(|t| t.keys().keys().copied())
                        })
                        .collect();
                    for key_frame in frames {
                        let key_refs: Vec<_> = properties
                            .iter()
                            .filter(|p| {
                                layer
                                    .track(**p)
                                    .is_some_and(|t| t.keys().contains_key(&key_frame))
                            })
                            .map(|p| KeyRef {
                                id,
                                property: *p,
                                frame: key_frame,
                            })
                            .collect();
                        let active = key_refs.iter().any(|k| selected_keys.contains(k));
                        let offset = self
                            .drag
                            .as_ref()
                            .filter(|_| active)
                            .map_or(0, |d| d.to as i64 - d.from as i64);
                        let display = (key_frame as i64 + offset).max(0) as u32;
                        if display < start || display > start + visible {
                            continue;
                        }
                        let hits = self.hit_keys.clone();
                        let measured = key_refs.clone();
                        let menu_keys = key_refs.clone();
                        let property = key_refs[0].property;
                        let (glyph, description) =
                            super::key_glyph::Glyph::row(layer, &properties, key_frame)
                                .expect("visible timeline key");
                        keys = keys.child(
                            div()
                                .id(SharedString::from(format!("key-{id}-{label}-{key_frame}")))
                                .absolute()
                                .left(relative((display - start) as f32 / visible as f32))
                                .ml(px(-6.0))
                                .top(px(4.0))
                                .size(px(13.0))
                                .cursor_pointer()
                                .tooltip(move |_, cx| {
                                    cx.new(|_| ui::Tip(description.clone().into())).into()
                                })
                                .child(glyph.element(active))
                                .child(
                                    canvas(
                                        move |b, _, _| {
                                            for k in &measured {
                                                hits.borrow_mut().push((*k, b));
                                            }
                                        },
                                        |_, _, _, _| (),
                                    )
                                    .absolute()
                                    .size_full(),
                                )
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            this.open_key_menu(
                                                Some(&menu_keys),
                                                event.position,
                                                window,
                                                cx,
                                            );
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        },
                                    ),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(
                                        move |this, event: &gpui::MouseDownEvent, window, cx| {
                                            window.focus(&this.focus);
                                            cx.stop_propagation();
                                            this.scrubbing = false;
                                            this.colors.clear_selection();
                                            this.state.update(cx, |s, cx| {
                                                if event.modifiers.control || event.modifiers.shift
                                                {
                                                    for k in &key_refs {
                                                        if !s.selected_keys.remove(k) {
                                                            s.selected_keys.insert(*k);
                                                        }
                                                    }
                                                } else if !key_refs
                                                    .iter()
                                                    .any(|k| s.selected_keys.contains(k))
                                                {
                                                    s.selected_keys =
                                                        key_refs.iter().copied().collect();
                                                }
                                                s.graph_activate_property(
                                                    GraphChannel { id, property },
                                                    true,
                                                );
                                                s.graph_key = (property
                                                    != PropertyPath::SourceText)
                                                    .then_some(KeyRef {
                                                        id,
                                                        property,
                                                        frame: key_frame,
                                                    });
                                                if property == PropertyPath::SourceText {
                                                    s.graph_open = false;
                                                }
                                                s.dispatch(&Action::Seek(key_frame), window, cx);
                                            });
                                            if this
                                                .scope_blocked(filter_safety::TargetScope::Keys, cx)
                                            {
                                                this.blocked_filter_edit(cx);
                                                return;
                                            }
                                            this.drag = Some(KeyDrag {
                                                from: key_frame,
                                                to: key_frame,
                                            });
                                            cx.notify();
                                        },
                                    ),
                                ),
                        );
                    }
                    row_index += 1;
                    rows = rows.child(
                        div()
                            .flex()
                            .h(px(25.0))
                            .flex_none()
                            .child(controls)
                            .when(!graph_open, |s| s.child(keys)),
                    );
                }
            }
        }
        if self.visible_layers.is_empty() {
            rows = rows.child(
                div()
                    .h(px(90.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(ui::MUTED))
                    .child(if comp.layers().is_empty() {
                        "No layers. Create a solid with Ctrl+Y."
                    } else {
                        "No matching layers. Clear the layer filters or turn off Hide Shy."
                    }),
            );
        }
        let work_left = (work_start.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
        let work_right = (work_end.saturating_sub(start) as f32 / visible as f32).clamp(0.0, 1.0);
        let pointer_owner = cx.entity();
        div()
            .id("timeline")
            .relative()
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .bg(rgb(ui::BG))
            .on_modifiers_changed(cx.listener(
                |this, event: &gpui::ModifiersChangedEvent, _, cx| {
                    if this.colors.pointer_modifiers_changed(event.modifiers) {
                        cx.notify();
                    }
                },
            ))
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        // Installed before the first press, independently of hover and
                        // before any sibling can consume movement or outside release.
                        let down = pointer_owner.clone();
                        window.on_mouse_event(move |_: &gpui::MouseDownEvent, phase, _, cx| {
                            if phase.capture() {
                                down.update(cx, |this, cx| {
                                    if this.colors.cancel_pointer() {
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let moving = pointer_owner.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase.capture() {
                                moving.update(cx, |this, cx| {
                                    this.colors.pointer_move(
                                        event,
                                        &this.state,
                                        &this.focus,
                                        window,
                                        cx,
                                    )
                                });
                            }
                        });
                        let ending = pointer_owner.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase.capture() {
                                ending.update(cx, |this, cx| {
                                    this.colors.pointer_up(
                                        event,
                                        &this.state,
                                        &this.focus,
                                        window,
                                        cx,
                                    )
                                });
                            }
                        });
                        let scrolling = pointer_owner.clone();
                        window.on_mouse_event(move |_: &gpui::ScrollWheelEvent, phase, _, cx| {
                            if phase.capture() {
                                scrolling.update(cx, |this, cx| {
                                    if this.colors.cancel_pointer() {
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_move(cx.listener(Self::moving))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::up))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.layer_filter_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if this.key_menu_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if !this.focus.is_focused(window) {
                    return;
                }
                if this.layer_rename_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if this.layer_navigation_key(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if filter_safety::shortcut_scope(event)
                    .is_some_and(|scope| this.scope_blocked(scope, cx))
                {
                    this.blocked_filter_edit(cx);
                    cx.stop_propagation();
                    return;
                }
                if this.colors.key_down(event, &this.state, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if let Some((incoming, outgoing)) = super::key_easing::shortcut(event)
                    && this.focus.is_focused(window)
                    && this.drag.is_none()
                    && this.bar_drag.is_none()
                    && this.marquee.is_none()
                {
                    this.state.update(cx, |s, cx| {
                        let keys = s.selected_keys.iter().copied().collect::<Vec<_>>();
                        match super::key_easing::selected(
                            s.editor.project(),
                            &keys,
                            incoming,
                            outgoing,
                        ) {
                            Ok(Some(command)) => s.dispatch(&Action::Edit(command), window, cx),
                            Ok(None) => {}
                            Err(error) => {
                                s.status = error;
                                cx.notify();
                            }
                        }
                    });
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "escape" {
                    this.drag = None;
                    this.bar_drag = None;
                    this.marquee = None;
                    this.resizing = false;
                    this.scrubbing = false;
                    this.parent_open = None;
                    cx.notify();
                }
                if event.keystroke.key == "a"
                    && event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.shift
                    && !event.keystroke.modifiers.alt
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.function
                {
                    this.colors.clear_selection();
                    let keys = this.hit_keys.borrow().iter().map(|(k, _)| *k).collect();
                    this.state.update(cx, |s, cx| {
                        s.selected_keys = keys;
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
                if event.keystroke.key == "delete" {
                    this.state
                        .update(cx, |s, cx| s.dispatch(&Action::DeleteSelection, window, cx));
                    cx.stop_propagation();
                }
            }))
            .child(ui::panel_header(if self.state.read(cx).welcome() {
                "Timeline".to_string()
            } else {
                comp.name().to_string()
            }))
            .child(
                div()
                    .flex()
                    .h(px(32.0))
                    .flex_none()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .child(
                        div()
                            .w(px(105.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(ui::BLUE))
                                    .child(comp.timecode(frame)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(ui::MUTED))
                                    .child(format!("{frame:05}  ({} fps)", comp.fps().label())),
                            ),
                    )
                    .child(
                        ui::tool(
                            "all-properties",
                            "chevron-down",
                            "Reveal all properties",
                            false,
                        )
                        .on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |state, cx| {
                                    state.dispatch(&Action::Filter(None), window, cx)
                                })
                            }
                        }),
                    )
                    .child(
                        ui::tool(
                            "animated-properties",
                            "stopwatch",
                            "Reveal animated properties (U)",
                            false,
                        )
                        .on_click({
                            let state = self.state.clone();
                            move |_, window, cx| {
                                state.update(cx, |state, cx| {
                                    state.dispatch(
                                        &Action::Filter(Some(PropertyFilter::Animated)),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }),
                    )
                    .child(ui::action_tool(
                        "open-graph",
                        "chart-line",
                        "Graph Editor (Shift+F3)",
                        &self.state,
                        Action::ToggleGraph,
                        graph_open,
                    ))
                    .child(
                        ui::tool(
                            "hide-shy-layers",
                            "eye-slash",
                            "Hide shy layers",
                            comp.hide_shy(),
                        )
                        .when(comp.hide_shy(), |s| {
                            s.bg(rgb(0x164a7b)).text_color(rgb(ui::BLUE))
                        })
                        .on_click({
                            let state = self.state.clone();
                            let hidden = comp.hide_shy();
                            move |_, window, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::Edit(Command::SetHideShy(!hidden)),
                                        window,
                                        cx,
                                    )
                                })
                            }
                        }),
                    )
                    .child(
                        ui::tool(
                            "add-comp-marker",
                            "diamond",
                            "Add composition marker",
                            false,
                        )
                        .on_click({
                            let state = self.state.clone();
                            move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    s.dispatch(
                                        &Action::AddMarker(
                                            libre_effects_core::MarkerTarget::Composition,
                                        ),
                                        w,
                                        cx,
                                    )
                                })
                            }
                        }),
                    )
                    .child(
                        ui::tool(
                            "add-layer-marker",
                            "plus",
                            "Add selected layer marker",
                            false,
                        )
                        .when(selected.is_none(), |d| d.opacity(0.4))
                        .on_click({
                            let state = self.state.clone();
                            move |_, w, cx| {
                                state.update(cx, |s, cx| {
                                    if let Some(id) = s.editor.selected() {
                                        s.dispatch(
                                            &Action::AddMarker(
                                                libre_effects_core::MarkerTarget::Layer(id),
                                            ),
                                            w,
                                            cx,
                                        )
                                    }
                                })
                            }
                        }),
                    )
                    .child(div().flex_1())
                    .child(ui::action_tool(
                        "timeline-minus",
                        "minus",
                        "Zoom out (−)",
                        &self.state,
                        Action::ZoomTimeline(0.5),
                        false,
                    ))
                    .child(div().w(px(38.0)).text_center().child(format!("{zoom:.0}×")))
                    .child(ui::action_tool(
                        "timeline-plus",
                        "plus",
                        "Zoom in (+)",
                        &self.state,
                        Action::ZoomTimeline(2.0),
                        false,
                    )),
            )
            .child(layer_filters)
            .child(
                div()
                    .flex()
                    .h(px(22.0))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(ui::BORDER))
                    .child(
                        div()
                            .w(px(left))
                            .flex_none()
                            .pl_2()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child("Composition markers"),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .children(
                                comp.markers()
                                    .iter()
                                    .filter(|m| {
                                        m.frame() < start.saturating_add(visible)
                                            && m.end() >= start
                                    })
                                    .map(|m| {
                                        super::markers::marker_item(
                                            m,
                                            libre_effects_core::MarkerTarget::Composition,
                                            start,
                                            visible,
                                            &self.state,
                                        )
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .h(px(29.0))
                    .flex_none()
                    .bg(rgb(0x262626))
                    .child(
                        div()
                            .w(px(left))
                            .flex_none()
                            .flex()
                            .items_end()
                            .px_2()
                            .pb_1()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(div().w(px(178.0)).child("Switches"))
                            .child(div().flex_1().child("Source Name"))
                            .when(show_modes, |s| s.child(div().w(px(88.0)).child("Mode")))
                            .when(show_mattes, |s| {
                                s.child(div().w(px(200.0)).child("Track Matte"))
                            })
                            .child(div().w(px(181.0)).child("Parent & Link       Order")),
                    )
                    .child(
                        div()
                            .id("time-ruler")
                            .relative()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .cursor_crosshair()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&this.focus);
                                    this.selected_key = None;
                                    this.scrubbing = true;
                                    if let Some(position) = this.seek_position(event.position.x, cx)
                                    {
                                        let state = this.state.read(cx);
                                        let comp = state.editor.project().composition();
                                        let targets = super::timeline_snap::targets(
                                            comp,
                                            None,
                                            &BTreeSet::new(),
                                            &BTreeSet::new(),
                                        );
                                        let (frame, snap) =
                                            if state.snapping && !event.modifiers.alt {
                                                super::timeline_snap::snap_delta(
                                                    position,
                                                    &[0],
                                                    &targets,
                                                    state.visible_frames(),
                                                    this.ruler
                                                        .get()
                                                        .map_or(0.0, |b| f32::from(b.size.width)),
                                                    (0, i64::from(comp.duration() - 1)),
                                                )
                                            } else {
                                                (position.round() as i64, None)
                                            };
                                        this.snapped_to = snap;
                                        this.state.update(cx, |state, cx| {
                                            state.dispatch(&Action::Seek(frame as u32), window, cx)
                                        });
                                    }
                                }),
                            )
                            .child(
                                canvas(move |rect, _, _| bounds.set(Some(rect)), |_, _, _, _| ())
                                    .absolute()
                                    .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .left(relative(work_left))
                                    .w(relative((work_right - work_left).max(0.0)))
                                    .top_0()
                                    .h(px(8.0))
                                    .bg(rgb(0x69634e))
                                    .border_l_2()
                                    .border_r_2()
                                    .border_color(rgb(ui::BLUE)),
                            )
                            .children((0..10).map(|tick| {
                                div()
                                    .absolute()
                                    .left(relative(tick as f32 / 10.0))
                                    .top(px(10.0))
                                    .h(px(19.0))
                                    .border_l_1()
                                    .border_color(rgb(0x777777))
                                    .pl_1()
                                    .text_size(px(10.0))
                                    .child(format!(
                                        "{:.2}s",
                                        (start as f32 + visible as f32 * tick as f32 / 10.0)
                                            / comp.fps().as_f64() as f32
                                    ))
                            }))
                            .children(cached_ranges.iter().filter_map(|range| {
                                let first = range.start.max(start);
                                let end = range.end.min(start.saturating_add(visible));
                                (first < end).then(|| {
                                    div()
                                        .absolute()
                                        .left(relative((first - start) as f32 / visible as f32))
                                        .w(relative((end - first) as f32 / visible as f32))
                                        .bottom_0()
                                        .h(px(3.0))
                                        .bg(rgb(0x58bf96))
                                })
                            }))
                            .when(frame >= start && frame <= start + visible, |s| {
                                s.child(
                                    div()
                                        .absolute()
                                        .top(px(8.0))
                                        .left(relative(
                                            frame.saturating_sub(start) as f32 / visible as f32,
                                        ))
                                        .ml(px(-3.0))
                                        .w(px(7.0))
                                        .h(px(8.0))
                                        .bg(rgb(ui::BLUE)),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("timeline-body")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            if this.state.read(cx).graph_open
                                || this.ruler.get().is_none_or(|b| event.position.x < b.left())
                            {
                                return;
                            }
                            window.focus(&this.focus);
                            this.colors.clear_selection();
                            this.marquee_additive =
                                event.modifiers.shift || event.modifiers.control;
                            this.marquee = Some((event.position, event.position));
                            cx.notify();
                        }),
                    )
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        rows.id("timeline-rows")
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.row_scroll)
                            .when(graph_open, |s| {
                                s.w(px(left)).flex_none().overflow_x_hidden()
                            })
                            .when(!graph_open, |s| s.flex_1()),
                    )
                    .when(graph_open, |s| {
                        s.child(div().flex_1().min_w_0().min_h_0().child(self.graph.clone()))
                    }),
            )
            .child(
                div()
                    .h(px(29.0))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(ui::BORDER))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_2()
                    .child(ui::action_tool(
                        "pan-time-left",
                        "arrow-left",
                        "Pan timeline left",
                        &self.state,
                        Action::PanTimeline(-(visible as i32 / 4).max(1)),
                        false,
                    ))
                    .child(ui::action_tool(
                        "pan-time-right",
                        "arrow-right",
                        "Pan timeline right",
                        &self.state,
                        Action::PanTimeline((visible as i32 / 4).max(1)),
                        false,
                    ))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(format!(
                                "View: {}–{}f",
                                start,
                                (start + visible).min(comp.duration())
                            )),
                    )
                    .child(div().flex_1())
                    .child(ui::text_button("work-start", "Set In (B)").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(&Action::WorkStart, window, cx)
                            })
                        }
                    }))
                    .child(ui::text_button("work-end", "Set Out (N)").on_click({
                        let state = self.state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.dispatch(&Action::WorkEnd, window, cx)
                            })
                        }
                    }))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(ui::MUTED))
                            .child(format!("Work area: {work_start}–{work_end}f")),
                    ),
            )
            .child(
                div()
                    .id("timeline-column-divider")
                    .absolute()
                    .left(px(left - 2.0))
                    .top(px(58.0))
                    .bottom(px(29.0))
                    .w(px(4.0))
                    .cursor_col_resize()
                    .hover(|s| s.bg(rgb(ui::BLUE)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resizing = true;
                            cx.stop_propagation();
                        }),
                    )
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        if event.click_count() == 2 {
                            this.left = LEFT;
                            cx.notify();
                        }
                    })),
            )
            .when(self.marquee.is_some(), |s| {
                let (a, b) = self.marquee.unwrap();
                s.child(
                    canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            let rect = Bounds::from_corners(
                                point(a.x.min(b.x), a.y.min(b.y)),
                                point(a.x.max(b.x), a.y.max(b.y)),
                            );
                            window.paint_quad(fill(rect, gpui::rgba(0x529bdf35)));
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .when(self.snapped_to.is_some(), |d| {
                let at = self.snapped_to.unwrap();
                let ruler = self.ruler.get();
                d.child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            if let Some(r) = ruler {
                                let x = r.left()
                                    + r.size.width * ((at as f32 - start as f32) / visible as f32);
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(x, bounds.top() + px(58.0)),
                                        size(px(1.0), bounds.size.height - px(88.0)),
                                    ),
                                    rgb(0xe7bc6a),
                                ));
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
            })
            .when_some(key_menu, |d, menu| d.child(menu))
            .when(marker_open, |d| {
                d.child(
                    gpui::deferred(
                        div()
                            .absolute()
                            .right(px(8.0))
                            .bottom(px(30.0))
                            .occlude()
                            .child(self.marker_editor.clone()),
                    )
                    .with_priority(5),
                )
            })
            .into_any_element()
    }
}
#[cfg(test)]
mod tests {
    use libre_effects_core::{
        Command, Content, Editor, PropertyPath, TextPaint, TextParam, TextSelectorParam, TrackEdit,
    };
    #[test]
    fn native_opacity_has_one_filtered_read_only_row_and_no_scalar_graph_lane() {
        let editor = crate::opacity_test_support::overshoot_editor(true);
        let layer = editor.selected_layer().unwrap();
        let properties = [libre_effects_core::Property::Opacity.into()];
        for filter in [
            None,
            Some(super::PropertyFilter::Opacity),
            Some(super::PropertyFilter::Animated),
        ] {
            assert!(super::native_opacity_visible(layer, filter));
            assert!(!super::group_visible(layer, &properties, filter));
        }
        assert!(!super::native_opacity_visible(
            layer,
            Some(super::PropertyFilter::Position)
        ));
        assert!(
            !crate::view_state::GraphChannel {
                id: 1,
                property: properties[0]
            }
            .available(editor.project().composition())
        );
        assert_eq!(layer.opacity_key_count(), 2);
    }

    #[test]
    fn ordinary_2d_position_keeps_scalar_lanes_and_has_no_joined_row() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        let layer = editor.selected_layer().unwrap();
        let position = [
            libre_effects_core::Property::PositionX.into(),
            libre_effects_core::Property::PositionY.into(),
        ];
        assert!(super::group_visible(layer, &position, None));
        assert!(super::group_visible(
            layer,
            &position,
            Some(super::PropertyFilter::Position)
        ));
        for filter in [
            None,
            Some(super::PropertyFilter::Position),
            Some(super::PropertyFilter::Animated),
        ] {
            assert!(!super::joined_position_visible(layer, filter));
        }
    }

    #[test]
    fn joined_position_has_one_read_only_row_and_no_scalar_position_group() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        let layer = editor.selected_layer().unwrap();
        let position = [
            libre_effects_core::Property::PositionX.into(),
            libre_effects_core::Property::PositionY.into(),
        ];
        for filter in [
            None,
            Some(super::PropertyFilter::Position),
            Some(super::PropertyFilter::Animated),
        ] {
            assert!(!super::group_visible(layer, &position, filter));
        }
        assert!(super::joined_position_visible(layer, None));
        assert!(super::joined_position_visible(
            layer,
            Some(super::PropertyFilter::Position)
        ));
        assert!(!super::joined_position_visible(
            layer,
            Some(super::PropertyFilter::Animated)
        ));
        assert!(!super::joined_position_visible(
            layer,
            Some(super::PropertyFilter::Opacity)
        ));
        assert!(super::group_visible(
            layer,
            &[libre_effects_core::Property::Opacity.into()],
            None
        ));
        editor
            .execute(Command::SetSpatialPosition {
                id: 1,
                edit: libre_effects_core::SpatialEdit::Key {
                    frame: 10,
                    value: [12.0, 34.0, 56.0],
                },
            })
            .unwrap();
        let layer = editor.selected_layer().unwrap();
        assert!(super::joined_position_visible(
            layer,
            Some(super::PropertyFilter::Animated)
        ));
        assert!(!super::group_visible(
            layer,
            &position,
            Some(super::PropertyFilter::Animated)
        ));
    }

    #[test]
    fn typography_timeline_numeric_formatting_is_noop_at_interpolated_frames() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Title".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Text".into(),
        })
        .unwrap();
        for (parameter, value) in [
            (TextParam::FontSize, 91.123456789),
            (TextParam::Tracking, 135.987654321),
            (TextParam::Leading, 2.987654321),
            (TextParam::FillOpacity, 37.123456789),
            (TextParam::StrokeOpacity, 86.987654321),
        ] {
            e.execute(Command::EditText {
                id: 1,
                parameter,
                edit: TrackEdit::ToggleAnimation { frame: 0 },
            })
            .unwrap();
            let command = e
                .selected_layer()
                .unwrap()
                .text_value_command(parameter, value, 60)
                .unwrap()
                .unwrap();
            e.execute(command).unwrap();
            let sample = e
                .selected_layer()
                .unwrap()
                .text_value_at(parameter, 17)
                .unwrap();
            let before = e.project().clone();
            for input in [format!("  {sample}  "), format!("\t{sample:e}\n")] {
                let parsed = super::parse_scalar(&input).unwrap();
                assert_eq!(parsed, sample);
                assert!(
                    e.selected_layer()
                        .unwrap()
                        .text_value_command(parameter, parsed, 17)
                        .unwrap()
                        .is_none()
                );
            }
            assert_eq!(e.project(), &before);
            assert!(
                !e.selected_layer()
                    .unwrap()
                    .track(PropertyPath::Text(parameter))
                    .unwrap()
                    .keys()
                    .contains_key(&17)
            );
        }
        assert!(super::parse_scalar("invalid").is_err());
    }

    #[test]
    fn text_paint_timeline_groups_keep_sparse_baselines_and_animated_filter_coherent() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Text".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 100.,
            name: "Title".into(),
        })
        .unwrap();
        let groups = super::text_groups(e.selected_layer().unwrap());
        assert_eq!(
            groups
                .iter()
                .map(|(s, p)| (s.as_str(), p.len()))
                .collect::<Vec<_>>(),
            [
                ("Source Text · Hold", 1),
                ("Fill Color", 3),
                ("Stroke Color", 3),
                ("Stroke Width", 1),
                ("Font Size", 1),
                ("Tracking", 1),
                ("Leading", 1),
                ("Fill Opacity", 1),
                ("Stroke Opacity", 1),
                ("Range Start", 1),
                ("Range End", 1),
                ("Range Offset", 1),
                ("Amount", 1),
                ("Scale X", 1),
                ("Scale Y", 1),
                ("Rotation", 1),
                ("Position X", 1),
                ("Position Y", 1),
                ("Opacity", 1)
            ]
        );
        // Source enums append new channels for stable storage; the UI groups
        // Offset beside Start/End. Every source channel still appears once.
        assert_eq!(
            groups
                .iter()
                .flat_map(|(_, p)| p.iter())
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            std::iter::once(PropertyPath::SourceText)
                .chain(TextParam::ALL.map(PropertyPath::Text))
                .collect::<std::collections::BTreeSet<_>>()
        );
        for (_, properties) in &groups {
            assert!(super::group_visible(
                e.selected_layer().unwrap(),
                properties,
                None
            ));
            assert!(!super::group_visible(
                e.selected_layer().unwrap(),
                properties,
                Some(super::PropertyFilter::Animated)
            ));
            assert!(!super::group_visible(
                e.selected_layer().unwrap(),
                properties,
                Some(super::PropertyFilter::Opacity)
            ));
            for p in properties {
                assert_eq!(
                    e.selected_layer().unwrap().track_value(*p, 30).is_some(),
                    *p != PropertyPath::SourceText,
                );
            }
        }
        e.execute(Command::EditText {
            id: 1,
            parameter: TextParam::FillGreen,
            edit: TrackEdit::ToggleAnimation { frame: 0 },
        })
        .unwrap();
        let fill = &groups[1].1;
        assert!(super::group_animated(e.selected_layer().unwrap(), fill));
        assert!(super::group_visible(
            e.selected_layer().unwrap(),
            fill,
            Some(super::PropertyFilter::Animated)
        ));
        let command = super::group_watch(e.selected_layer().unwrap(), fill, 30);
        e.execute(command).unwrap();
        assert!(
            !e.selected_layer()
                .unwrap()
                .text_color_animated(TextPaint::Fill)
        );
        assert!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::Text(TextParam::FillRed))
                .is_none()
        );
        let command = super::group_watch(e.selected_layer().unwrap(), fill, 0);
        e.execute(command).unwrap();
        for p in fill {
            assert!(
                e.selected_layer()
                    .unwrap()
                    .track(*p)
                    .unwrap()
                    .keys()
                    .contains_key(&0)
            );
        }
        assert!(
            e.selected_layer()
                .unwrap()
                .text_color_animated(TextPaint::Fill)
        );
        assert!(
            !e.selected_layer()
                .unwrap()
                .text_color_animated(TextPaint::Stroke)
        );
        e.execute(Command::AddSolid).unwrap();
        assert!(super::text_groups(e.selected_layer().unwrap()).is_empty());
    }

    #[test]
    fn text_animator_timeline_groups_keep_independent_sparse_values_units_and_filters() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Range".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Animator lanes".into(),
        })
        .unwrap();
        let groups = super::text_groups(e.selected_layer().unwrap());
        let original = e.project().clone();
        for (index, (parameter, default)) in [
            (TextParam::AnimatorStart, 0.),
            (TextParam::AnimatorEnd, 100.),
            (TextParam::AnimatorOffset, 0.),
            (TextParam::AnimatorAmount, 100.),
            (TextParam::AnimatorScaleX, 100.),
            (TextParam::AnimatorScaleY, 100.),
            (TextParam::AnimatorRotation, 0.),
            (TextParam::AnimatorPositionX, 0.),
            (TextParam::AnimatorPositionY, 0.),
            (TextParam::AnimatorOpacity, 100.),
        ]
        .into_iter()
        .enumerate()
        {
            let paths = &groups[index + 9].1;
            assert_eq!(paths, &[PropertyPath::Text(parameter)]);
            assert_eq!(
                super::text_channel_label(parameter),
                match parameter {
                    TextParam::AnimatorPositionX | TextParam::AnimatorPositionY => "px",
                    TextParam::AnimatorRotation => "°",
                    _ => "%",
                }
            );
            assert_eq!(
                e.selected_layer().unwrap().track_value(paths[0], 30),
                Some(default)
            );
            assert!(e.selected_layer().unwrap().track(paths[0]).is_none());
            assert!(!super::group_visible(
                e.selected_layer().unwrap(),
                paths,
                Some(super::PropertyFilter::Animated)
            ));
        }
        assert_eq!(e.project(), &original);
        for (index, (_, paths)) in groups[9..].iter().enumerate() {
            e.execute(super::group_watch(e.selected_layer().unwrap(), paths, 30))
                .unwrap();
            assert!(super::group_visible(
                e.selected_layer().unwrap(),
                paths,
                Some(super::PropertyFilter::Animated)
            ));
            for (_, other) in groups[9..].iter().skip(index + 1) {
                assert!(!super::group_animated(e.selected_layer().unwrap(), other));
            }
            e.execute(super::group_watch(e.selected_layer().unwrap(), paths, 30))
                .unwrap();
            assert!(!super::group_animated(e.selected_layer().unwrap(), paths));
        }
    }

    #[test]
    fn secondary_selector_timeline_lanes_follow_order_but_keep_stable_scalar_identity() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Selectors".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Text".into(),
            })
            .unwrap();
        let legacy = super::text_groups(editor.selected_layer().unwrap());
        for _ in 0..2 {
            editor
                .execute(Command::AddTextRangeSelector { id: 1 })
                .unwrap();
        }
        let source = editor.project().clone();
        let groups = super::text_groups(editor.selected_layer().unwrap());
        assert_eq!(&groups[..legacy.len()], legacy.as_slice());
        assert_eq!(groups.len(), legacy.len() + 8);
        for (index, (label, paths)) in groups[legacy.len()..].iter().enumerate() {
            let selector = 1 + (index / 4) as u64;
            let parameter = TextSelectorParam::ALL[index % 4];
            assert_eq!(label, parameter.label());
            assert_eq!(
                paths,
                &[PropertyPath::TextSelector {
                    selector,
                    parameter
                }]
            );
            assert!(editor.selected_layer().unwrap().track(paths[0]).is_none());
            assert!(
                editor
                    .selected_layer()
                    .unwrap()
                    .track_value(paths[0], 30)
                    .is_some()
            );
            assert!(!super::group_visible(
                editor.selected_layer().unwrap(),
                paths,
                Some(super::PropertyFilter::Animated)
            ));
        }
        assert_eq!(editor.project(), &source);
        let lane = groups[legacy.len() + 6].1.clone();
        editor
            .execute(super::group_watch(
                editor.selected_layer().unwrap(),
                &lane,
                30,
            ))
            .unwrap();
        assert!(super::group_visible(
            editor.selected_layer().unwrap(),
            &lane,
            Some(super::PropertyFilter::Animated)
        ));
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .track(lane[0])
                .unwrap()
                .keys()
                .contains_key(&30)
        );
        editor
            .execute(Command::MoveTextRangeSelector {
                id: 1,
                selector: 2,
                index: 0,
            })
            .unwrap();
        let reordered = super::text_groups(editor.selected_layer().unwrap());
        assert_eq!(
            &reordered[legacy.len()..legacy.len() + 4],
            &groups[legacy.len() + 4..]
        );
        assert_eq!(
            &reordered[legacy.len() + 4..],
            &groups[legacy.len()..legacy.len() + 4]
        );
        assert!(super::group_animated(
            editor.selected_layer().unwrap(),
            &lane
        ));
        editor
            .execute(Command::RemoveTextRangeSelector { id: 1, selector: 2 })
            .unwrap();
        assert_eq!(
            super::text_groups(editor.selected_layer().unwrap()).len(),
            legacy.len() + 4
        );
        editor.undo();
        assert_eq!(
            super::text_groups(editor.selected_layer().unwrap()),
            reordered
        );
        assert!(super::group_animated(
            editor.selected_layer().unwrap(),
            &lane
        ));
    }

    #[test]
    fn text_animator_timeline_displays_static_selector_metadata_without_extra_lanes() {
        use libre_effects_core::{TextSelector, TextSelectorShape, TextSelectorUnits};
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Words\nLines".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 180.,
                name: "Selector metadata".into(),
            })
            .unwrap();
        let initial_groups = super::text_groups(editor.selected_layer().unwrap());
        assert_eq!(
            super::animator_section_label(editor.selected_layer().unwrap()),
            "Text Animator · Characters · Square"
        );
        assert_eq!(
            initial_groups[12],
            (
                "Amount".to_string(),
                vec![PropertyPath::Text(TextParam::AnimatorAmount)]
            )
        );
        for units in TextSelectorUnits::ALL {
            for shape in TextSelectorShape::ALL {
                editor
                    .execute(Command::SetTextSelector {
                        id: 1,
                        selector: TextSelector { units, shape },
                    })
                    .unwrap();
                let layer = editor.selected_layer().unwrap();
                assert_eq!(
                    super::animator_section_label(layer),
                    format!("Text Animator · {} · {}", units.label(), shape.label())
                );
                assert_eq!(super::text_groups(layer), initial_groups);
                for (_, paths) in &initial_groups[9..] {
                    assert!(!super::group_animated(layer, paths));
                }
            }
        }
    }

    #[test]
    fn text_opacity_timeline_has_independent_percentage_groups_and_watches() {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Opacity".into(),
                font_size: 48.,
            },
            width: 400.,
            height: 120.,
            name: "Text".into(),
        })
        .unwrap();
        let source = e.project().clone();
        let groups = super::text_groups(e.selected_layer().unwrap());
        assert_eq!(groups.iter().flat_map(|(_, paths)| paths).count(), 23);
        for (paint, index) in [(TextPaint::Fill, 7), (TextPaint::Stroke, 8)] {
            let parameter = paint.opacity();
            let paths = &groups[index].1;
            assert_eq!(paths, &[PropertyPath::Text(parameter)]);
            assert_eq!(super::text_channel_label(parameter), "%");
            assert_eq!(
                e.selected_layer().unwrap().track_value(paths[0], 30),
                Some(100.)
            );
            assert!(e.selected_layer().unwrap().track(paths[0]).is_none());
            assert!(!super::group_animated(e.selected_layer().unwrap(), paths));
        }
        assert_eq!(e.project(), &source);
        let fill = &groups[7].1;
        let stroke = &groups[8].1;
        e.execute(super::group_watch(e.selected_layer().unwrap(), fill, 30))
            .unwrap();
        assert!(super::group_animated(e.selected_layer().unwrap(), fill));
        assert!(!super::group_animated(e.selected_layer().unwrap(), stroke));
        assert!(
            !e.selected_layer()
                .unwrap()
                .text_color_animated(TextPaint::Fill)
        );
        assert!(
            !e.selected_layer()
                .unwrap()
                .text_color_animated(TextPaint::Stroke)
        );
        assert!(super::group_visible(
            e.selected_layer().unwrap(),
            fill,
            Some(super::PropertyFilter::Animated)
        ));
        e.execute(super::group_watch(e.selected_layer().unwrap(), stroke, 40))
            .unwrap();
        let stroke_before = e.selected_layer().unwrap().track(stroke[0]).cloned();
        e.execute(super::group_watch(e.selected_layer().unwrap(), fill, 50))
            .unwrap();
        assert!(!super::group_animated(e.selected_layer().unwrap(), fill));
        assert_eq!(
            e.selected_layer().unwrap().track(stroke[0]),
            stroke_before.as_ref()
        );
        assert!(!super::group_visible(
            e.selected_layer().unwrap(),
            fill,
            Some(super::PropertyFilter::Opacity)
        ));
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track_value(libre_effects_core::Property::Opacity.into(), 30),
            Some(100.)
        );
    }

    use super::*;
    #[test]
    fn ruler_mapping_accounts_for_pan_zoom_and_edges() {
        assert_eq!(frame_at(150.0, 100.0, 100.0, 30, 60, 150), 60.0);
        assert_eq!(frame_at(-50.0, 100.0, 100.0, 30, 60, 150), 30.0);
        assert_eq!(frame_at(500.0, 100.0, 100.0, 100, 60, 150), 149.0);
    }
}

#[cfg(test)]
mod source_text_tests {
    use super::*;
    use libre_effects_core::{Content, Editor, Interpolation, KeyScale};

    fn editor() -> Editor {
        let mut e = Editor::default();
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Start 🦋".into(),
                font_size: 48.0,
            },
            width: 400.0,
            height: 120.0,
            name: "Text".into(),
        })
        .unwrap();
        e
    }

    fn command(e: &Editor, frame: u32, control: SourceTextControl) -> Command {
        let Some(Action::Edit(command)) =
            source_text_action(e.selected_layer().unwrap(), frame, control)
        else {
            panic!("Expected Source Text command")
        };
        command
    }

    #[test]
    fn source_text_timeline_controls_hold_watch_key_navigation_and_sample_summary() {
        let mut e = editor();
        assert_eq!(
            text_groups(e.selected_layer().unwrap())[0],
            ("Source Text · Hold".into(), vec![PropertyPath::SourceText])
        );
        assert_eq!(
            source_text_summary(e.selected_layer().unwrap(), 15),
            "Start 🦋"
        );
        assert!(
            source_text_action(e.selected_layer().unwrap(), 15, SourceTextControl::Previous)
                .is_none()
        );
        assert!(
            source_text_action(e.selected_layer().unwrap(), 15, SourceTextControl::Next).is_none()
        );
        assert!(matches!(
            source_text_action(e.selected_layer().unwrap(), 15, SourceTextControl::Edit),
            Some(Action::BeginText(Some(1), _))
        ));
        e.execute(command(&e, 10, SourceTextControl::Animation))
            .unwrap();
        e.execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: "".into(),
        })
        .unwrap();
        e.execute(Command::EditSourceText {
            id: 1,
            frame: 50,
            text: "世界\n🦋\tline".into(),
        })
        .unwrap();
        assert_eq!(
            source_text_summary(e.selected_layer().unwrap(), 29),
            "Start 🦋"
        );
        assert_eq!(
            source_text_summary(e.selected_layer().unwrap(), 30),
            "(empty)"
        );
        assert_eq!(
            source_text_summary(e.selected_layer().unwrap(), 50),
            "世界↵🦋⇥line"
        );
        for (frame, previous, next) in [
            (0, None, Some(10)),
            (10, None, Some(30)),
            (30, Some(10), Some(50)),
            (60, Some(50), None),
        ] {
            let seek = |kind| match source_text_action(e.selected_layer().unwrap(), frame, kind) {
                Some(Action::Seek(frame)) => Some(frame),
                None => None,
                _ => panic!("Expected seek"),
            };
            assert_eq!(seek(SourceTextControl::Previous), previous);
            assert_eq!(seek(SourceTextControl::Next), next);
        }
        e.execute(command(&e, 40, SourceTextControl::Key)).unwrap();
        let track = e
            .selected_layer()
            .unwrap()
            .track(PropertyPath::SourceText)
            .unwrap();
        assert_eq!(track.keys().len(), 4);
        assert!(
            track
                .keys()
                .values()
                .all(|k| k.interpolation == Interpolation::Hold)
        );
        assert_eq!(e.selected_layer().unwrap().source_text_at(40), Some(""));
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track_value(PropertyPath::SourceText, 40),
            None
        );
        assert!(group_visible(
            e.selected_layer().unwrap(),
            &[PropertyPath::SourceText],
            Some(PropertyFilter::Animated)
        ));
        e.execute(command(&e, 40, SourceTextControl::Animation))
            .unwrap();
        assert_eq!(e.selected_layer().unwrap().source_text_at(0), Some(""));
        assert!(!group_animated(
            e.selected_layer().unwrap(),
            &[PropertyPath::SourceText]
        ));
        e.execute(Command::ToggleLocked(1)).unwrap();
        for kind in [
            SourceTextControl::Animation,
            SourceTextControl::Key,
            SourceTextControl::Edit,
            SourceTextControl::Previous,
            SourceTextControl::Next,
        ] {
            assert!(source_text_action(e.selected_layer().unwrap(), 40, kind).is_none());
        }
    }

    #[test]
    fn source_text_timeline_copy_retime_and_delete_preserve_string_payload_and_hold_keys() {
        let mut e = editor();
        e.execute(command(&e, 10, SourceTextControl::Animation))
            .unwrap();
        e.execute(Command::EditSourceText {
            id: 1,
            frame: 30,
            text: "Copied 世界".into(),
        })
        .unwrap();
        let key = KeyRef {
            id: 1,
            property: PropertyPath::SourceText,
            frame: 30,
        };
        let copy = e
            .selected_layer()
            .unwrap()
            .copy_key(key.property, key.frame)
            .unwrap();
        assert_eq!(copy.source_text.as_deref(), Some("Copied 世界"));
        e.execute(Command::AddContent {
            content: Content::Text {
                text: "Unrelated pool baseline".into(),
                font_size: 60.0,
            },
            width: 400.0,
            height: 120.0,
            name: "Target".into(),
        })
        .unwrap();
        let target = e.selected().unwrap();
        e.execute(command(&e, 0, SourceTextControl::Animation))
            .unwrap();
        e.execute(Command::PasteKeys {
            keys: vec![copy],
            frame: 40,
            target: Some(target),
        })
        .unwrap();
        let pasted = KeyRef {
            id: target,
            property: PropertyPath::SourceText,
            frame: 40,
        };
        assert_eq!(
            e.selected_layer().unwrap().source_text_at(40),
            Some("Copied 世界")
        );
        e.execute(Command::MoveKeys {
            keys: vec![pasted],
            delta: 5,
        })
        .unwrap();
        let moved = KeyRef {
            frame: 45,
            ..pasted
        };
        e.execute(Command::ScaleKeys {
            keys: vec![moved],
            scale: KeyScale {
                time_origin: 0.0,
                time_scale: 2.0,
                value_origin: 0.0,
                value_scale: 1.0,
            },
        })
        .unwrap();
        assert_eq!(
            e.selected_layer().unwrap().source_text_at(89),
            Some("Unrelated pool baseline")
        );
        assert_eq!(
            e.selected_layer().unwrap().source_text_at(90),
            Some("Copied 世界")
        );
        let keys = e
            .selected_layer()
            .unwrap()
            .track(PropertyPath::SourceText)
            .unwrap()
            .keys()
            .keys()
            .map(|frame| KeyRef {
                id: target,
                property: PropertyPath::SourceText,
                frame: *frame,
            })
            .collect();
        let before = e.project().clone();
        e.execute(Command::DeleteKeys(keys)).unwrap();
        assert_eq!(
            e.selected_layer().unwrap().source_text_at(0),
            Some("Copied 世界")
        );
        assert!(!group_animated(
            e.selected_layer().unwrap(),
            &[PropertyPath::SourceText]
        ));
        e.undo();
        assert_eq!(e.project(), &before);
        assert_eq!(
            e.selected_layer()
                .unwrap()
                .track(PropertyPath::SourceText)
                .unwrap()
                .keys()[&90]
                .interpolation,
            Interpolation::Hold
        );
    }
}

#[cfg(test)]
mod animator_stack_timeline_tests {
    use super::*;
    use libre_effects_core::{Content, Editor};
    #[test]
    fn extra_animator_lanes_follow_stack_order_with_stable_ids_and_sparse_filters() {
        let mut editor = Editor::default();
        editor
            .execute(Command::AddContent {
                content: Content::Text {
                    text: "Stack".into(),
                    font_size: 48.,
                },
                width: 400.,
                height: 120.,
                name: "Text".into(),
            })
            .unwrap();
        let legacy = text_groups(editor.selected_layer().unwrap());
        for _ in 0..2 {
            editor.execute(Command::AddTextAnimator { id: 1 }).unwrap();
        }
        let groups = text_groups(editor.selected_layer().unwrap());
        assert_eq!(&groups[..legacy.len()], legacy.as_slice());
        assert_eq!(groups.len(), legacy.len() + 20);
        let mut lanes = vec![];
        for (index, (label, paths)) in groups[legacy.len()..].iter().enumerate() {
            let parameter = super::super::text_animator::ANIMATOR_PARAMETERS[index % 10];
            let path = PropertyPath::TextAnimator {
                animator: 1 + (index / 10) as u64,
                parameter,
            };
            assert_eq!(paths, &[path]);
            assert_eq!(label, parameter.label().trim_start_matches("Animator · "));
            let layer = editor.selected_layer().unwrap();
            assert!(layer.track(path).is_none());
            assert_eq!(
                layer.track_value(path, 10),
                layer.text_value_at(parameter, 10)
            );
            assert!(!group_visible(layer, paths, Some(PropertyFilter::Animated)));
            lanes.push(path);
        }
        editor
            .execute(group_watch(
                editor.selected_layer().unwrap(),
                &[lanes[19]],
                10,
            ))
            .unwrap();
        assert!(group_visible(
            editor.selected_layer().unwrap(),
            &[lanes[19]],
            Some(PropertyFilter::Animated)
        ));
        editor
            .execute(Command::MoveTextAnimator {
                id: 1,
                animator: 2,
                index: 0,
            })
            .unwrap();
        let reordered = text_groups(editor.selected_layer().unwrap());
        assert_eq!(reordered[legacy.len()].1, vec![lanes[10]]);
        assert_eq!(reordered[legacy.len() + 10].1, vec![lanes[0]]);
        assert!(
            editor
                .selected_layer()
                .unwrap()
                .track(lanes[19])
                .unwrap()
                .keys()
                .contains_key(&10)
        );
    }
}
