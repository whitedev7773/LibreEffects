//! Bounded native/VM modal handshake. Native edits can arrive faster than a VM
//! callback; nested alerts temporarily suspend, rather than consume, those edits.
use crate::automation::{UiNode, UiRequest, UiResponse};
use std::collections::VecDeque;

#[derive(Default)]
pub struct UiSession {
    pub revision: u64,
    pub request: Option<UiRequest>,
    pub waiting: bool,
    pub action_pending: bool,
    queued: VecDeque<UiResponse>,
}
impl UiSession {
    pub fn inputs_acknowledged(&self) -> bool {
        self.waiting && self.queued.is_empty()
    }
    pub fn matches(&self, response: &UiResponse) -> bool {
        match (&self.request, response) {
            (
                Some(UiRequest::Dialog { id, root, .. }),
                UiResponse::Click {
                    dialog_id,
                    control_id,
                },
            ) => {
                id == dialog_id
                    && active_control(root, *control_id).is_some_and(|node| node.kind == "button")
            }
            (
                Some(UiRequest::Dialog { id, root, .. }),
                UiResponse::Change {
                    dialog_id,
                    control_id,
                    text,
                },
            ) => {
                id == dialog_id
                    && text.len() <= 16384
                    && active_control(root, *control_id).is_some_and(|node| node.kind == "edittext")
            }
            (
                Some(UiRequest::Dialog { id, .. }),
                UiResponse::Close { dialog_id } | UiResponse::Resize { dialog_id },
            ) => id == dialog_id,
            (Some(UiRequest::Alert { .. }), UiResponse::AlertDismissed)
            | (Some(UiRequest::Confirm { .. }), UiResponse::Confirm { .. }) => true,
            _ => false,
        }
    }
    fn sending(&mut self, response: UiResponse) -> Option<UiResponse> {
        if !matches!(response, UiResponse::Change { .. }) {
            self.action_pending = true;
        }
        self.waiting = false;
        Some(response)
    }
    pub fn receive(&mut self, request: UiRequest) -> Option<UiResponse> {
        self.revision = self.revision.wrapping_add(1);
        self.request = Some(request);
        self.waiting = true;
        if matches!(self.request, Some(UiRequest::Dialog { .. })) {
            // Retirement is source-observed: a callback may hide/disable/remove
            // controls or open a different dialog before queued input returns.
            let queue = std::mem::take(&mut self.queued);
            self.queued = queue
                .into_iter()
                .filter(|response| self.matches(response))
                .collect();
        }
        self.action_pending = self.queued.iter().any(|response| {
            !matches!(response, UiResponse::Change { .. }) && self.matches(response)
        });
        if self
            .queued
            .front()
            .is_some_and(|response| self.matches(response))
        {
            let response = self.queued.pop_front().unwrap();
            self.sending(response)
        } else {
            None
        }
    }
    pub fn respond(&mut self, response: UiResponse) -> Result<Option<UiResponse>, String> {
        if !self.matches(&response) {
            return Ok(None);
        }
        if !matches!(response, UiResponse::Change { .. }) {
            if self.action_pending {
                return Ok(None);
            }
            self.action_pending = true;
        }
        if self.waiting
            && self
                .queued
                .front()
                .is_none_or(|queued| !self.matches(queued))
        {
            return Ok(self.sending(response));
        }
        if self.queued.len() >= 64 {
            return Err("ScriptUI input queue exceeded 64 events".into());
        }
        self.queued.push_back(response);
        Ok(None)
    }
}
pub fn active_control(node: &UiNode, id: u64) -> Option<&UiNode> {
    if !node.enabled || !node.visible {
        return None;
    }
    if node.id == id {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| active_control(child, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: u64, kind: &str) -> UiNode {
        UiNode {
            id,
            kind: kind.into(),
            text: String::new(),
            enabled: true,
            visible: true,
            active: false,
            focus_request: 0,
            multiline: false,
            orientation: "column".into(),
            children: vec![],
            minimum_size: None,
            preferred_size: None,
            help_tip: String::new(),
        }
    }
    fn dialog(id: u64) -> UiRequest {
        let mut root = node(id, "dialog");
        root.children = vec![node(2, "edittext"), node(3, "button")];
        UiRequest::Dialog {
            id,
            title: "Test".into(),
            root,
            default_element: Some(3),
            cancel_element: None,
            resizable: false,
        }
    }
    fn change(text: &str) -> UiResponse {
        UiResponse::Change {
            dialog_id: 1,
            control_id: 2,
            text: text.into(),
        }
    }
    fn click() -> UiResponse {
        UiResponse::Click {
            dialog_id: 1,
            control_id: 3,
        }
    }
    #[test]
    fn nested_alert_bypasses_edit_queue_and_pending_click() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        assert!(ui.respond(change("a")).unwrap().is_some());
        assert!(ui.respond(change("ab")).unwrap().is_none());
        assert!(ui.respond(click()).unwrap().is_none());
        assert!(
            ui.receive(UiRequest::Alert {
                message: "Wait".into()
            })
            .is_none()
        );
        assert!(!ui.action_pending);
        assert!(matches!(
            ui.respond(UiResponse::AlertDismissed).unwrap(),
            Some(UiResponse::AlertDismissed)
        ));
        assert!(matches!(ui.receive(dialog(1)),Some(UiResponse::Change {text,..}) if text=="ab"));
        assert!(ui.action_pending);
        assert!(matches!(
            ui.receive(dialog(1)),
            Some(UiResponse::Click { .. })
        ));
        ui.receive(dialog(1));
        assert!(!ui.action_pending);
        assert!(ui.inputs_acknowledged());
    }
    #[test]
    fn confirm_bypasses_parent_changes_without_reordering_them() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        ui.respond(change("a")).unwrap();
        ui.respond(change("b")).unwrap();
        ui.receive(UiRequest::Confirm {
            message: "Continue?".into(),
        });
        assert!(matches!(
            ui.respond(UiResponse::Confirm { value: false }).unwrap(),
            Some(UiResponse::Confirm { value: false })
        ));
        assert!(matches!(ui.receive(dialog(1)),Some(UiResponse::Change {text,..}) if text=="b"));
    }
    #[test]
    fn new_dialog_retires_old_events_and_accepts_input() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        ui.respond(change("a")).unwrap();
        ui.respond(click()).unwrap();
        assert!(ui.receive(dialog(9)).is_none());
        assert!(ui.inputs_acknowledged());
        assert!(!ui.action_pending);
        assert!(
            ui.respond(UiResponse::Click {
                dialog_id: 9,
                control_id: 3
            })
            .unwrap()
            .is_some()
        );
    }
    #[test]
    fn hidden_or_disabled_controls_retire_queued_input() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        ui.respond(change("a")).unwrap();
        ui.respond(change("b")).unwrap();
        let mut next = dialog(1);
        if let UiRequest::Dialog { root, .. } = &mut next {
            root.children[0].enabled = false;
        }
        assert!(ui.receive(next).is_none());
        assert!(ui.inputs_acknowledged());
        assert!(ui.respond(change("c")).unwrap().is_none());
    }
    #[test]
    fn repeated_buttons_and_stale_ids_never_submit_twice() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        assert!(ui.respond(click()).unwrap().is_some());
        assert!(ui.respond(click()).unwrap().is_none());
        assert!(
            ui.respond(UiResponse::Click {
                dialog_id: 99,
                control_id: 3
            })
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn queue_is_bounded_and_disabled_ancestor_blocks_default() {
        let mut ui = UiSession::default();
        ui.receive(dialog(1));
        ui.respond(change("a")).unwrap();
        for _ in 0..64 {
            ui.respond(change("a")).unwrap();
        }
        assert!(ui.respond(change("a")).is_err());
        let mut root = node(1, "dialog");
        root.children.push(node(3, "button"));
        root.enabled = false;
        assert!(active_control(&root, 3).is_none());
    }
}

