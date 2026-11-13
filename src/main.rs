//! lumen: a terminal emulator.

mod actions;
mod blocks;
mod commands;
mod config;
mod editor;
mod element;
mod fonts;
mod graphics;
mod instance;
mod integration;
mod keys;
mod lag;
mod links;
mod mouse;
mod palette;
mod phone;
mod pty;
mod pulse;
mod queries;
mod settings;
mod slash;
mod terminal;
mod update;
mod view;
mod workspace;

use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    App, AppContext, AssetSource, Bounds, CursorHideMode, QuitMode, SharedString, WindowBounds, WindowHandle,
    WindowOptions, WindowingRequest, px, size,
};

use crate::config::{Config, StartupMode};
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
            "icons/search.svg" => include_bytes!("../assets/icons/search.svg"),
            "icons/pulse.svg" => include_bytes!("../assets/icons/pulse.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

const USAGE: &str = "\
USAGE
    lumen [options] [-e program [args...]]

OPTIONS
    -e, --command <program> [args...]   run this instead of the shell (takes the rest of the line)
    -d, --working-directory <dir>       start in this folder
        --new-window                    a window of its own, where a lumen runs already
        --new-process                   a lumen of its own, that starts from nothing
        --daemon                        wait without a window, so that windows open at once
        --config-path                   print where the config file is read from
    -V, --version                       print the version
    -h, --help                          print this help
";

/// `lumen --help`: the banner the other tools' help has, then the sections,
/// with the banner and the headings colored on a terminal.
fn help_text(color: bool) -> String {
    let paint = |code: &str, s: &str| if color { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() };
    // packaging/install.sh knows a lumen by "✦ lumen" in the program
    let mut text = format!(
        "\n  {} {}  {}\n\n",
        paint("38;5;183;1", "✦ lumen"),
        paint("38;5;59", &format!("v{}", env!("CARGO_PKG_VERSION"))),
        paint("38;5;146;3", "a terminal emulator"),
    );
    for line in
        format!("{USAGE}\n{}\n{}\n{}\n{}", settings::USAGE, update::USAGE, integration::USAGE, phone::USAGE).lines()
    {
        if line.is_empty() {
            text.push('\n');
        } else if line.starts_with(|c: char| c.is_ascii_uppercase()) {
            text.push_str(&format!("  {}\n", paint("38;5;111;1", line)));
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    text.push('\n');
    text
}

/// How this lumen comes to show what was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// Through the lumen that runs, if one does: a tab, or a window, as its config says.
    Join,
    /// A window of the lumen that runs, if one does (`--new-window`).
    Window,
    /// On its own, as if no lumen ran (`--new-process`).
    Process,
    /// No window: waits for launches (`--daemon`).
    Daemon,
}

/// What the command line asked for.
struct Started {
    spawn: Spawn,
    how: How,
}

/// What the command line asked for, or the exit status when it asked for
/// something that needs no window.
fn parse_args(args: impl Iterator<Item = String>) -> Result<Started, i32> {
    let mut spawn = Spawn::default();
    let mut how = How::Join;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                use std::io::IsTerminal;
                let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
                print!("{}", help_text(color));
                return Err(0);
            },
            // -v is what it was before the tools agreed on -V
            "-V" | "-v" | "--version" => {
                println!("lumen {}", env!("CARGO_PKG_VERSION"));
                return Err(0);
            },
            "--config-path" => {
                println!("{}", Config::path().display());
                return Err(0);
            },
            "--new-window" => how = How::Window,
            "--new-process" => how = How::Process,
            "--daemon" => how = How::Daemon,
            "-d" | "--working-directory" => match args.next() {
                // a running lumen is somewhere else: the folder is made whole here
                Some(dir) => spawn.cwd = Some(std::path::absolute(&dir).unwrap_or_else(|_| PathBuf::from(dir))),
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
            other if other.starts_with('-') => {
                eprintln!("lumen: unknown flag {other} (see lumen --help)");
                return Err(2);
            },
            other => {
                eprintln!("lumen: unknown command {other} (see lumen --help)");
                return Err(2);
            },
        }
    }
    if how == How::Daemon && (spawn.cwd.is_some() || spawn.command.is_some()) {
        eprintln!("lumen: --daemon opens nothing itself: it takes no -d or -e\nTry 'lumen --help'.");
        return Err(2);
    }
    Ok(Started { spawn, how })
}

/// Why a window wasn't opened.
pub enum Unopened {
    /// The window itself: the display or the GPU.
    Window(String),
    /// Its terminal: the program or the folder.
    Shell(String),
}

