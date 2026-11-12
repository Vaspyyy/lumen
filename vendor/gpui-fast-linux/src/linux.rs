// Modified by Longbridge for gpui-fast.
// Modified for lumen: `give_activation_token`.
mod dispatcher;
mod display_connection;
mod headless;
mod keyboard;
mod platform;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod power;
mod system_notifications;
#[cfg(any(feature = "wayland", feature = "x11"))]
mod text_system;
#[cfg(feature = "wayland")]
pub(crate) mod wayland;
#[cfg(feature = "x11")]
mod x11;

#[cfg(any(feature = "wayland", feature = "x11"))]
mod xdg_desktop_portal;

pub use dispatcher::*;
pub(crate) use display_connection::{Backend, DisplayConnection, select_backend};
pub(crate) use headless::*;
pub(crate) use keyboard::*;
pub(crate) use platform::*;
#[cfg(any(feature = "wayland", feature = "x11"))]
pub(crate) use text_system::*;
#[cfg(feature = "wayland")]
pub(crate) use wayland::*;
#[cfg(feature = "x11")]
pub(crate) use x11::*;

use std::rc::Rc;
use std::sync::Mutex;

use gpui::WindowingModes;

/// A token that another program got for this one: see [`give_activation_token`].
static GIVEN_ACTIVATION_TOKEN: Mutex<Option<String>> = Mutex::new(None);

/// Hands the platform an xdg-activation token that another program got, for example the
/// launcher that asked this running app for a window. On Wayland, the next window that opens
/// uses it to take focus, or the next window that is activated, if that comes first: a token
/// the app requests itself is only granted right after the user did something in it. A newer
/// token replaces one that is still unused. X11 has no such tokens and ignores it.
pub fn give_activation_token(token: String) {
    *GIVEN_ACTIVATION_TOKEN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token).filter(|t| !t.is_empty());
}

/// The token [`give_activation_token`] handed over, once.
#[cfg_attr(not(feature = "wayland"), allow(dead_code))]
pub(crate) fn take_given_activation_token() -> Option<String> {
    GIVEN_ACTIVATION_TOKEN
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// Returns the default platform implementation for the current OS.
///
/// A windowed platform connects to the display server the process environment names, or starts
/// headless when it names none, and can switch between those modes later. A headless platform
/// stays headless.
pub fn current_platform(headless: bool) -> Rc<dyn gpui::Platform> {
    if headless || std::env::var_os("ZED_HEADLESS").is_some() {
        linux_platform(WindowingModes::HEADLESS)
    } else {
        linux_platform(WindowingModes::all())
    }
}

/// Returns a platform that may switch among `allowed_modes`.
///
/// It starts windowed in the process's own environment, or headless if that names no allowed
/// display server. Set another initial mode with [`gpui::Application::with_windowing`].
pub fn linux_platform(allowed_modes: WindowingModes) -> Rc<dyn gpui::Platform> {
    Rc::new(LinuxPlatform::new(allowed_modes))
}