/// Activation-key ownership survives native modal transitions. GPUI's X11
/// repeat flag alone is insufficient; reliable release semantics must first be
/// negotiated on the client connection. Unsupported/error cases use pointer-only
/// policy. Text fields retain normal repeat insertion independently of this latch.
#[derive(Default)]
pub struct ActivationKeys {
    down: u8,
    pointer_only: bool,
}

/// Receipt from the workspace's current key-down dispatch. A focused control
/// must not infer a fresh press from a later key-up or from newly valid state.
#[derive(Clone, Copy, Debug)]
pub struct ActivationPress {
    mask: u8,
    first: bool,
    allowed: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusedToggleAction {
    Pass,
    Consume,
    Activate,
}
impl ActivationPress {
    pub fn allowed(self) -> bool {
        self.allowed
    }
    /// Enter/Space belong to a focused toggle. The first unmodified Space while
    /// playing is the sole exception: transport owns that complete press.
    pub fn focused_toggle(
        self,
        modified: bool,
        window_active: bool,
        playing: bool,
        eligible: bool,
    ) -> FocusedToggleAction {
        use FocusedToggleAction::*;
        if !matches!(self.mask, 1 | 2) {
            return Pass;
        }
        if modified || !window_active {
            return Consume;
        }
        if playing {
            return if self.mask == 2 && self.first {
                Pass
            } else {
                Consume
            };
        }
        if eligible && self.allowed {
            Activate
        } else {
            Consume
        }
    }
}
impl ActivationKeys {
    /// A request/reply error has exactly the same safe policy as unsupported.
    pub fn from_release_negotiation(result: Result<bool, ()>) -> Self {
        if result == Ok(true) {
            Self::default()
        } else {
            Self::pointer_only()
        }
    }
    /// Unsupported release semantics must not unlock keyboard modal actions.
    /// Text insertion and navigation are owned separately by native controls.
    pub fn pointer_only() -> Self {
        Self {
            down: 0,
            pointer_only: true,
        }
    }
    pub fn press(&mut self, key: &str, is_held: bool) -> bool {
        self.press_receipt(key, is_held).allowed()
    }
    pub fn press_receipt(&mut self, key: &str, is_held: bool) -> ActivationPress {
        let mask = match key {
            "enter" => 1,
            "space" => 2,
            "escape" => 4,
            _ => 0,
        };
        let first = mask != 0 && self.down & mask == 0 && !is_held;
        self.down |= mask;
        ActivationPress {
            mask,
            first,
            allowed: first && !self.pointer_only,
        }
    }
    pub fn release(&mut self, key: &str) {
        let mask = match key {
            "enter" => 1,
            "space" => 2,
            "escape" => 4,
            _ => return,
        };
        self.down &= !mask;
    }
    pub fn has_pressed_activation(&self) -> bool {
        self.down != 0
    }
    pub fn clear(&mut self) {
        self.down = 0;
    }
}

#[cfg(test)]
mod focused_toggle_tests {
    use super::{ActivationKeys, FocusedToggleAction::*};