impl std::fmt::Display for Unopened {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Unopened::Window(why) | Unopened::Shell(why) => f.write_str(why),
        }
    }
}

/// Opens a window with a terminal: the first one of a lumen, `Ctrl+Shift+N`, or one a
/// launch asked for.
pub fn open_window(spawn: Spawn, cx: &mut App) -> Result<WindowHandle<Workspace>, Unopened> {
    let config = cx.global::<Config>().clone();
    // room for the configured grid at a typical cell size, its padding, the tab bar and the
    // window's border; the grid follows the window from then on
    let pad = config.window.padding;
    let bar = if config.tab_bar(1) { workspace::TAB_BAR } else { 0. };
    let bounds = Bounds::centered(
        None,
        size(
            px(config.window.columns as f32 * 9.7 + pad.x * 2. + 4.),
            px(config.window.lines as f32 * 21. + pad.y * 2. + bar + 4.),
        ),
        cx,
    );
    let options = WindowOptions {
        window_bounds: Some(match config.window.startup_mode {
            StartupMode::Windowed => WindowBounds::Windowed(bounds),
            StartupMode::Maximized => WindowBounds::Maximized(bounds),
            StartupMode::Fullscreen => WindowBounds::Fullscreen(bounds),
        }),
        window_min_size: Some(size(px(320.), px(180.))),
        titlebar: None,
        window_decorations: Some(config.decorations()),
        window_background: config.appearance(),
        app_id: Some("lumen".into()),
        // GPUI holds a window to 30 frames a second in the power-saver profile: typing
        // then waits up to two refreshes of a fast display for its frame, and the cursor
        // glides in two steps. A terminal draws only when something changes, so it keeps
        // the display's rate whatever the profile.
        low_power_frame_interval: None,
        ..Default::default()
    };
    // a terminal that doesn't start leaves the window empty, and it closes again
    let failed = Rc::new(RefCell::new(None));
    let opened = cx.open_window(options, {
        let failed = failed.clone();
        move |window, cx| {
            cx.new(|cx| match Workspace::new(spawn, window, cx) {
                Ok(workspace) => workspace,
                Err(e) => {
                    *failed.borrow_mut() = Some(e);
                    Workspace::empty(&Spawn::default(), window, cx)
                },
            })
        }
    });
    let window = opened.map_err(|e| Unopened::Window(format!("can't open a window: {e}")))?;
    if let Some(e) = failed.take() {
        let _ = window.update(cx, |_, window, _| window.remove_window());
        return Err(Unopened::Shell(format!("can't start the shell: {e}")));
    }
    instance::note_active(window.into(), cx);
    pulse::begin_after_first_frame(cx);
    Ok(window)
}

/// What the config sets for all of lumen at once: the keys, and whether typing hides the pointer.
fn apply_config(cx: &mut App) {
    actions::bind(cx);
    let hide = if cx.global::<Config>().mouse.hide_while_typing {
        CursorHideMode::OnTypingAndAction
    } else {
        CursorHideMode::Never
    };
    cx.set_cursor_hide_mode(hide);
}

/// Takes on the config file whenever it is saved, without a restart.
fn watch_config(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut seen = Config::modified();
        loop {
            cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
            let now = Config::modified();
            if now == seen {
                continue;
            }
            seen = now;
            cx.update(|cx| {
                let config = cx.global::<Config>().reloaded();
                cx.set_global(config);
                apply_config(cx);
                cx.refresh_windows();
            });
        }
    })
    .detach();
}

