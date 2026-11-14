//! Which fonts lumen can use on this system.
//!
//! GPUI panics when it is asked for a family that is not installed and none
//! of a short list of common ones is either. A system with one fixed-width
//! font and an uncommon one for everything else is enough for that. A
//! terminal has to start on whatever fonts there are, so lumen asks first,
//! and names only families that can be found.

use std::panic::{AssertUnwindSafe, catch_unwind, set_hook, take_hook};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use gpui::{App, SharedString, TextSystem, font};

/// GPUI's name for the font of the desktop.
const SYSTEM: &str = ".SystemUIFont";

/// The desktop's font for lumen's own words, as found when lumen started.
static DESKTOP: OnceLock<SharedString> = OnceLock::new();
/// The one in use: the config's `ui_family` where it can be found, or else the desktop's.
static UI: RwLock<Option<SharedString>> = RwLock::new(None);
/// The family the config last asked for, and the one that was found for it.
static GRID: Mutex<Option<(String, SharedString)>> = Mutex::new(None);

/// The family of lumen's own words: the tab titles, the search field, the
/// notices.
pub fn ui() -> SharedString {
    let chosen = UI.read().ok().and_then(|ui| ui.clone());
    chosen.or_else(|| DESKTOP.get().cloned()).unwrap_or_else(|| SYSTEM.into())
}

/// Takes on the family the config names for lumen's own words: the desktop's
/// when it names none, or one that can't be found (which is said).
pub fn set_ui(wanted: Option<&str>, cx: &App) {
    let chosen = wanted.map(str::trim).filter(|wanted| !wanted.is_empty()).and_then(|wanted| {
        if resolves(cx.text_system(), wanted) {
            return Some(SharedString::from(wanted.to_string()));
        }
        eprintln!("lumen: the font {wanted:?} for lumen's own words can't be found, using the desktop's");
        None
    });
    if let Ok(mut ui) = UI.write() {
        *ui = chosen;
    }
}

/// Looks for the font of lumen's own words, once, when lumen starts. Ends
/// lumen with a message when the system has no font at all.
pub fn init(cx: &App) {
    let text = cx.text_system();
    let ui = if resolves(text, SYSTEM) {
        SYSTEM.into()
    } else {
        // what the desktop takes for text without serifs, or else anything
        let names = text.all_font_names();
        let found = fontconfig("sans-serif").into_iter().chain(names).find(|family| resolves(text, family));
        let Some(found) = found else {
            eprintln!("lumen: this system has no font to draw text with. Install one: DejaVu, for example.");
            std::process::exit(1);
        };
        found.into()
    };
    let _ = DESKTOP.set(ui);
}

/// The family to draw the grid in: the one the config names when it can be
/// found, or else a fixed-width one that can.
pub fn grid(wanted: &str, text: &TextSystem) -> SharedString {
    let mut last = GRID.lock().unwrap();
    if let Some((asked, found)) = &*last
        && asked == wanted
    {
        return found.clone();
    }
    let found: SharedString = if resolves(text, wanted) {
        wanted.to_string().into()
    } else {
        let names = text.all_font_names();
        let fixed = names.iter().filter(|name| name.to_lowercase().contains("mono")).cloned();
        let other = fontconfig("monospace").into_iter().chain(fixed).chain(names.iter().cloned());
        match other.into_iter().find(|family| resolves(text, family)) {
            Some(other) => {
                eprintln!("lumen: the font {wanted:?} can't be found, using {other:?}");
                other.into()
            },
            None => {
                eprintln!("lumen: this system has no font to draw text with. Install one: DejaVu, for example.");
                std::process::exit(1);
            },
        }
    };
    *last = Some((wanted.to_string(), found.clone()));
    found
}

/// Whether GPUI has a font to draw this family with: the family itself, or
/// one of the common ones it puts in its place. It panics when it has none,
/// and has no other way to be asked, so the question is put where a panic is
/// caught. (A build that aborts on a panic ends here as it would have ended
/// at the first text.)
fn resolves(text: &TextSystem, family: &str) -> bool {
    let font = font(SharedString::from(family.to_string()));
    // The panic of this question is not worth a line. One on another thread,
    // in the same moment, still is.
    let asking = std::thread::current().id();
    let usual = Arc::new(take_hook());
    set_hook(Box::new({
        let usual = usual.clone();
        move |info| {
            if std::thread::current().id() != asking {
                usual(info);
            }
        }
    }));
    let found = catch_unwind(AssertUnwindSafe(|| text.resolve_font(&font))).is_ok();
    // taking ours down lets go of its hold on the usual one, which goes back as it was
    drop(take_hook());
    match Arc::try_unwrap(usual) {
        Ok(usual) => set_hook(usual),
        Err(usual) => set_hook(Box::new(move |info| usual(info))),
    }
    found
}

/// What fontconfig takes for a kind of font ("sans-serif", "monospace"), if
/// it is installed.
fn fontconfig(kind: &str) -> Option<String> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{family}", kind]).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    // a font can have several names, the first is the one to ask for
    let first = text.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_string())
}
