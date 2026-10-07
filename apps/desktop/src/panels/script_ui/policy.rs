//! Pure policies shared by the native ScriptUI view and its lightweight tests.

#[derive(Default)]
pub(crate) struct FocusRequests {
    seen: u64,
}

impl FocusRequests {
    /// Consume the latest assignment, including a request whose target became
    /// hidden, disabled or inactive. Re-enabling it must not replay stale intent.
    pub(crate) fn take(
        &mut self,
        requests: impl IntoIterator<Item = (u64, u64, bool)>,
    ) -> Option<u64> {
        let (serial, id, eligible) = requests.into_iter().max_by_key(|request| request.0)?;
        if serial <= self.seen {
            return None;
        }
        self.seen = serial;
        eligible.then_some(id)
    }
}

/// Minimums win over preferred sizes. The viewport wins over both; text fields
/// scroll internally and the dialog body scrolls to the remaining controls.
fn extent(minimum: Option<f64>, preferred: Option<f64>, default: f32, available: f32) -> f32 {
    (preferred.unwrap_or(default as f64) as f32)
        .max(minimum.unwrap_or(0.0) as f32)
        .max(1.0)
        .min(available.max(1.0))
}

/// The smallest body scroll that reveals keyboard focus. Already visible
/// controls must not jump to the top and hide the dialog's guide/header.
pub(crate) fn reveal_delta(view_top: f32, view_bottom: f32, top: f32, bottom: f32) -> f32 {
    if top < view_top || bottom - top > view_bottom - view_top {
        view_top - top
    } else if bottom > view_bottom {
        view_bottom - bottom
    } else {
        0.0
    }
}

#[derive(Debug)]
pub(crate) struct ControlSize {
    pub width: Option<f32>,
    pub height: f32,
}

#[derive(Debug, PartialEq)]
pub(crate) enum EditWidth {
    Fill,
    Flexible { minimum: f32 },
    Requested(f32),
}

impl ControlSize {
    pub fn edit_width(&self, parent_row: bool, available: f32) -> EditWidth {
        match self.width {
            Some(width) => EditWidth::Requested(width),
            None if parent_row => EditWidth::Flexible {
                minimum: 160.0_f32.min(available),
            },
            None => EditWidth::Fill,
        }
    }
    pub fn new(
        minimum: Option<[f64; 2]>,
        preferred: Option<[f64; 2]>,
        default_height: f32,
        available: [f32; 2],
    ) -> Self {
        Self {
            width: (minimum.is_some_and(|size| size[0] > 0.0) || preferred.is_some()).then(|| {
                extent(
                    minimum.map(|size| size[0]),
                    preferred.map(|size| size[0]),
                    0.0,
                    available[0],
                )
            }),
            height: extent(
                minimum.map(|size| size[1]),
                preferred.map(|size| size[1]),
                default_height,
                available[1],
            ),
        }
    }
}

#[derive(Debug)]
pub(crate) struct DialogSize {
    pub width: f32,
    pub minimum_height: f32,
    pub height: Option<f32>,
    pub maximum_height: f32,
    pub body_height: f32,
    pub controls: [f32; 2],
}

