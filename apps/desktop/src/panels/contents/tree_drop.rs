//! Pure drag hit testing and preview geometry for one completed tree layout.
//! Paint generation ownership lives in the UI; equality here deliberately ignores
//! repaint counters so an identical repaint does not invalidate a gesture.
use super::tree_selection::{DropPlan, DropTarget, Selection, bounded_rows, plan_drop};
use gpui::{Bounds, Pixels, Point, point, px, size};
use libre_effects_core::{CompositionId, ContentsKind, ShapeContents};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowGeometry {
    pub item: u64,
    pub parent: u64,
    pub depth: usize,
    pub group: bool,
    /// Full, unclipped row and label bounds. Controls are outside the label.
    pub bounds: Bounds<Pixels>,
    pub label: Bounds<Pixels>,
    /// The actual viewport/content mask, not the row's visible intersection.
    pub clip: Bounds<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RootGeometry {
    pub bounds: Bounds<Pixels>,
    pub clip: Bounds<Pixels>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Geometry {
    pub owner: (CompositionId, u64),
    /// Full logical visible order, including rows outside the viewport.
    pub visible: Vec<(usize, u64, u64)>,
    /// Only this completed generation's registrations, in logical visible order.
    /// Missing offscreen rows never borrow stale bounds from an earlier paint.
    pub rows: Vec<RowGeometry>,
    pub root: Option<RootGeometry>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DropMarker {
    Line {
        start: Point<Pixels>,
        end: Point<Pixels>,
    },
    Into(Bounds<Pixels>),
    Root(Bounds<Pixels>),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DropPreview {
    pub plan: DropPlan,
    pub marker: DropMarker,
}

impl Geometry {
    pub fn matches_visible(&self, visible: &[(usize, u64, u64)]) -> bool {
        self.visible == visible
    }

    /// The same resolver serves hover and final release. A structural target is
    /// never returned unless its actual marker/highlight is currently drawable.
    pub fn resolve(
        &self,
        contents: &ShapeContents,
        selection: &Selection,
        position: Point<Pixels>,
    ) -> Option<DropPreview> {
        self.valid_source(contents).then_some(())?;
        let (target, marker) = self.hit(position)?;
        Some(DropPreview {
            plan: plan_drop(contents, selection, target)?,
            marker,
        })
    }

    fn valid_source(&self, contents: &ShapeContents) -> bool {
        let Some(source) = bounded_rows(contents) else {
            return false;
        };
        // A logical visible sequence may omit complete collapsed subtrees, but
        // never an arbitrary sibling, first child or half of a collapsed subtree.
        let mut next = 0;
        let mut hidden_depth = None;
        for (depth, parent, node) in &source {
            if hidden_depth.is_some_and(|hidden| *depth > hidden) {
                continue;
            }
            hidden_depth = None;
            if self.visible.get(next) != Some(&(*depth, *parent, node.id)) {
                return false;
            }
            next += 1;
            if matches!(node.kind, ContentsKind::Group(_))
                && self.visible.get(next).is_none_or(|(d, _, _)| d <= depth)
            {
                hidden_depth = Some(*depth);
            }
        }
        if next != self.visible.len() {
            return false;
        }
        let mut previous_index = None;
        let mut previous_bottom = None;
        for row in &self.rows {
            let Some(index) = self
                .visible
                .iter()
                .position(|entry| *entry == (row.depth, row.parent, row.item))
            else {
                return false;
            };
            if previous_index.is_some_and(|previous| previous >= index)
                || previous_bottom.is_some_and(|bottom| bottom > row.bounds.top())
                || !positive(row.bounds)
                || !positive(row.label)
                || !finite(row.clip)
                || row.label.left() < row.bounds.left()
                || row.label.right() > row.bounds.right()
                || row.label.top() < row.bounds.top()
                || row.label.bottom() > row.bounds.bottom()
                || !source.iter().any(|(_, _, node)| {
                    node.id == row.item && matches!(node.kind, ContentsKind::Group(_)) == row.group
                })
            {
                return false;
            }
            previous_index = Some(index);
            previous_bottom = Some(row.bounds.bottom());
        }
        self.root.as_ref().is_none_or(|root| {
            positive(root.bounds)
                && finite(root.clip)
                && previous_bottom.is_none_or(|bottom| bottom <= root.bounds.top())
        })
    }

    fn hit(&self, position: Point<Pixels>) -> Option<(DropTarget, DropMarker)> {
        // Tree order is intentional; numeric IDs carry no visual ordering.
        for row in &self.rows {
            let Some(visible) = intersection(row.label, row.clip) else {
                continue;
            };
            if !half_open(visible, position) {
                continue;
            }
            if row.label.size.height < px(12.) {
                return None;
            }
            let top_end = row.label.top() + px(6.);
            let bottom_start = row.label.bottom() - px(6.);
            if position.y < top_end {
                return Some((
                    DropTarget::Before(row.item),
                    line(row, row.bounds.top(), row.clip)?,
                ));
            }
            if position.y >= bottom_start {
                let logical_index = self.visible.iter().position(|(_, _, id)| *id == row.item)?;
                let endpoint = self.visible[logical_index + 1..]
                    .iter()
                    .take_while(|(depth, _, _)| *depth > row.depth)
                    .last()
                    .map(|(_, _, id)| *id)
                    .unwrap_or(row.item);
                // Expanded After belongs at the logical subtree end. If that
                // row has no current registration, there is no visible target.
                let endpoint = self.rows.iter().find(|row| row.item == endpoint)?;
                return Some((
                    DropTarget::After(row.item),
                    line(row, endpoint.bounds.bottom(), endpoint.clip)?,
                ));
            }
            if !row.group {
                return None;
            }
            let center = Bounds::new(
                point(row.label.left() + px(1.), top_end + px(1.)),
                size(
                    row.label.size.width - px(2.),
                    bottom_start - top_end - px(2.),
                ),
            );
            return Some((
                DropTarget::Into(row.item),
                DropMarker::Into(intersection(center, row.clip)?),
            ));
        }
        let root = self.root.as_ref()?;
        let visible = intersection(root.bounds, root.clip)?;
        half_open(visible, position).then_some((DropTarget::RootEnd, DropMarker::Root(visible)))
    }
}

fn finite(rect: Bounds<Pixels>) -> bool {
    [rect.left(), rect.top(), rect.right(), rect.bottom()]
        .into_iter()
        .all(|v| f32::from(v).is_finite())
}
fn positive(rect: Bounds<Pixels>) -> bool {
    finite(rect) && rect.size.width > px(0.) && rect.size.height > px(0.)
}
fn intersection(a: Bounds<Pixels>, b: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    if !positive(a) || !positive(b) {
        return None;
    }
    let left = a.left().max(b.left());
    let top = a.top().max(b.top());
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    let result = Bounds::new(point(left, top), size(right - left, bottom - top));
    positive(result).then_some(result)
}
fn half_open(rect: Bounds<Pixels>, position: Point<Pixels>) -> bool {
    position.x >= rect.left()
        && position.x < rect.right()
        && position.y >= rect.top()
        && position.y < rect.bottom()
}
fn line(row: &RowGeometry, y: Pixels, endpoint_clip: Bounds<Pixels>) -> Option<DropMarker> {
    let clip = intersection(row.clip, endpoint_clip)?;
    // Never clamp an offscreen edge to the viewport boundary.
    if y < clip.top() || y >= clip.bottom() {
        return None;
    }
    let left = row.label.left().max(clip.left());
    let right = row.label.right().min(clip.right());
    (right > left).then_some(DropMarker::Line {
        start: point(left, y),
        end: point(right, y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::contents::tree_selection::visible_rows;
    use libre_effects_core::ContentsNode;
    use std::collections::BTreeSet;

    fn node(id: u64, children: Option<Vec<ContentsNode>>) -> ContentsNode {
        ContentsNode {
            id,
            name: format!("Item {id}"),
            enabled: true,
            kind: children
                .map(ContentsKind::Group)
                .unwrap_or(ContentsKind::Fill { even_odd: false }),
            composite: Default::default(),
            blend: Default::default(),
            parameters: Default::default(),
        }
    }
    fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }
    fn fixture() -> (ShapeContents, Selection, Geometry) {
        let mut contents = ShapeContents::default();
        contents.items = vec![
            node(
                90,
                Some(vec![node(60, None), node(5, Some(vec![node(83, None)]))]),
            ),
            node(7, None),
            node(3, Some(vec![])),
            node(94, None),
        ];
        contents.items[0].enabled = false;
        let mut selection = Selection::default();
        selection.all(0, &[94, 7]);
        let geometry = layout(&contents, &BTreeSet::new());
        (contents, selection, geometry)
    }
    fn layout(contents: &ShapeContents, collapsed: &BTreeSet<u64>) -> Geometry {
        let visible = visible_rows(contents, collapsed);
        let rows = visible
            .iter()
            .enumerate()
            .map(|(index, &(depth, parent, item))| {
                let x = 24. + depth as f32 * 16.;
                RowGeometry {
                    item,
                    parent,
                    depth,
                    group: matches!(contents.node(item).unwrap().kind, ContentsKind::Group(_)),
                    bounds: rect(0., index as f32 * 26., 200., 26.),
                    label: rect(x, index as f32 * 26., 200. - x, 26.),
                    clip: rect(0., 0., 220., 240.),
                }
            })
            .collect();
        let root = Some(RootGeometry {
            bounds: rect(0., visible.len() as f32 * 26., 200., 26.),
            clip: rect(0., 0., 220., 240.),
        });
        Geometry {
            owner: (1, 4),
            visible,
            rows,
            root,
        }
    }
    fn at(
        geometry: &Geometry,
        contents: &ShapeContents,
        selection: &Selection,
        x: f32,
        y: f32,
    ) -> Option<DropPreview> {
        geometry.resolve(contents, selection, point(px(x), px(y)))
    }
    fn mask(geometry: &mut Geometry, clip: Bounds<Pixels>) {
        for row in &mut geometry.rows {
            row.clip = clip;
        }
        if let Some(root) = &mut geometry.root {
            root.clip = clip;
        }
    }

    #[test]
    fn drop_geometry_bands_are_full_label_half_open_and_exclude_controls() {
        let (contents, selection, geometry) = fixture();
        for (y, target) in [
            (0., DropTarget::Before(90)),
            (5.999, DropTarget::Before(90)),
            (6., DropTarget::Into(90)),
            (19.999, DropTarget::Into(90)),
            (20., DropTarget::After(90)),
            (25.999, DropTarget::After(90)),
            (26., DropTarget::Before(60)),
            (31.999, DropTarget::Before(60)),
            (46., DropTarget::After(60)),
            (51.999, DropTarget::After(60)),
            (52., DropTarget::Before(5)),
        ] {
            assert_eq!(
                at(&geometry, &contents, &selection, 60., y)
                    .unwrap()
                    .plan
                    .target,
                target
            );
        }
        for (x, y) in [
            (23.999, 3.),
            (10., 14.),
            (39.999, 29.),
            (60., 32.),
            (60., 45.999),
            (200., 3.),
            (60., -0.001),
            (60., 208.),
        ] {
            assert!(
                at(&geometry, &contents, &selection, x, y).is_none(),
                "{x}, {y}"
            );
        }
        assert_eq!(
            at(&geometry, &contents, &selection, 24., 3.)
                .unwrap()
                .plan
                .target,
            DropTarget::Before(90)
        );
    }
    #[test]
    fn drop_geometry_after_expanded_group_marks_logical_subtree_at_parent_indent() {
        let (contents, selection, geometry) = fixture();
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 24.)
                .unwrap()
                .marker,
            DropMarker::Line {
                start: point(px(24.), px(104.)),
                end: point(px(200.), px(104.))
            }
        );
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 74.)
                .unwrap()
                .marker,
            DropMarker::Line {
                start: point(px(40.), px(104.)),
                end: point(px(200.), px(104.))
            }
        );
        let collapsed = layout(&contents, &[90].into());
        assert_eq!(
            at(&collapsed, &contents, &selection, 60., 24.)
                .unwrap()
                .marker,
            DropMarker::Line {
                start: point(px(24.), px(26.)),
                end: point(px(200.), px(26.))
            }
        );
        assert_eq!(
            at(&collapsed, &contents, &selection, 60., 14.)
                .unwrap()
                .plan
                .target,
            DropTarget::Into(90)
        );
        let empty = at(&geometry, &contents, &selection, 60., 144.).unwrap();
        assert_eq!(empty.plan.target, DropTarget::Into(3));
        assert_eq!((empty.plan.parent, empty.plan.index), (3, 0));
        assert!(matches!(empty.marker, DropMarker::Into(_)));
    }
    #[test]
    fn drop_geometry_root_landing_is_dedicated_stable_and_stays_root_after_nested_row() {
        let (mut contents, mut selection, _) = fixture();
        contents.items.swap(0, 3); // Expanded group is now the last root sibling.
        let geometry = layout(&contents, &BTreeSet::new());
        selection.one(90, 83); // This is stale: 83 belongs to nested group 5.
        assert!(at(&geometry, &contents, &selection, 20., 190.).is_none());
        selection.one(5, 83);
        let preview = at(&geometry, &contents, &selection, 20., 190.).unwrap();
        assert_eq!(preview.plan.target, DropTarget::RootEnd);
        assert_eq!((preview.plan.parent, preview.plan.index), (0, 4));
        assert_eq!(preview.marker, DropMarker::Root(rect(0., 182., 200., 26.)));
        assert!(at(&geometry, &contents, &selection, 201., 190.).is_none());
        assert!(at(&geometry, &contents, &selection, 20., 208.).is_none());
        let mut absent = geometry.clone();
        absent.root = None;
        assert!(at(&absent, &contents, &selection, 20., 190.).is_none());
        assert_eq!(geometry, geometry.clone());
    }
    #[test]
    fn drop_geometry_partial_clip_never_repartitions_bands_or_clamps_markers() {
        let (contents, selection, mut geometry) = fixture();
        mask(&mut geometry, rect(0., 3., 220., 237.));
        assert!(at(&geometry, &contents, &selection, 60., 4.).is_none()); // Before's real edge is hidden.
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 6.)
                .unwrap()
                .plan
                .target,
            DropTarget::Into(90)
        );
        mask(&mut geometry, rect(0., 0., 220., 104.));
        assert!(at(&geometry, &contents, &selection, 60., 24.).is_none()); // End is exactly clip bottom.
        mask(&mut geometry, rect(0., 0., 220., 104.01));
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 24.)
                .unwrap()
                .plan
                .target,
            DropTarget::After(90)
        );
        mask(&mut geometry, rect(0., 0., 220., 23.));
        assert!(at(&geometry, &contents, &selection, 60., 21.).is_none());
        mask(&mut geometry, rect(0., 28., 220., 212.));
        // A fully clipped first registration must not hide later eligible rows.
        assert!(at(&geometry, &contents, &selection, 60., 29.).is_none());
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 49.)
                .unwrap()
                .plan
                .target,
            DropTarget::After(60)
        );
    }
    #[test]
    fn drop_geometry_horizontal_clip_requires_positive_drawable_span() {
        let (contents, selection, mut geometry) = fixture();
        mask(&mut geometry, rect(80., 0., 70., 240.));
        let preview = at(&geometry, &contents, &selection, 80., 3.).unwrap();
        assert_eq!(
            preview.marker,
            DropMarker::Line {
                start: point(px(80.), px(0.)),
                end: point(px(150.), px(0.))
            }
        );
        assert!(at(&geometry, &contents, &selection, 79.999, 3.).is_none());
        assert!(at(&geometry, &contents, &selection, 150., 3.).is_none());
        mask(&mut geometry, rect(200., 0., 20., 240.));
        assert!(at(&geometry, &contents, &selection, 200., 3.).is_none());
        mask(&mut geometry, rect(50., 0., 0., 240.));
        assert!(at(&geometry, &contents, &selection, 50., 3.).is_none());
        mask(&mut geometry, rect(50., 0., -10., 240.));
        assert!(at(&geometry, &contents, &selection, 50., 3.).is_none());
    }
    #[test]
    fn drop_geometry_missing_offscreen_rows_only_suppress_targets_needing_their_bounds() {
        let (contents, selection, mut geometry) = fixture();
        geometry
            .rows
            .retain(|row| ![60, 83, 7, 94].contains(&row.item));
        assert!(geometry.valid_source(&contents));
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 14.)
                .unwrap()
                .plan
                .target,
            DropTarget::Into(90)
        );
        assert!(at(&geometry, &contents, &selection, 60., 24.).is_none());
        assert!(at(&geometry, &contents, &selection, 60., 74.).is_none());
        assert_eq!(
            at(&geometry, &contents, &selection, 60., 144.)
                .unwrap()
                .plan
                .items,
            vec![7, 94]
        );
        assert_eq!(
            at(&geometry, &contents, &selection, 20., 190.)
                .unwrap()
                .plan
                .target,
            DropTarget::RootEnd
        );
    }
    #[test]
    fn drop_geometry_rejects_stale_metadata_duplicate_rows_and_numeric_id_order() {
        let (contents, selection, geometry) = fixture();
        assert!(geometry.matches_visible(&visible_rows(&contents, &BTreeSet::new())));
        assert!(!geometry.matches_visible(&visible_rows(&contents, &[90].into())));
        let mut stale = geometry.clone();
        stale.rows.sort_by_key(|row| row.item);
        assert!(at(&stale, &contents, &selection, 60., 14.).is_none());
        stale = geometry.clone();
        stale.rows.insert(0, stale.rows[0].clone());
        assert!(at(&stale, &contents, &selection, 60., 14.).is_none());
        stale = geometry.clone();
        stale.visible.remove(1); // Cannot hide just the first child of an expanded group.
        stale.rows.retain(|row| row.item != 60);
        assert!(at(&stale, &contents, &selection, 60., 14.).is_none());
        stale = geometry.clone();
        stale.rows[0].group = false;
        assert!(at(&stale, &contents, &selection, 60., 14.).is_none());
        stale = geometry.clone();
        stale.rows[0].parent = 3;
        assert!(at(&stale, &contents, &selection, 60., 14.).is_none());
        let mut replaced = contents.clone();
        replaced.items.swap(1, 3);
        assert!(at(&geometry, &replaced, &selection, 60., 14.).is_none());
    }
    #[test]
    fn drop_geometry_signature_changes_with_owner_scroll_layout_clip_or_visibility() {
        let (contents, _, geometry) = fixture();
        assert_eq!(geometry, layout(&contents, &BTreeSet::new()));
        let mut changed = geometry.clone();
        changed.owner.1 += 1;
        assert_ne!(geometry, changed);
        changed = geometry.clone();
        changed.rows[0].bounds.origin.y += px(1.);
        assert_ne!(geometry, changed);
        changed = geometry.clone();
        changed.rows[0].label.size.width -= px(1.);
        assert_ne!(geometry, changed);
        changed = geometry.clone();
        changed.rows[0].clip.size.height -= px(1.);
        assert_ne!(geometry, changed);
        changed = geometry.clone();
        changed.root.as_mut().unwrap().bounds.origin.y += px(1.);
        assert_ne!(geometry, changed);
        assert_ne!(geometry, layout(&contents, &[90].into()));
    }
    #[test]
    fn drop_geometry_resolver_has_marker_plan_agreement_without_hover_mutation() {
        let (contents, selection, geometry) = fixture();
        let before = (contents.clone(), selection.clone(), geometry.clone());
        for (x, y) in [(60., 3.), (60., 14.), (60., 24.), (60., 144.), (20., 190.)] {
            let first = at(&geometry, &contents, &selection, x, y).unwrap();
            assert_eq!(
                Some(first.plan.clone()),
                plan_drop(&contents, &selection, first.plan.target)
            );
            assert_eq!(Some(first), at(&geometry, &contents, &selection, x, y));
        }
        assert_eq!((contents, selection, geometry), before);
    }
    #[test]
    fn drop_geometry_empty_source_has_only_root_hit_and_no_fabricated_source_plan() {
        let contents = ShapeContents::default();
        let geometry = layout(&contents, &BTreeSet::new());
        assert!(geometry.valid_source(&contents));
        assert_eq!(
            geometry.hit(point(px(1.), px(1.))).unwrap().0,
            DropTarget::RootEnd
        );
        assert!(at(&geometry, &contents, &Selection::default(), 1., 1.).is_none());
        let absent = Geometry {
            root: None,
            ..geometry
        };
        assert!(absent.hit(point(px(1.), px(1.))).is_none());
    }
}