fn main() {
    // the commands for phones work over SSH, with no display: before anything of the window
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(status) = phone::run(&args)
        .or_else(|| integration::run(&args))
        .or_else(|| update::run(&args))
        .or_else(|| settings::run(&args))
    {
        std::process::exit(status);
    }
    let Started { spawn, mut how } = match parse_args(args.into_iter()) {
        Ok(started) => started,
        Err(status) => std::process::exit(status),
    };
    // a lumen that records its lag is one of its own (see lag.rs)
    let measuring = cfg!(feature = "lag") && std::env::var_os("LUMEN_LAG").is_some_and(|path| !path.is_empty());
    if measuring {
        how = How::Process;
    }
    // A lumen that runs already opens what was asked for, in a fraction of the time this
    // one would take to start: before anything of the GPU, the config or the fonts.
    if matches!(how, How::Join | How::Window)
        && let Some(status) = instance::hand_over(&spawn, how == How::Window)
    {
        std::process::exit(status);
    }
    let daemon = how == How::Daemon;
    // Loading the GPU driver and making a device is a good part of the time before the
    // first frame: it starts now, on a thread of its own, while the config, the fonts and
    // the window are made. The window takes the device when it opens.
    gpui_wgpu::prewarm();
    let (config, problem) = Config::load();
    if let Some(problem) = problem {
        eprintln!("lumen: {}: {problem}\nlumen: using the defaults", Config::path().display());
    }

    let mut application = gpui_platform::application().with_assets(Assets);
    if daemon {
        // no display until a launch names one: the daemon outlives the sessions it serves,
        // and stays when its last window closes
        application = application.with_windowing(WindowingRequest::Headless).with_quit_mode(QuitMode::Explicit);
    }
    application.run(move |cx: &mut App| {
        lag::start(cx);
        cx.set_global(config);
        fonts::init(cx);
        apply_config(cx);
        cx.on_action(move |_: &actions::Quit, cx| {
            if !daemon {
                cx.quit();
                return;
            }
            // the daemon closes its windows and goes back to waiting
            for window in cx.windows() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.on_action(|_: &actions::NewWindow, cx| {
            // in the folder the window it is pressed in was opened in, with its environment
            let from = cx.active_window().and_then(|window| window.downcast::<Workspace>());
            let spawn = from.and_then(|window| window.read_with(cx, |workspace, cx| workspace.spawn_window(cx)).ok());
            if let Err(e) = open_window(spawn.unwrap_or_default(), cx) {
                eprintln!("lumen: {e}");
            }
        });
        cx.on_window_closed(move |cx, _| {
            if !cx.windows().is_empty() {
                return;
            }
            if daemon {
                instance::wait_without_window(cx);
            } else {
                cx.quit();
            }
        })
        .detach();
        // a click on a notification leads to the pane it is about
        cx.on_system_notification_response(|response, cx| {
            for window in cx.windows() {
                let Some(window) = window.downcast::<Workspace>() else { continue };
                let shown = window.update(cx, |workspace, window, cx| workspace.show_pane(&response.tag, window, cx));
                if shown.unwrap_or(false) {
                    break;
                }
            }
        });
        pulse::init(cx);
        if !measuring {
            instance::serve(daemon, cx);
        }
        if !daemon {
            if let Err(e) = open_window(spawn, cx) {
                eprintln!("lumen: {e}");
                gpui_wgpu::finish_prewarm();
                std::process::exit(1);
            }
            cx.activate(true);
            lag::drive(cx);
        }
        watch_config(cx);
    });
    gpui_wgpu::finish_prewarm();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> impl Iterator<Item = String> {
        list.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter()
    }

    #[test]
    fn help_has_the_banner_install_sh_knows() {
        // packaging/install.sh tells a lumen from other programs by it
        let help = help_text(false);
        assert!(help.starts_with(&format!("\n  ✦ lumen v{}  a terminal emulator\n", env!("CARGO_PKG_VERSION"))));
        assert!(help.contains("\n    -V, --version "));
        assert!(help.contains("\n  PHONES\n"));
        assert!(!help.contains('\x1b'));
        assert!(help_text(true).contains("\x1b[38;5;111;1mOPTIONS\x1b[0m"));
    }

    #[test]
    fn unknown_flags_and_commands_are_usage_errors() {
        assert!(matches!(parse_args(args(&["--bogus"])), Err(2)));
        assert!(matches!(parse_args(args(&["bogus"])), Err(2)));
        assert!(matches!(parse_args(args(&["-d"])), Err(2)));
        assert!(matches!(parse_args(args(&["--daemon", "-e", "htop"])), Err(2)));
    }

    #[test]
    fn a_launch_joins_the_running_lumen_unless_told_otherwise() {
        let how = |list: &[&str]| parse_args(args(list)).ok().map(|started| started.how);
        assert_eq!(how(&[]), Some(How::Join));
        assert_eq!(how(&["-e", "--new-window"]), Some(How::Join));
        assert_eq!(how(&["--new-window", "-d", "/tmp"]), Some(How::Window));
        assert_eq!(how(&["--new-process"]), Some(How::Process));
        assert_eq!(how(&["--daemon"]), Some(How::Daemon));
    }

    #[test]
    fn a_folder_is_made_whole_for_the_running_lumen() {
        let started = parse_args(args(&["-d", "notes"])).ok().unwrap();
        let cwd = started.spawn.cwd.unwrap();
        assert!(cwd.is_absolute());
        assert!(cwd.ends_with("notes"));
    }
}
