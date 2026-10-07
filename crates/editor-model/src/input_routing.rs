//! Keys which must reach the platform input handler rather than be inserted
//! from a key-down listener. This preserves composed text and AltGr.
pub fn native_text_key(key: &str, control: bool, platform: bool, alt: bool) -> bool {
    !platform
        && (!control || alt)
        && !matches!(
            key,
            "enter"
                | "tab"
                | "escape"
                | "backspace"
                | "delete"
                | "left"
                | "right"
                | "home"
                | "end"
                | "up"
                | "down"
                | "pageup"
                | "pagedown"
        )
        && !(alt && key == "f4")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printable_dead_and_altgr_keys_reach_native_input() {
        for key in ["g", "G", "space", "3", "eacute", "dead_acute", "v", "t"] {
            assert!(native_text_key(key, false, false, false));
            assert!(native_text_key(key, true, false, true));
            assert!(!native_text_key(key, true, false, false));
            assert!(!native_text_key(key, false, true, false));
        }
    }
    #[test]
    fn navigation_is_not_synthesized_as_text() {
        for key in [
            "enter",
            "tab",
            "escape",
            "backspace",
            "delete",
            "left",
            "right",
            "home",
            "end",
            "up",
            "down",
            "pageup",
            "pagedown",
        ] {
            assert!(!native_text_key(key, false, false, false));
        }
        assert!(!native_text_key("f4", false, false, true));
    }
}

/// Block only a known-held activation after its modal has closed. A fresh
/// physical press or an unrelated editor key keeps its normal meaning.
pub fn suppress_modal_exit_key(barrier: bool, modal_open: bool, key: &str, fresh: bool) -> bool {
    barrier && !modal_open && !fresh && matches!(key, "enter" | "space" | "escape")
}

#[cfg(test)]
mod modal_exit_tests {
    use super::*;
    use crate::automation_ui::ActivationKeys;
    #[test]
    fn held_activation_cannot_start_playback_after_modal_closes() {
        let mut keys = ActivationKeys::default();
        assert!(keys.press("space", false));
        assert!(keys.has_pressed_activation());
        assert!(suppress_modal_exit_key(
            true,
            false,
            "space",
            keys.press("space", false)
        ));
        keys.release("space");
        assert!(!keys.has_pressed_activation());
        assert!(!suppress_modal_exit_key(
            true,
            false,
            "space",
            keys.press("space", false)
        ));
    }
    #[test]
    fn barrier_does_not_capture_other_keys_or_current_modal_actions() {
        assert!(!suppress_modal_exit_key(true, false, "v", false));
        assert!(!suppress_modal_exit_key(true, true, "enter", false));
        assert!(!suppress_modal_exit_key(false, false, "escape", false));
    }
}
