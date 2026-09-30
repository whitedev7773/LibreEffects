use gpui::{
    Bounds, ContentMask, Context, Entity, PathBuilder, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};
use libre_effects_core::Property;

use crate::{editor::EditorState, theme::ActiveTheme};

pub(crate) struct Preview {
    state: Entity<EditorState>,
}

impl Preview {
    pub fn new(state: Entity<EditorState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { state }
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = window.theme().colors;
        let state = self.state.read(cx);
        let comp = state.editor.project().composition().clone();
        let frame = state.frame;
        let selected = state.editor.selected();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .h_8()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_xs()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(format!("Composition  /  {}", comp.name())),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let available_width = f32::from(bounds.size.width).max(1.0);
                        let available_height = f32::from(bounds.size.height).max(1.0);
                        let zoom = ((available_width - 32.0).max(1.0) / comp.width() as f32)
                            .min((available_height - 32.0).max(1.0) / comp.height() as f32);
                        let width = comp.width() as f32 * zoom;
                        let height = comp.height() as f32 * zoom;
                        let origin = point(
                            bounds.left() + px((available_width - width) / 2.0),
                            bounds.top() + px((available_height - height) / 2.0),
                        );
                        let stage = Bounds::new(origin, size(px(width), px(height)));
                        window.paint_quad(fill(stage, rgb(0x101117)));
                        window.with_content_mask(Some(ContentMask { bounds: stage }), |window| {
                            for layer in comp.layers().iter().rev().filter(|layer| layer.visible())
                            {
                                let corners = layer.corners_at(frame).map(|[x, y]| {
                                    point(
                                        origin.x + px(x as f32 * zoom),
                                        origin.y + px(y as f32 * zoom),
                                    )
                                });
                                let mut shape = PathBuilder::fill();
                                shape.move_to(corners[0]);
                                for corner in &corners[1..] {
                                    shape.line_to(*corner);
                                }
                                shape.close();
                                let mut color = rgb(layer.color());
                                color.a = (layer.property(Property::Opacity).value_at(frame)
                                    / 100.0) as f32;
                                if let Ok(path) = shape.build() {
                                    window.paint_path(path, color);
                                }
                                if selected == Some(layer.id()) {
                                    let mut outline = PathBuilder::stroke(px(1.0));
                                    outline.move_to(corners[0]);
                                    for corner in &corners[1..] {
                                        outline.line_to(*corner);
                                    }
                                    outline.close();
                                    if let Ok(path) = outline.build() {
                                        window.paint_path(path, rgb(0xb9acff));
                                    }
                                }
                            }
                        });
                    },
                )
                .flex_1()
                .w_full(),
            )
            .child(
                div()
                    .h_7()
                    .px_3()
                    .text_xs()
                    .text_color(colors.muted_foreground)
                    .child("Fit  /  2D composition preview"),
            )
    }
}
