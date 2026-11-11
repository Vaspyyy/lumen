//! lumen: a terminal emulator.

mod actions;
mod config;
mod element;
mod keys;
mod mouse;
mod palette;
mod terminal;
mod view;
mod workspace;

use std::borrow::Cow;
use std::path::PathBuf;

use gpui::{
    App, AppContext, AssetSource, Bounds, SharedString, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions, px, size,
};

use crate::config::Config;
use crate::terminal::Spawn;
use crate::workspace::Workspace;

/// A debug build with LUMEN_PREVIEW set draws itself as the framed, active
/// window it is on a desktop, where there is none to make it one: a bare X
/// display, for screenshots.
pub fn preview() -> bool {
    cfg!(debug_assertions) && std::env::var_os("LUMEN_PREVIEW").is_some()
}

/// The icons, compiled into the program.
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/close.svg" => include_bytes!("../assets/icons/close.svg"),
            "icons/plus.svg" => include_bytes!("../assets/icons/plus.svg"),
            "icons/minimize.svg" => include_bytes!("../assets/icons/minimize.svg"),
            "icons/maximize.svg" => include_bytes!("../assets/icons/maximize.svg"),
            "icons/restore.svg" => include_bytes!("../assets/icons/restore.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

const USAGE: &str = "\
lumen: a terminal emulator

USAGE
    lumen [options] [-e program [args...]]

OPTIONS
    -e, --command <program> [args...]   run this instead of the shell (takes the rest of the line)
    -d, --working-directory <dir>       start in this folder
        --config-path                   print where the config file is read from
    -v, --version                       print the version
    -h, --help                          print this help
";

/// What the command line asked for, or the exit status when it asked for
/// something that needs no window.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Spawn, i32> {
    let mut spawn = Spawn::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Err(0);
            },
            "-v" | "--version" => {
                println!("lumen {}", env!("CARGO_PKG_VERSION"));
                return Err(0);
            },
            "--config-path" => {
                println!("{}", Config::path().display());
                return Err(0);
            },
            "-d" | "--working-directory" => match args.next() {
                Some(dir) => spawn.cwd = Some(PathBuf::from(dir)),
                None => {
                    eprintln!("lumen: {arg} wants a folder\nTry 'lumen --help'.");
                    return Err(2);
                },
            },
            "-e" | "--command" => {
                let command: Vec<String> = args.by_ref().collect();
                if command.is_empty() {
                    eprintln!("lumen: {arg} wants a program\nTry 'lumen --help'.");
                    return Err(2);
                }
                spawn.command = Some(command);
            },
            other => {
                eprintln!("lumen: unknown option {other}\nTry 'lumen --help'.");
                return Err(2);
            },
        }
    }
    Ok(spawn)
}

fn open_window(spawn: Spawn, cx: &mut App) {
    let config = cx.global::<Config>().clone();
    // room for the configured grid at a typical cell size; the grid follows the window from then on
    let bounds = Bounds::centered(
        None,
        size(px(config.window.columns as f32 * 9.7 + 24.), px(config.window.lines as f32 * 21. + 64.)),
        cx,
    );
    let see_through = config.window.opacity < 1.;
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(320.), px(180.))),
        titlebar: None,
        window_decorations: Some(WindowDecorations::Client),
        window_background: match (see_through, config.window.blur) {
            (false, _) => WindowBackgroundAppearance::Transparent,
            (true, true) => WindowBackgroundAppearance::Blurred,
            (true, false) => WindowBackgroundAppearance::Transparent,
        },
        app_id: Some("lumen".into()),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        cx.new(|cx| match Workspace::new(spawn, window, cx) {
            Ok(workspace) => workspace,
            Err(e) => {
                eprintln!("lumen: can't start the shell: {e}");
                std::process::exit(1);
            },
        })
    });
    if let Err(e) = opened {
        eprintln!("lumen: can't open a window: {e}");
        std::process::exit(1);
    }
}

fn main() {
    let spawn = match parse_args(std::env::args().skip(1)) {
        Ok(spawn) => spawn,
        Err(status) => std::process::exit(status),
    };
    let (config, problem) = Config::load();
    if let Some(problem) = problem {
        eprintln!("lumen: {problem}\nlumen: using the defaults");
    }

    gpui_platform::application().with_assets(Assets).run(move |cx: &mut App| {
        cx.set_global(config);
        actions::bind(cx);
        cx.on_action(|_: &actions::Quit, cx| cx.quit());
        cx.on_action(|_: &actions::NewWindow, cx| open_window(Spawn::default(), cx));
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        open_window(spawn, cx);
        cx.activate(true);
    });
}
