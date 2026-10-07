//! Platform repeat semantics negotiated once on the app's existing connection.
//! Failed/unsupported negotiation leaves modal activation pointer-only, while
//! native text insertion and Tab/navigation remain independent.
use gpui::{App, Global, KeyDownEvent, Window, WindowId};
use libre_effects_editor_model::automation_ui::{ActivationKeys, ActivationPress};

struct ModalKeyboard {
    negotiation: Result<bool, ()>,
    press: Option<WorkspacePress>,
}
struct WorkspacePress {
    window: WindowId,
    event: KeyDownEvent,
    activation: ActivationPress,
}
impl Global for ModalKeyboard {}

pub(crate) const POINTER_ONLY_WARNING: &str = "This display cannot safely distinguish held confirmation keys. Use the buttons to confirm or cancel. Text typing and Tab still work.";

pub(crate) fn initialize(window: &Window, cx: &mut App) {
    // This opt-in safe mode can only remove keyboard activation, never force it.
    let forced = std::env::var_os("LIBREEFFECTS_MODAL_POINTER_ONLY").as_deref()
        == Some(std::ffi::OsStr::new("1"));
    let result = if forced {
        Ok(false)
    } else {
        window.request_reliable_key_releases()
    };
    let negotiation = match result {
        Ok(true) => {
            eprintln!(
                "Modal keyboard: server repeat releases suppressed or native backend verified"
            );
            Ok(true)
        }
        Ok(false) => {
            eprintln!(
                "Modal keyboard: unsupported release semantics; confirmation is pointer-only"
            );
            Ok(false)
        }
        Err(error) => {
            eprintln!(
                "Modal keyboard: release negotiation failed ({error}); confirmation is pointer-only"
            );
            Err(())
        }
    };
    cx.set_global(ModalKeyboard {
        negotiation,
        press: None,
    });
}

/// Share only this dispatch's receipt from the existing workspace latch. This
/// lets Repeat recognize a held key that began in another focused control.
pub(crate) fn record_press(
    event: &KeyDownEvent,
    activation: ActivationPress,
    window: &Window,
    cx: &mut App,
) {
    if cx.try_global::<ModalKeyboard>().is_some() {
        cx.global_mut::<ModalKeyboard>().press = Some(WorkspacePress {
            window: window.window_handle().window_id(),
            event: event.clone(),
            activation,
        });
    }
}
pub(crate) fn take_press(
    event: &KeyDownEvent,
    window: &Window,
    cx: &mut App,
) -> Option<ActivationPress> {
    cx.try_global::<ModalKeyboard>()?;
    let press = cx.global_mut::<ModalKeyboard>().press.take()?;
    (press.window == window.window_handle().window_id() && press.event == *event)
        .then_some(press.activation)
}
pub(crate) fn clear_press(window: &Window, cx: &mut App) {
    if cx.try_global::<ModalKeyboard>().is_some_and(|state| {
        state
            .press
            .as_ref()
            .is_some_and(|press| press.window == window.window_handle().window_id())
    }) {
        cx.global_mut::<ModalKeyboard>().press = None;
    }
}

pub(crate) fn activation_keys(cx: &App) -> ActivationKeys {
    ActivationKeys::from_release_negotiation(
        cx.try_global::<ModalKeyboard>()
            .map_or(Err(()), |state| state.negotiation),
    )
}
fn supported(cx: &App) -> bool {
    cx.try_global::<ModalKeyboard>()
        .is_some_and(|state| state.negotiation == Ok(true))
}
pub(crate) fn warning(cx: &App) -> Option<&'static str> {
    (!supported(cx)).then_some(POINTER_ONLY_WARNING)
}