impl DialogSize {
    /// Extra native chrome is measured at the final dialog width, including
    /// wrapped fallback-keyboard guidance and the additional inter-item gap.
    pub fn reserve_notice(&mut self, height: f32) {
        self.body_height = (self.body_height - height).max(1.0);
        self.controls[1] = (self.body_height - 8.0).max(1.0);
    }
    pub fn new(minimum: Option<[f64; 2]>, preferred: Option<[f64; 2]>, viewport: [f32; 2]) -> Self {
        let width = extent(
            minimum.map(|size| size[0]),
            preferred.map(|size| size[0]),
            800.0,
            viewport[0] - 48.0,
        );
        let maximum_height = (viewport[1] - 36.0).max(1.0);
        let minimum_height = extent(minimum.map(|size| size[1]), None, 1.0, maximum_height);
        let height = preferred.map(|size| {
            extent(
                minimum.map(|size| size[1]),
                Some(size[1]),
                1.0,
                maximum_height,
            )
        });
        let body_height = (height.unwrap_or(maximum_height) - 154.0).max(1.0);
        Self {
            width,
            minimum_height,
            height,
            maximum_height,
            body_height,
            // Dialog padding/border and body padding; reserve the title,
            // compatibility notice, optional keyboard warning, gaps and footer.
            controls: [(width - 42.0).max(1.0), (body_height - 8.0).max(1.0)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_assignment_is_consumed_once_across_revisions_and_nested_messages() {
        let mut focus = FocusRequests::default();
        assert_eq!(focus.take([(1, 20, true)]), Some(20));
        assert_eq!(focus.take([(1, 20, true)]), None); // text acknowledgement
        assert_eq!(focus.take([]), None); // nested confirmation
        assert_eq!(focus.take([(1, 20, true)]), None); // restored parent
        assert_eq!(focus.take([(2, 20, true)]), Some(20)); // sample/clear
        assert_eq!(focus.take([(2, 20, true)]), None);
    }

    #[test]
    fn latest_assignment_wins_independently_of_tree_order() {
        let mut focus = FocusRequests::default();
        assert_eq!(focus.take([(3, 20, true), (2, 30, true)]), Some(20));
        assert_eq!(focus.take([(3, 20, true), (4, 30, true)]), Some(30));
        assert_eq!(focus.take([(3, 20, true)]), None);
    }

    #[test]
    fn retired_target_is_consumed_without_replaying_an_older_request() {
        let mut focus = FocusRequests::default();
        assert_eq!(focus.take([(1, 20, true), (2, 30, false)]), None);
        assert_eq!(focus.take([(1, 20, true), (2, 30, true)]), None);
        assert_eq!(focus.take([(1, 20, true), (3, 30, true)]), Some(30));
    }

    #[test]
    fn requested_dialog_and_child_sizes_fit_an_ordinary_viewport() {
        let dialog = DialogSize::new(Some([720.0, 620.0]), None, [1280.0, 900.0]);
        assert_eq!(dialog.width, 800.0);
        assert_eq!(dialog.minimum_height, 620.0);
        assert_eq!(dialog.height, None);
        let input = ControlSize::new(Some([680.0, 350.0]), None, 112.0, dialog.controls);
        assert_eq!(input.width, Some(680.0));
        assert_eq!(input.height, 350.0);
        let button = ControlSize::new(None, Some([150.0, 34.0]), 25.0, dialog.controls);
        assert_eq!(button.width, Some(150.0));
        assert_eq!(button.height, 34.0);
    }

    #[test]
    fn reduced_viewport_bounds_the_dialog_and_edit_surface() {
        let dialog = DialogSize::new(Some([720.0, 620.0]), None, [640.0, 480.0]);
        assert_eq!(dialog.width, 592.0);
        assert_eq!(dialog.minimum_height, 444.0);
        assert_eq!(dialog.maximum_height, 444.0);
        let input = ControlSize::new(Some([680.0, 350.0]), None, 112.0, dialog.controls);
        assert_eq!(input.width, Some(550.0));
        assert_eq!(dialog.body_height, 290.0);
        assert_eq!(input.height, 282.0);
        // A capped input may fill the body. Its following buttons stay in that
        // body's scroll content instead of being compressed to zero height.
        assert!(input.height + 34.0 > dialog.controls[1]);
    }

    #[test]
    fn minimum_overrides_preference_and_preference_overrides_defaults() {
        let dialog = DialogSize::new(Some([720.0, 620.0]), Some([900.0, 700.0]), [1400.0, 1000.0]);
        assert_eq!(dialog.width, 900.0);
        assert_eq!(dialog.height, Some(700.0));
        let input = ControlSize::new(
            Some([680.0, 350.0]),
            Some([300.0, 100.0]),
            112.0,
            dialog.controls,
        );
        assert_eq!(input.width, Some(680.0));
        assert_eq!(input.height, 350.0);
        let button = ControlSize::new(None, None, 25.0, dialog.controls);
        assert_eq!(button.width, None);
        assert_eq!(button.height, 25.0);
    }

    #[test]
    fn huge_hints_and_tiny_viewports_never_escape_available_bounds() {
        let dialog = DialogSize::new(
            Some([4096.0, 4096.0]),
            Some([4096.0, 4096.0]),
            [320.0, 240.0],
        );
        assert_eq!(dialog.width, 272.0);
        assert_eq!(dialog.height, Some(204.0));
        let input = ControlSize::new(Some([4096.0, 4096.0]), None, 112.0, dialog.controls);
        assert_eq!(input.width, Some(230.0));
        assert_eq!(input.height, 42.0);
    }

    #[test]
    fn keyboard_reveal_preserves_visible_content_and_reaches_offscreen_buttons() {
        assert_eq!(reveal_delta(100.0, 390.0, 120.0, 350.0), 0.0);
        assert_eq!(reveal_delta(100.0, 390.0, 470.0, 504.0), -114.0);
        assert_eq!(reveal_delta(100.0, 390.0, 70.0, 104.0), 30.0);
        // A field larger than the final layout gets its leading edge revealed.
        assert_eq!(reveal_delta(100.0, 390.0, 140.0, 500.0), -40.0);
    }

    #[test]
    fn unhinted_row_fields_share_space_and_hinted_fields_keep_requested_width() {
        let ordinary = ControlSize::new(None, None, 24.0, [550.0, 282.0]);
        assert_eq!(
            ordinary.edit_width(true, 550.0),
            EditWidth::Flexible { minimum: 160.0 }
        );
        assert_eq!(ordinary.edit_width(false, 550.0), EditWidth::Fill);
        assert_eq!(
            ordinary.edit_width(true, 120.0),
            EditWidth::Flexible { minimum: 120.0 }
        );
        let hinted = ControlSize::new(None, Some([200.0, 34.0]), 24.0, [550.0, 282.0]);
        assert_eq!(hinted.edit_width(true, 550.0), EditWidth::Requested(200.0));
    }

    #[test]
    fn wrapped_keyboard_warning_reduces_both_body_and_input_budget() {
        let mut dialog = DialogSize::new(Some([720.0, 620.0]), None, [640.0, 480.0]);
        dialog.reserve_notice(44.0); // two 16px lines and an extra 12px gap
        assert_eq!(dialog.body_height, 246.0);
        let input = ControlSize::new(Some([680.0, 350.0]), None, 112.0, dialog.controls);
        assert_eq!(input.height, 238.0);
        assert!(input.height + 8.0 <= dialog.body_height);
        dialog.reserve_notice(400.0);
        assert_eq!(dialog.body_height, 1.0);
        assert_eq!(dialog.controls[1], 1.0);
    }
}