    #[test]
    fn stopping_transport_cannot_become_a_toggle_after_state_refresh() {
        let mut keys = ActivationKeys::default();
        assert_eq!(
            keys.press_receipt("space", false)
                .focused_toggle(false, true, true, false),
            Pass
        );
        // Transport has stopped and the target is now eligible. Neither an
        // unflagged X11 repeat nor a platform-marked repeat becomes a new edit.
        for held in [false, true, false] {
            assert_eq!(
                keys.press_receipt("space", held)
                    .focused_toggle(false, true, false, true),
                Consume
            );
        }
        keys.release("space"); // Key-up itself has no activation operation.
        assert_eq!(
            keys.press_receipt("space", false)
                .focused_toggle(false, true, false, true),
            Activate
        );
    }

    #[test]
    fn paused_toggle_activates_once_per_independent_press() {
        let mut keys = ActivationKeys::default();
        for key in ["enter", "space"] {
            assert_eq!(
                keys.press_receipt(key, false)
                    .focused_toggle(false, true, false, true),
                Activate
            );
            for held in [false, true, false] {
                assert_eq!(
                    keys.press_receipt(key, held)
                        .focused_toggle(false, true, false, true),
                    Consume
                );
            }
            keys.release(key);
            assert_eq!(
                keys.press_receipt(key, false)
                    .focused_toggle(false, true, false, true),
                Activate
            );
            keys.release(key);
        }
    }

    #[test]
    fn rejected_or_elsewhere_press_cannot_be_retargeted_while_held() {
        let mut keys = ActivationKeys::default();
        for (modified, active, eligible) in [
            (true, true, true),
            (false, false, true),
            (false, true, false),
        ] {
            assert_eq!(
                keys.press_receipt("space", false)
                    .focused_toggle(modified, active, false, eligible),
                Consume
            );
            assert_eq!(
                keys.press_receipt("space", false)
                    .focused_toggle(false, true, false, true),
                Consume
            );
            keys.release("space");
        }
        // Workspace capture observes a down in another field before focus moves
        // here, and observes release even when focus subsequently leaves again.
        assert!(keys.press("space", false));
        assert_eq!(
            keys.press_receipt("space", false)
                .focused_toggle(false, true, false, true),
            Consume
        );
        keys.release("space");
        assert_eq!(
            keys.press_receipt("space", false)
                .focused_toggle(false, true, false, true),
            Activate
        );
    }

