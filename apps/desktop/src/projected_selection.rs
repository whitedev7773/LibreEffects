//! GPUI-free selection geometry, sampled from the compositor's projection/order.
use libre_effects_core::{Composition, Layer, LayerId};
use std::collections::BTreeSet;

pub(crate) fn point_in_quad(point: [f64; 2], corners: [[f64; 2]; 4]) -> bool {
    let mut positive = false;
    let mut negative = false;
    let mut area = 0.0;
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        positive |= cross > 0.0;
        negative |= cross < 0.0;
        area += a[0] * b[1] - b[0] * a[1];
    }
    area.abs() > 0.001 && !(positive && negative)
}

/// Back-to-front controls use pixel paint order. Nulls and selected hidden matte
/// controls are separate overlays, appended in their existing stack order.
/// A Null has no painted plane: if it cannot project, omit only its controls.
/// Invalid visible paint geometry and hidden matte geometry still return errors.
pub(crate) fn projected_control_order(
    comp: &Composition,
    frame: u32,
    selected: &BTreeSet<LayerId>,
) -> Result<Vec<LayerId>, String> {
    let mut order = comp.render_order(frame, true)?;
    let active: BTreeSet<_> = order.iter().copied().collect();
    for layer in comp.layers().iter().rev().filter(|layer| {
        !active.contains(&layer.id())
            && libre_effects_editor_model::preview_scene::controls_active(
                comp,
                layer,
                frame,
                selected.contains(&layer.id()),
            )
    }) {
        match comp.projected_geometry(layer.id(), frame) {
            Ok(_) => order.push(layer.id()),
            Err(_) if matches!(layer.content(), libre_effects_core::Content::Null) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(order)
}

pub(crate) fn projected_layer_hit(
    comp: &Composition,
    frame: u32,
    point: [f64; 2],
    selected: &BTreeSet<LayerId>,
    source_bounds: impl Fn(&Layer) -> [f64; 4],
) -> Result<Option<LayerId>, String> {
    let order = projected_control_order(comp, frame, selected)?;
    for layer in order.iter().rev().filter_map(|id| comp.layer(*id)) {
        if layer.locked()
            || !libre_effects_editor_model::preview_scene::controls_active(
                comp,
                layer,
                frame,
                selected.contains(&layer.id()),
            )
        {
            continue;
        }
        let projected = comp.projected_geometry(layer.id(), frame)?;
        let [x, y, width, height] = source_bounds(layer);
        let corners = [
            [x, y],
            [x + width, y],
            [x + width, y + height],
            [x, y + height],
        ]
        .map(|p| projected.transform.point(p));
        if point_in_quad(point, corners) {
            return Ok(Some(layer.id()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libre_effects_core::{Command, Editor, Property};

    fn hit(editor: &Editor, point: [f64; 2]) -> Option<LayerId> {
        projected_layer_hit(
            editor.project().composition(),
            0,
            point,
            &BTreeSet::new(),
            |layer| [0., 0., layer.width(), layer.height()],
        )
        .unwrap()
    }

    #[test]
    fn ordinary_stack_order_locked_layers_and_source_bounds_are_preserved() {
        let mut editor = Editor::default();
        editor.execute(Command::AddRectangle).unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        let comp = editor.project().composition();
        let point = comp
            .projected_geometry(1, 0)
            .unwrap()
            .transform
            .point([160., 100.]);
        assert_eq!(hit(&editor, point), Some(2));
        editor.execute(Command::ToggleLocked(2)).unwrap();
        assert_eq!(hit(&editor, point), Some(1));
        let comp = editor.project().composition();
        assert_eq!(
            projected_layer_hit(comp, 0, point, &BTreeSet::new(), |_| [0., 0., 1., 1.]).unwrap(),
            None
        );
        editor
            .execute(Command::SetValue {
                id: 1,
                property: Property::ScaleX,
                frame: 0,
                value: 0.,
            })
            .unwrap();
        assert_eq!(hit(&editor, point), None);
    }

    #[test]
    fn spatial_depth_ties_locked_layers_and_existing_selection_use_painted_geometry() {
        use libre_effects_core::{Camera3, SpatialEdit};
        let mut editor = Editor::default();
        editor
            .execute(Command::SetCamera {
                camera: Some(Camera3 {
                    position: [960., 540., -500.],
                    focal_distance: 500.,
                    principal_point: [960., 540.],
                    near_clip: 1.,
                }),
            })
            .unwrap();
        for _ in 0..2 {
            editor.execute(Command::AddRectangle).unwrap();
            let id = editor.selected().unwrap();
            editor
                .execute(Command::SetThreeD { id, enabled: true })
                .unwrap();
            editor
                .execute(Command::SetSpatialPosition {
                    id,
                    edit: SpatialEdit::Value([960., 540., if id == 1 { 0. } else { 500. }]),
                })
                .unwrap();
        }
        let point = [960., 540.];
        assert_eq!(hit(&editor, point), Some(1));
        assert_eq!(
            projected_layer_hit(
                editor.project().composition(),
                0,
                point,
                &[2].into(),
                |layer| [0., 0., layer.width(), layer.height()]
            )
            .unwrap(),
            Some(1)
        );
        assert_eq!(
            editor
                .project()
                .composition()
                .render_order(0, true)
                .unwrap(),
            [2, 1]
        );
        editor
            .execute(Command::SetSpatialPosition {
                id: 2,
                edit: SpatialEdit::Value([960., 540., 0.]),
            })
            .unwrap();
        assert_eq!(
            editor
                .project()
                .composition()
                .render_order(0, true)
                .unwrap(),
            [1, 2]
        );
        assert_eq!(hit(&editor, point), Some(2));
        editor.execute(Command::ToggleLocked(2)).unwrap();
        assert_eq!(hit(&editor, point), Some(1));
        editor.execute(Command::SetCamera { camera: None }).unwrap();
        assert!(
            projected_layer_hit(
                editor.project().composition(),
                0,
                point,
                &BTreeSet::new(),
                |layer| [0., 0., layer.width(), layer.height()]
            )
            .is_err()
        );
    }

    #[test]
    fn null_controls_are_overlays_without_imposing_pixel_camera_requirements() {
        use libre_effects_core::{Camera3, SpatialEdit};
        let mut editor = Editor::default();
        editor
            .execute(Command::SetCamera {
                camera: Some(Camera3 {
                    position: [960., 540., -500.],
                    focal_distance: 500.,
                    principal_point: [960., 540.],
                    near_clip: 1.,
                }),
            })
            .unwrap();
        editor.execute(Command::AddRectangle).unwrap();
        editor
            .execute(Command::SetThreeD {
                id: 1,
                enabled: true,
            })
            .unwrap();
        editor.execute(Command::AddNull).unwrap();
        assert_eq!(
            editor
                .project()
                .composition()
                .render_order(0, true)
                .unwrap(),
            [1]
        );
        assert_eq!(
            projected_control_order(editor.project().composition(), 0, &BTreeSet::new()).unwrap(),
            [1, 2]
        );
        assert_eq!(hit(&editor, [970., 550.]), Some(2));
        editor
            .execute(Command::SetThreeD {
                id: 2,
                enabled: true,
            })
            .unwrap();
        editor
            .execute(Command::SetSpatialPosition {
                id: 2,
                edit: SpatialEdit::Value([960., 540., -500.]),
            })
            .unwrap();
        assert_eq!(
            projected_control_order(editor.project().composition(), 0, &BTreeSet::new()).unwrap(),
            [1]
        );
        assert_eq!(hit(&editor, [960., 540.]), Some(1));
        editor.execute(Command::RemoveLayer(1)).unwrap();
        editor.execute(Command::SetCamera { camera: None }).unwrap();
        assert!(
            editor
                .project()
                .composition()
                .render_order(0, true)
                .unwrap()
                .is_empty()
        );
        assert!(
            projected_control_order(editor.project().composition(), 0, &BTreeSet::new())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn singular_and_reversed_winding_quads_do_not_invent_hits() {
        assert!(!point_in_quad([0., 0.], [[0., 0.]; 4]));
        assert!(point_in_quad(
            [2., 3.],
            [[4., 0.], [0., 0.], [0., 6.], [4., 6.]]
        ));
        assert!(!point_in_quad(
            [5., 3.],
            [[4., 0.], [0., 0.], [0., 6.], [4., 6.]]
        ));
    }
}