    #[test]
    fn pointer_only_rejects_edits_but_allows_transport_stop() {
        for negotiation in [Ok(false), Err(())] {
            let mut keys = ActivationKeys::from_release_negotiation(negotiation);
            assert_eq!(
                keys.press_receipt("space", false)
                    .focused_toggle(false, true, true, false),
                Pass
            );
            for key in ["space", "enter"] {
                for _ in 0..3 {
                    keys.release(key);
                    assert_eq!(
                        keys.press_receipt(key, false)
                            .focused_toggle(false, true, false, true),
                        Consume
                    );
                }
            }
        }
    }

    #[test]
    fn activation_keys_remain_independent_and_other_keys_pass() {
        let mut keys = ActivationKeys::default();
        assert!(keys.press("enter", false));
        assert_eq!(
            keys.press_receipt("space", false)
                .focused_toggle(false, true, true, false),
            Pass
        );
        assert_eq!(
            keys.press_receipt("enter", false)
                .focused_toggle(false, true, false, true),
            Consume
        );
        assert_eq!(
            keys.press_receipt("v", false)
                .focused_toggle(false, true, false, true),
            Pass
        );
    }
}
#[cfg(test)]
mod activation_tests {
    use super::ActivationKeys;
    #[test]
    fn repeat_without_platform_flag_cannot_cross_modal_or_text_ownership() {
        let mut keys = ActivationKeys::default();
        assert!(keys.press("enter", false)); // field default opens confirm
        assert!(!keys.press("enter", false)); // X11 repeat in confirm
        assert!(!keys.press("enter", true));
        keys.release("enter");
        assert!(keys.press("enter", false));
        assert!(keys.press("space", false)); // ordinary field text may open alert
        assert!(!keys.press("space", false)); // same held Space cannot dismiss it
        keys.release("space");
        assert!(keys.press("space", false));
    }
    #[test]
    fn keys_are_independent_and_new_run_clears_ownership() {
        let mut keys = ActivationKeys::default();
        assert!(!keys.press("enter", true));
        assert!(!keys.press("enter", false));
        assert!(keys.press("escape", false));
        assert!(!keys.press("escape", false));
        keys.release("space");
        assert!(!keys.press("enter", false));
        keys.clear();
        assert!(keys.press("enter", false));
        assert!(!keys.press("a", false));
    }
}

#[cfg(test)]
mod release_capability_tests {
    use super::ActivationKeys;
    #[test]
    fn negotiation_errors_and_unsupported_reply_fail_closed() {
        for result in [Err(()), Ok(false)] {
            let mut keys = ActivationKeys::from_release_negotiation(result);
            assert!(!keys.press("enter", false));
            keys.release("enter");
            assert!(!keys.press("enter", false));
        }
        assert!(ActivationKeys::from_release_negotiation(Ok(true)).press("enter", false));
    }
    #[test]
    fn unsupported_server_never_activates_after_synthetic_releases() {
        let mut keys = ActivationKeys::pointer_only();
        for key in ["enter", "space", "escape"] {
            for _ in 0..4 {
                assert!(!keys.press(key, false));
                keys.release(key);
            }
            keys.clear(); // a new script cannot upgrade unsupported capability
            assert!(!keys.press(key, false));
        }
    }
    #[test]
    fn reliable_server_requires_release_for_each_independent_action() {
        let mut keys = ActivationKeys::default();
        for key in ["enter", "space", "escape"] {
            assert!(keys.press(key, false));
            for _ in 0..20 {
                assert!(!keys.press(key, false));
            }
            keys.release(key);
            assert!(keys.press(key, false));
            keys.release(key);
        }
    }
    #[test]
    fn pointer_only_retains_exit_barrier_but_ignores_nonactivation_keys() {
        let mut keys = ActivationKeys::pointer_only();
        assert!(!keys.press("g", false));
        assert!(!keys.has_pressed_activation());
        assert!(!keys.press("space", false));
        assert!(keys.has_pressed_activation());
        keys.release("space");
        assert!(!keys.has_pressed_activation());
    }
}
