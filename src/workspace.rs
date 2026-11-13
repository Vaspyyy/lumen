//! The window: its tabs, and the panes each tab is split into.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Axis, Bounds, BoxShadow, Context, Corners, CursorStyle, Decorations, Entity, EntityId, Focusable,
    Hsla, MouseButton, MouseMoveEvent, MouseUpEvent, Pixels, ResizeEdge, SharedString, Size, Subscription,
    SystemNotification, Task, Tiling, Window, WindowDecorations, canvas, div, point, px, size, svg,
};

use crate::actions::{
    ClosePane, CloseTab, FocusDown, FocusLeft, FocusRight, FocusUp, GoToTab, MoveTabLeft, MoveTabRight, NewTab,
    NextTab, OpenPulse, PreviousTab, SplitDown, SplitRight, ToggleZoomPane,
};
use crate::commands::Note;
use crate::config::Config;
use crate::editor;
use crate::element::Metrics;
use crate::pulse::{Pulse, Use};
use crate::slash::Slash;
use crate::terminal::{CommandEvent, Env, Spawn, Terminal};
use crate::view::{TerminalView, ViewEvent};

type Pane = Entity<TerminalView>;

/// Left, top, width and height, as parts of the tab's area.
type Rect = (f32, f32, f32, f32);

/// A tab's layout: a pane, or panes side by side (or stacked), each of
/// which can be split again.
enum Node {
    Leaf(Pane),
    Split {
        id: usize,
        axis: Axis,
        children: Vec<Node>,
        /// Each child's share of the split, adding up to 1.
        shares: Vec<f32>,
    },
}

impl Node {
    fn panes(&self, out: &mut Vec<Pane>) {
        match self {
            Node::Leaf(pane) => out.push(pane.clone()),
            Node::Split { children, .. } => children.iter().for_each(|child| child.panes(out)),
        }
    }

    fn contains(&self, pane: &Pane) -> bool {
        match self {
            Node::Leaf(p) => p == pane,
            Node::Split { children, .. } => children.iter().any(|child| child.contains(pane)),
        }
    }

    /// Puts `new` next to `target` along `axis`: into the split `target`
    /// is already part of if that runs the same way, else into a new one
    /// in `target`'s place. `target` gives up half its room.
    fn split(&mut self, target: &Pane, new: Pane, axis: Axis, next_id: &mut usize) -> bool {
        match self {
            Node::Leaf(pane) if pane == target => {
                let old = std::mem::replace(self, Node::Leaf(new.clone()));
                *next_id += 1;
                *self =
                    Node::Split { id: *next_id, axis, children: vec![old, Node::Leaf(new)], shares: vec![0.5, 0.5] };
                true
            },
            Node::Leaf(_) => false,
            Node::Split { axis: own_axis, children, shares, .. } => {
                let direct = children.iter().position(|child| matches!(child, Node::Leaf(p) if p == target));
                if let (Some(i), true) = (direct, *own_axis == axis) {
                    let half = shares[i] / 2.;
                    shares[i] = half;
                    shares.insert(i + 1, half);
                    children.insert(i + 1, Node::Leaf(new));
                    return true;
                }
                children.iter_mut().any(|child| child.split(target, new.clone(), axis, next_id))
            },
        }
    }

    /// Takes `target` out. Its room goes to its neighbours, and a split
    /// left with one child becomes that child. Returns false when `target`
    /// was the whole node, which the caller has to drop.
    fn remove(&mut self, target: &Pane) -> bool {
        let Node::Split { children, shares, .. } = self else {
            return false;
        };
        if let Some(i) = children.iter().position(|child| matches!(child, Node::Leaf(p) if p == target)) {
            children.remove(i);
            let freed = shares.remove(i);
            let rest: f32 = shares.iter().sum();
            for share in shares.iter_mut() {
                *share += freed * if rest > 0. { *share / rest } else { 0. };
            }
        } else if !children.iter_mut().any(|child| child.remove(target)) {
            return false;
        }
        if children.len() == 1 {
            *self = children.remove(0);
        }
        true
    }

    /// Every pane with the part of the tab it covers, as fractions of the
    /// tab's width and height.
    fn rects(&self, rect: Rect, out: &mut Vec<(Pane, Rect)>) {
        match self {
            Node::Leaf(pane) => out.push((pane.clone(), rect)),
            Node::Split { axis, children, shares, .. } => {
                let (x, y, w, h) = rect;
                let mut at = 0.;
                for (child, share) in children.iter().zip(shares) {
                    let part = match axis {
                        Axis::Horizontal => (x + w * at, y, w * share, h),
                        Axis::Vertical => (x, y + h * at, w, h * share),
                    };
                    child.rects(part, out);
                    at += share;
                }
            },
        }
    }

    fn shares_mut(&mut self, split: usize) -> Option<&mut Vec<f32>> {
        match self {
            Node::Leaf(_) => None,
            Node::Split { id, shares, children, .. } => {
                if *id == split {
                    return Some(shares);
                }
                children.iter_mut().find_map(|child| child.shares_mut(split))
            },
        }
    }
}

/// A tab on its way to another place in the bar.
#[derive(Clone)]
struct DraggedTab {
    id: usize,
    title: SharedString,
}

/// What is under the mouse while a tab is dragged.
struct TabGhost(SharedString);

impl Render for TabGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(pill(self.0.clone(), cx.global::<Config>()))
    }
}

/// What is said about the thing the mouse rests on: the first line, and
/// more in a softer voice below it.
struct Tip(Vec<SharedString>);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines =
            self.0.iter().enumerate().map(|(n, line)| {
                div().when(n > 0, |this| this.text_color(gpui::white().opacity(0.5))).child(line.clone())
            });
        // below the mouse, clear of what it rests on
        div()
            .pt(px(6.))
            .child(label(cx.global::<Config>()).flex().flex_col().gap(px(3.)).max_w(px(560.)).children(lines))
    }
}

/// A small label that floats over the terminal: a notice, a dragged tab.
fn pill(text: SharedString, config: &Config) -> gpui::Div {
    label(config).child(text)
}

fn label(config: &Config) -> gpui::Div {
    div()
        .px(px(12.))
        .py(px(6.))
        .rounded(px(9.))
        .bg(config.colors.primary.background.hsla().opacity(0.95))
        .border_1()
        .border_color(config.accent().opacity(0.45))
        .shadow(vec![BoxShadow {
            color: gpui::black().opacity(0.35),
            offset: point(px(0.), px(2.)),
            blur_radius: px(10.),
            spread_radius: px(0.),
            inset: false,
        }])
        .font_family(crate::fonts::ui())
        .text_size(px(12.5))
        .text_color(gpui::white().opacity(0.92))
}

/// The name a notification about a pane goes by: what leads back to the
/// pane when the notification is clicked.
fn tag_of(pane: EntityId) -> String {
    format!("lumen-pane-{}", pane.as_u64())
}

/// A pane that is forever telling is heard this often on the desktop.
const NOTE_GAP: Duration = Duration::from_secs(3);
/// And this many times, until its tab is looked at again. Each notification
/// on the desktop is held open until it goes away there.
const NOTE_MOST: u32 = 5;

/// When a pane last had a notification on the desktop, and how many it had
/// since its tab was last looked at.
#[derive(Clone, Copy)]
struct Noted {
    last: Instant,
    count: u32,
}

impl Noted {
    /// Whether the pane may have another one at `now`.
    fn allows(&self, now: Instant) -> bool {
        self.count < NOTE_MOST && now.duration_since(self.last) >= NOTE_GAP
    }
}

/// A notification's text as the desktop is given it. The desktop reads a
/// few tags in it, and a program's text is not written in them: `a < b`
/// is shown as it stands.
fn as_body(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

struct Tab {
    id: usize,
    root: Node,
    /// The pane that has, or last had, the keyboard.
    active: Pane,
    /// The active pane fills the tab, the others wait behind it.
    zoomed: bool,
    /// What happened in it while it wasn't looked at.
    news: Option<News>,
}

/// What a tab that isn't looked at has to tell, the least pressing first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum News {
    /// A program rang the bell, or has something to tell.
    Bell,
    /// A command ended.
    Done,
    /// A command failed.
    Failed,
}

/// A divider being dragged.
struct Drag {
    split: usize,
    /// The divider is between child `index` and the next.
    index: usize,
    axis: Axis,
}

pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
    drag: Option<Drag>,
    /// Where each split is on screen, as of the last frame: what a dragged
    /// divider's position is measured against.
    split_bounds: Rc<RefCell<HashMap<usize, Bounds<Pixels>>>>,
    subscriptions: HashMap<gpui::EntityId, Subscription>,
    /// A shell that is already running, for the next tab.
    spare: Option<Spare>,
    /// The wait before the spare is looked at again.
    spare_refresh: Option<Task<()>>,
    /// Something said in passing at the bottom of the window, and the wait
    /// until it goes away.
    notice: Option<SharedString>,
    notice_end: Option<Task<()>>,
    /// The window's title as the desktop was last told: the active tab's.
    title: String,
    /// The shells of its panes, as pulse was last told.
    shells: Vec<u32>,
    /// The notifications each pane had on the desktop.
    noted: HashMap<EntityId, Noted>,
    /// The folder the window was opened in: where `Ctrl+Shift+N` opens the next one.
    home: Option<PathBuf>,
    /// The environment of the launch that opened the window, which every terminal in it
    /// gets: None for lumen's own.
    env: Option<Env>,
    _observers: Vec<Subscription>,
}

/// A terminal that was started before it was asked for, so that the tab it
/// becomes shows a prompt at once: a shell with a greeting and plugins can
/// take a tenth of a second to get there, which is a visible pop.
struct Spare {
    pane: Pane,
    /// The folder it was started in. It only fits a tab that would start there.
    cwd: Option<PathBuf>,
    /// The environment it was started with. It only fits a tab with the same one.
    env: Option<Env>,
    born: Instant,
    /// The window's size when it was started, which its output is laid out for.
    viewport: Size<Pixels>,
}

/// A spare older than this shows a greeting that has gone stale.
const SPARE_MAX_AGE: Duration = Duration::from_secs(300);

pub const TAB_BAR: f32 = 40.;

impl Workspace {
    pub fn new(spawn: Spawn, window: &mut Window, cx: &mut Context<Self>) -> std::io::Result<Self> {
        let mut this = Self::empty(&spawn, window, cx);
        let pane = this.open(spawn, (1., 1.), true, window, cx)?;
        this.push_tab(pane, window, cx);
        this.refresh_spare_soon(window, cx);
        Ok(this)
    }

    /// A window without a tab yet: what `new` starts from.
    pub fn empty(spawn: &Spawn, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Workspace {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
            drag: None,
            split_bounds: Rc::default(),
            subscriptions: HashMap::new(),
            spare: None,
            spare_refresh: None,
            notice: None,
            notice_end: None,
            title: String::new(),
            shells: Vec::new(),
            noted: HashMap::new(),
            home: spawn.cwd.clone(),
            env: spawn.env.clone(),
            _observers: vec![
                // a spare is laid out for the window's size: a resized window wants a new one
                cx.observe_window_bounds(window, |this, window, cx| this.refresh_spare_soon(window, cx)),
                cx.observe_global_in::<Config>(window, Self::config_changed),
                // back at the window, what its tab had to tell has been seen
                cx.observe_window_activation(window, |this, window, cx| {
                    if !window.is_window_active() {
                        return;
                    }
                    // the window a launch opens its tab in
                    crate::instance::note_active(window.window_handle(), cx);
                    this.looked_at(this.active);
                    if let Some(tab) = this.tabs.get_mut(this.active)
                        && tab.news.take().is_some()
                    {
                        cx.notify();
                    }
                }),
            ],
        };
        if let Some(monitor) = cx.try_global::<Pulse>().map(|pulse| pulse.0.clone()) {
            // what pulse says is shown on the tabs and in the title bar
            this._observers.push(cx.observe(&monitor, |this, _, cx| {
                this.sample(cx);
                cx.notify();
            }));
            let id = cx.entity_id();
            this._observers.push(cx.on_release(move |_, cx| {
                if let Some(monitor) = cx.try_global::<Pulse>().map(|pulse| pulse.0.clone()) {
                    monitor.update(cx, |monitor, _| monitor.forget(id));
                }
            }));
        }
        this
    }

    /// Starts a terminal and wires it to this window.
    /// `share` is the part of the window's width and height the pane is
    /// about to get, and `tab` whether it gets a tab of its own.
    fn open(
        &mut self,
        spawn: Spawn,
        share: (f32, f32),
        tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::io::Result<Pane> {
        let config = cx.global::<Config>();
        let viewport = window.viewport_size();
        // the tab bar as it will be once the pane is there
        let bar = if config.tab_bar(self.tabs.len() + usize::from(tab)) { px(TAB_BAR) } else { px(0.) };
        let area = size(viewport.width * share.0, (viewport.height - bar) * share.1);
        let grid = Metrics::new(config, 1., window).grid(area, config);
        let started = Terminal::start(spawn, config, grid)?;
        let pane = cx.new(|cx| TerminalView::new(started, window, cx));
        let subscription = cx.subscribe_in(&pane, window, |this, pane, event: &ViewEvent, window, cx| match event {
            ViewEvent::Closed => this.close_pane(&pane.clone(), window, cx),
            // a shell says its title at every prompt, which is when its folder may have changed
            ViewEvent::Title => {
                this.refresh_spare_soon(window, cx);
                cx.notify();
            },
            // a shell that never sets a title still prints a prompt after `cd`
            ViewEvent::Output => {
                // the images the program replaced or scrolled away are freed here, drawn or not
                let images = pane.read(cx).terminal.read(cx).images.lock().unwrap().take_dropped(false);
                for image in images {
                    let _ = window.drop_image(image);
                }
                if this.spare.as_ref().is_some_and(|spare| spare.cwd != this.spawn_tab(cx).cwd) {
                    this.refresh_spare_soon(window, cx);
                }
            },
            ViewEvent::Notice(text) => this.say(text.clone(), cx),
            ViewEvent::Slash(Slash::Settings) => this.open_settings(window, cx),
            ViewEvent::Edit { path, line, column } => this.edit(path, *line, *column, window, cx),
            ViewEvent::Bell => this.tell(pane, News::Bell, window, cx),
            ViewEvent::Notify(note) => this.note(pane, note, window, cx),
            ViewEvent::Command(CommandEvent::Started) => cx.notify(),
            ViewEvent::Command(CommandEvent::Ended { failed }) => {
                this.tell(pane, if *failed { News::Failed } else { News::Done }, window, cx)
            },
            ViewEvent::Focused => {
                if let Some(tab) = this.tabs.iter_mut().find(|tab| tab.root.contains(pane))
                    && &tab.active != pane
                {
                    tab.active = pane.clone();
                    cx.notify();
                }
                this.refresh_spare_soon(window, cx);
            },
        });
        self.subscriptions.insert(pane.entity_id(), subscription);
        Ok(pane)
    }

    /// Opens a line of a file in the editor: in a tab if the editor runs in
    /// a terminal, in its own window otherwise. Without an editor the file
    /// goes to whatever the desktop opens it with.
    fn edit(&mut self, path: &Path, line: u32, column: Option<u32>, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.global::<Config>().editor.clone();
        let launched = self.pane_env(cx);
        let env = |name: &str| match &launched {
            Some(env) => env.iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()),
            None => std::env::var(name).ok(),
        };
        let Some(launch) = editor::launch(editor.command.as_deref(), editor.terminal, env, path, line, column) else {
            cx.open_with_system(path);
            return;
        };
        let program = launch.argv[0].clone();
        let started = if launch.terminal {
            let spawn =
                Spawn { cwd: path.parent().map(Path::to_path_buf), command: Some(launch.argv), env: launched.clone() };
            self.open(spawn, (1., 1.), true, window, cx).map(|pane| self.push_tab(pane, window, cx))
        } else {
            let mut command = std::process::Command::new(&program);
            command.args(&launch.argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            // on the display this window is on: a lumen started without one (the daemon) has none of its own
            if let Some(env) = &launched {
                command.envs(env.iter().map(|(key, value)| (key, value)));
            }
            if let Some(graphical) = cx.graphical_environment() {
                graphical.apply_to(&mut command);
            }
            command.spawn().map(|mut child| {
                // someone has to hear of its end, or it stays behind as a dead process
                std::thread::spawn(move || child.wait());
            })
        };
        if let Err(e) = started {
            self.say(format!("Can't start {program}: {e}"), cx);
        }
    }

    /// Something happened in a pane. Its tab keeps it as news, unless the
    /// tab is the one being looked at.
    fn tell(&mut self, pane: &Pane, news: News, window: &mut Window, cx: &mut Context<Self>) {
        let looked_at = window.is_window_active() || crate::preview();
        let active = self.active;
        if let Some((index, tab)) = self.tabs.iter_mut().enumerate().find(|(_, tab)| tab.root.contains(pane))
            && !(looked_at && index == active)
        {
            tab.news = tab.news.max(Some(news));
        }
        cx.notify();
    }

    /// A program in a pane has something to tell the user. In the tab that
    /// is looked at, the program's own output says it. Any other tab gets a
    /// dot, and the desktop a notification that leads back to the pane.
    fn note(&mut self, pane: &Pane, note: &Note, window: &mut Window, cx: &mut Context<Self>) {
        let looked_at = window.is_window_active() || crate::preview();
        let Some(index) = self.tabs.iter().position(|tab| tab.root.contains(pane)) else {
            return;
        };
        if looked_at && index == self.active {
            return;
        }
        let tab = &mut self.tabs[index];
        tab.news = tab.news.max(Some(News::Bell));
        cx.notify();
        if !cx.global::<Config>().notifications {
            return;
        }
        let (id, now) = (pane.entity_id(), Instant::now());
        let had = self.noted.get(&id).copied();
        if had.is_some_and(|had| !had.allows(now)) {
            return;
        }
        self.noted.insert(id, Noted { last: now, count: had.map_or(0, |had| had.count) + 1 });
        // A text without a title is about what the pane is called: a name the program
        // in it chose, so it is cleaned like the program's own title.
        let title = if note.title.is_empty() {
            crate::commands::shown(&pane.read(cx).terminal.read(cx).title(), crate::commands::TITLE_MOST)
        } else {
            note.title.clone()
        };
        cx.show_system_notification(SystemNotification {
            tag: tag_of(id).into(),
            title: title.into(),
            body: as_body(&note.body).into(),
            actions: Vec::new(),
        });
    }

    /// A tab is looked at: its panes may tell the desktop again.
    fn looked_at(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let mut panes = Vec::new();
        tab.root.panes(&mut panes);
        for pane in panes {
            self.noted.remove(&pane.entity_id());
        }
    }

    /// Brings the pane a notification was about to the front. False if it
    /// is not one of this window's.
    pub fn show_pane(&mut self, tag: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let found = self.tabs.iter().enumerate().find_map(|(index, tab)| {
            let mut panes = Vec::new();
            tab.root.panes(&mut panes);
            panes.into_iter().find(|pane| tag_of(pane.entity_id()) == tag).map(|pane| (index, pane))
        });
        let Some((index, pane)) = found else {
            return false;
        };
        self.tabs[index].active = pane;
        self.activate(index, window, cx);
        window.activate_window();
        true
    }

    /// The shells of all panes in tabs: the processes pulse is asked about.
    fn shell_pids(&self, cx: &App) -> Vec<u32> {
        let mut panes = Vec::new();
        for tab in &self.tabs {
            tab.root.panes(&mut panes);
        }
        panes.iter().map(|pane| pane.read(cx).terminal.read(cx).shell_pid()).collect()
    }

    /// What pulse last said about a pane's shell and everything it started.
    fn pane_use(pane: &Pane, cx: &App) -> Option<Use> {
        let monitor = cx.try_global::<Pulse>()?.0.read(cx);
        monitor.of(pane.read(cx).terminal.read(cx).shell_pid()).cloned()
    }

    /// What a tab's panes use together. None where pulse has said nothing about any of them.
    fn tab_use(tab: &Tab, cx: &App) -> Option<Use> {
        let mut panes = Vec::new();
        tab.root.panes(&mut panes);
        panes
            .iter()
            .filter_map(|pane| Self::pane_use(pane, cx))
            .fold(None, |sum: Option<Use>, pane| Some(sum.unwrap_or_default().with(&pane)))
    }

    /// pulse has read the machine again: the commands that run keep the most
    /// memory their pane was seen with.
    fn sample(&mut self, cx: &mut Context<Self>) {
        let mut panes = Vec::new();
        for tab in &self.tabs {
            tab.root.panes(&mut panes);
        }
        for pane in panes {
            if let Some(found) = Self::pane_use(&pane, cx) {
                pane.read(cx).terminal.read(cx).commands.lock().unwrap().sample(found.memory);
            }
        }
    }

    /// The monitor itself, in a tab: what a click on the title bar's light opens.
    fn open_monitor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let program = cx.try_global::<Pulse>().and_then(|pulse| pulse.0.read(cx).program().map(Path::to_path_buf));
        let program = program.map_or_else(|| "pulse".to_string(), |program| program.to_string_lossy().into_owned());
        let spawn = Spawn { cwd: dirs::home_dir(), command: Some(vec![program]), env: self.pane_env(cx) };
        match self.open(spawn, (1., 1.), true, window, cx) {
            Ok(pane) => self.push_tab(pane, window, cx),
            Err(e) => self.say(format!("Can't start pulse: {e}"), cx),
        }
    }

    /// `lumen settings`, in a tab that closes when it ends: what `/settings` at a prompt
    /// opens. The window behind it shows each change as it is made.
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // after an update, the program this lumen runs from is gone: the new one is there
        let program = std::fs::read_link("/proc/self/exe")
            .or_else(|_| std::env::current_exe())
            .map(|exe| exe.to_string_lossy().trim_end_matches(" (deleted)").to_string())
            .unwrap_or_else(|_| "lumen".into());
        let command = vec![program, "settings".into(), "--in-tab".into()];
        let spawn = Spawn { cwd: dirs::home_dir(), command: Some(command), env: self.pane_env(cx) };
        match self.open(spawn, (1., 1.), true, window, cx) {
            Ok(pane) => self.push_tab(pane, window, cx),
            Err(e) => self.say(format!("Can't open the settings: {e}"), cx),
        }
    }

    fn push_tab(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        self.next_id += 1;
        let at = if self.tabs.is_empty() { 0 } else { self.active + 1 };
        self.tabs.insert(
            at,
            Tab { id: self.next_id, root: Node::Leaf(pane.clone()), active: pane, zoomed: false, news: None },
        );
        self.activate(at, window, cx);
    }

    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        self.tabs[index].news = None;
        // a tab that comes to the front in a window nobody is at has not been looked at
        if window.is_window_active() || crate::preview() {
            self.looked_at(index);
        }
        let focus = self.tabs[index].active.focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// The environment of the active pane: that of the launch it was opened for, which may
    /// not be the window's.
    fn pane_env(&self, cx: &App) -> Option<Env> {
        match self.tabs.get(self.active) {
            Some(tab) => tab.active.read(cx).terminal.read(cx).env.clone(),
            None => self.env.clone(),
        }
    }

    /// Where a terminal opened from here starts: the folder of the active pane, with its
    /// environment.
    fn spawn_here(&self, cx: &App) -> Spawn {
        let cwd = self.tabs.get(self.active).and_then(|tab| tab.active.read(cx).terminal.read(cx).cwd());
        Spawn { cwd, command: None, env: self.pane_env(cx) }
    }

    /// Where a new tab starts: in the folder `new_tab_folder` names, or where a split
    /// would.
    fn spawn_tab(&self, cx: &App) -> Spawn {
        let mut spawn = self.spawn_here(cx);
        if let Some(folder) = cx.global::<Config>().new_tab_folder.folder() {
            spawn.cwd = Some(folder);
        }
        spawn
    }

    /// What `Ctrl+Shift+N` opens from here: a window in the folder this one was opened in,
    /// with the active pane's environment.
    pub fn spawn_window(&self, cx: &App) -> Spawn {
        Spawn { cwd: self.home.clone(), command: None, env: self.pane_env(cx) }
    }

    /// The pane that is in front.
    pub fn active_pane(&self) -> Option<Entity<TerminalView>> {
        self.tabs.get(self.active).map(|tab| tab.active.clone())
    }

    /// A tab for a launch of lumen: what `lumen` started while this one runs asked for. A
    /// shell that was started ahead of time is taken where it fits, as for `Ctrl+Shift+T`.
    pub fn open_launched(
        &mut self,
        spawn: Spawn,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::io::Result<Entity<TerminalView>> {
        let ready = if spawn.command.is_none() { self.take_spare(&spawn) } else { None };
        let pane = match ready {
            Some(pane) => pane,
            None => self.open(spawn, (1., 1.), true, window, cx)?,
        };
        self.push_tab(pane.clone(), window, cx);
        self.refresh_spare_soon(window, cx);
        Ok(pane)
    }

    fn report(&self, what: &str, error: std::io::Error) {
        eprintln!("lumen: {what}: {error}");
    }

    /// Shows a line at the bottom of the window for a moment.
    fn say(&mut self, text: String, cx: &mut Context<Self>) {
        // long enough to be read: an error takes longer than "Copied"
        let shown = Duration::from_millis((1800 + 45 * text.chars().count().saturating_sub(24) as u64).min(9000));
        self.notice = Some(text.into());
        self.notice_end = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(shown).await;
            let _ = this.update(cx, |this, cx| {
                this.notice = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// The config file was saved: what the window itself keeps of it is renewed.
    fn config_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = cx.global::<Config>();
        window.set_background_appearance(config.appearance());
        // asked again only when it changed: a desktop that draws no frames keeps saying so
        let has = match window.window_decorations() {
            Decorations::Server => WindowDecorations::Server,
            Decorations::Client { .. } => WindowDecorations::Client,
        };
        if config.decorations() != has {
            window.request_decorations(config.decorations());
        }
        let said = match &config.problem {
            Some(problem) => problem.clone(),
            None => "Config reloaded".to_string(),
        };
        // the spare was started with the old shell and settings
        if let Some(old) = self.spare.take() {
            self.forget(&old.pane, window, cx);
        }
        self.refresh_spare_soon(window, cx);
        self.say(said, cx);
    }

    /// Lets go of a pane that is closing: what the window holds for it.
    fn forget(&mut self, pane: &Pane, window: &mut Window, cx: &mut Context<Self>) {
        self.subscriptions.remove(&pane.entity_id());
        self.noted.remove(&pane.entity_id());
        let images = pane.read(cx).terminal.read(cx).images.lock().unwrap().take_dropped(true);
        for image in images {
            let _ = window.drop_image(image);
        }
    }

    /// The shell that was started ahead of time, if it was started where `spawn` would
    /// start one. One that doesn't fit stays for the next tab, which it may fit (a split
    /// starts in its pane's folder, a new tab where `new_tab_folder` says).
    fn take_spare(&mut self, spawn: &Spawn) -> Option<Pane> {
        let fits = self.spare.as_ref().is_some_and(|spare| spare.cwd == spawn.cwd && spare.env == spawn.env);
        fits.then(|| self.spare.take().map(|spare| spare.pane)).flatten()
    }

    fn new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        let spawn = self.spawn_tab(cx);
        let ready = self.take_spare(&spawn);
        match ready.map(Ok).unwrap_or_else(|| self.open(spawn, (1., 1.), true, window, cx)) {
            Ok(pane) => self.push_tab(pane, window, cx),
            Err(e) => self.report("can't open a tab", e),
        }
        self.refresh_spare_soon(window, cx);
    }

    /// Looks at the spare again in a moment: after the things that can make
    /// it unfit come in a burst (typing `cd`, dragging the window's edge).
    fn refresh_spare_soon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !cx.global::<Config>().prewarm {
            return;
        }
        self.spare_refresh = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(400)).await;
            let _ = this.update_in(cx, |this, window, cx| this.refresh_spare(window, cx));
        }));
    }

    /// Makes sure a spare is running that fits the next tab: started in the
    /// folder a new tab starts in, for the window's current size, not long ago.
    fn refresh_spare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let spawn = self.spawn_tab(cx);
        let viewport = window.viewport_size();
        let fits = self.spare.as_ref().is_some_and(|spare| {
            spare.cwd == spawn.cwd
                && spare.env == spawn.env
                && spare.born.elapsed() < SPARE_MAX_AGE
                && spare.viewport == viewport
        });
        if fits {
            return;
        }
        if let Some(old) = self.spare.take() {
            self.forget(&old.pane, window, cx);
        }
        let (cwd, env) = (spawn.cwd.clone(), spawn.env.clone());
        match self.open(spawn, (1., 1.), true, window, cx) {
            Ok(pane) => self.spare = Some(Spare { pane, cwd, env, born: Instant::now(), viewport }),
            Err(e) => self.report("can't start a shell ahead of time", e),
        }
    }

    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        // the new pane gets half of the active one
        let mut rects = Vec::new();
        if let Some(tab) = self.tabs.get(self.active) {
            tab.root.rects((0., 0., 1., 1.), &mut rects);
        }
        let (w, h) = self
            .tabs
            .get(self.active)
            .and_then(|tab| rects.iter().find(|(pane, _)| pane == &tab.active))
            .map_or((1., 1.), |(_, (_, _, w, h))| (*w, *h));
        let share = match axis {
            Axis::Horizontal => (w / 2., h),
            Axis::Vertical => (w, h / 2.),
        };
        // the spare was laid out for a whole tab; its shell is told the new size like any resized one
        let spawn = self.spawn_here(cx);
        let ready = self.take_spare(&spawn);
        let pane = match ready.map(Ok).unwrap_or_else(|| self.open(spawn, share, false, window, cx)) {
            Ok(pane) => pane,
            Err(e) => return self.report("can't split", e),
        };
        self.refresh_spare_soon(window, cx);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let target = tab.active.clone();
        tab.root.split(&target, pane.clone(), axis, &mut self.next_id);
        tab.active = pane.clone();
        tab.zoomed = false;
        window.focus(&pane.focus_handle(cx), cx);
        cx.notify();
    }

    fn close_pane(&mut self, pane: &Pane, window: &mut Window, cx: &mut Context<Self>) {
        self.forget(pane, window, cx);
        if self.spare.as_ref().is_some_and(|spare| &spare.pane == pane) {
            // the spare's shell ended on its own
            self.spare = None;
            return;
        }
        let Some(index) = self.tabs.iter().position(|tab| tab.root.contains(pane)) else {
            return;
        };
        let tab = &mut self.tabs[index];
        if !tab.root.remove(pane) {
            return self.close_tab_at(index, window, cx);
        }
        tab.zoomed = false;
        if &tab.active == pane {
            let mut panes = Vec::new();
            tab.root.panes(&mut panes);
            tab.active = panes.remove(0);
        }
        if index == self.active {
            let focus = self.tabs[index].active.focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn close_tab_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tabs.remove(index);
        let mut panes = Vec::new();
        tab.root.panes(&mut panes);
        for pane in panes {
            self.forget(&pane, window, cx);
        }
        if self.tabs.is_empty() {
            window.remove_window();
            return;
        }
        let next = if index < self.active || self.active >= self.tabs.len() {
            self.active.saturating_sub(1)
        } else {
            self.active
        };
        self.activate(next, window, cx);
    }

    fn step_tab(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.tabs.len() as isize;
        let next = (self.active as isize + by).rem_euclid(count) as usize;
        self.activate(next, window, cx);
    }

    fn move_tab(&mut self, by: isize, cx: &mut Context<Self>) {
        let to = self.active as isize + by;
        if to < 0 || to >= self.tabs.len() as isize {
            return;
        }
        self.tabs.swap(self.active, to as usize);
        self.active = to as usize;
        cx.notify();
    }

    /// Puts the tab `moved` where the tab `onto` is.
    fn reorder(&mut self, moved: usize, onto: usize, cx: &mut Context<Self>) {
        let place = |tabs: &[Tab], id: usize| tabs.iter().position(|tab| tab.id == id);
        let (Some(from), Some(to)) = (place(&self.tabs, moved), place(&self.tabs, onto)) else {
            return;
        };
        let shown = self.tabs[self.active].id;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = place(&self.tabs, shown).unwrap_or(0);
        cx.notify();
    }

    /// Moves the keyboard to the nearest pane in a direction, (dx, dy)
    /// being one of the four unit steps.
    fn focus_toward(&mut self, dx: f32, dy: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut rects = Vec::new();
        tab.root.rects((0., 0., 1., 1.), &mut rects);
        let Some(&(_, (x, y, w, h))) = rects.iter().find(|(pane, _)| pane == &tab.active) else {
            return;
        };
        let (cx0, cy0) = (x + w / 2., y + h / 2.);
        let best = rects
            .iter()
            .filter(|(pane, _)| pane != &tab.active)
            .filter_map(|(pane, (px0, py0, pw, ph))| {
                // how far the pane starts beyond this one's edge, in the direction asked for
                let ahead = if dx > 0. {
                    px0 - (x + w)
                } else if dx < 0. {
                    x - (px0 + pw)
                } else if dy > 0. {
                    py0 - (y + h)
                } else {
                    y - (py0 + ph)
                };
                if ahead < -0.001 {
                    return None;
                }
                // and how far off to the side its middle is
                let aside = if dx != 0. { (py0 + ph / 2. - cy0).abs() } else { (px0 + pw / 2. - cx0).abs() };
                // a pane that overlaps this one sideways is a neighbour: prefer it
                let overlaps = if dx != 0. { *py0 < y + h && py0 + ph > y } else { *px0 < x + w && px0 + pw > x };
                Some((!overlaps, ahead + aside, pane.clone()))
            })
            .min_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((_, _, pane)) = best {
            tab.active = pane.clone();
            tab.zoomed = false;
            window.focus(&pane.focus_handle(cx), cx);
            cx.notify();
        }
    }

    fn drag_divider(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &self.drag else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let Some(bounds) = self.split_bounds.borrow().get(&drag.split).copied() else {
            return;
        };
        let (at, extent) = match drag.axis {
            Axis::Horizontal => (event.position.x - bounds.origin.x, bounds.size.width),
            Axis::Vertical => (event.position.y - bounds.origin.y, bounds.size.height),
        };
        if extent <= px(0.) {
            return;
        }
        let at = at / extent;
        let (index, split) = (drag.index, drag.split);
        let Some(shares) = self.tabs.get_mut(self.active).and_then(|tab| tab.root.shares_mut(split)) else {
            return;
        };
        // the divider moves between its two neighbours; neither gets smaller than a sliver
        let before: f32 = shares[..index].iter().sum();
        let pair = shares[index] + shares[index + 1];
        let min = 0.05f32.min(pair / 2.);
        let first = (at - before).clamp(min, pair - min);
        shares[index] = first;
        shares[index + 1] = pair - first;
        cx.notify();
    }

    fn render_node(&self, node: &Node, tab: &Tab, cx: &mut Context<Self>) -> AnyElement {
        match node {
            Node::Leaf(pane) => {
                let dim = pane != &tab.active;
                div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .child(pane.clone())
                    .when(dim, |this| {
                        // the panes that don't have the keyboard step back
                        this.child(div().absolute().inset_0().bg(gpui::black().opacity(0.22)))
                    })
                    .into_any_element()
            },
            Node::Split { id, axis, children, shares } => {
                let (id, axis) = (*id, *axis);
                let bounds = self.split_bounds.clone();
                let mut row = div().relative().flex().size_full().child(
                    canvas(move |b, _, _| _ = bounds.borrow_mut().insert(id, b), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                );
                row = match axis {
                    Axis::Horizontal => row.flex_row(),
                    Axis::Vertical => row.flex_col(),
                };
                for (i, (child, share)) in children.iter().zip(shares).enumerate() {
                    if i > 0 {
                        row = row.child(self.render_divider(id, i - 1, axis, cx));
                    }
                    row = row.child(
                        div()
                            .flex_basis(px(0.))
                            .flex_grow(*share)
                            .flex_shrink(1.)
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(self.render_node(child, tab, cx)),
                    );
                }
                row.into_any_element()
            },
        }
    }

    /// A hairline between two panes, with a wider strip around it to grab.
    fn render_divider(&self, split: usize, index: usize, axis: Axis, cx: &mut Context<Self>) -> AnyElement {
        let line = gpui::white().opacity(0.08);
        let grab = div().id(SharedString::from(format!("divider-{split}-{index}"))).absolute().occlude().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.drag = Some(Drag { split, index, axis });
                cx.stop_propagation();
            }),
        );
        match axis {
            Axis::Horizontal => div()
                .relative()
                .flex_none()
                .w(px(1.))
                .h_full()
                .bg(line)
                .child(grab.top_0().bottom_0().left(px(-3.)).w(px(7.)).cursor(CursorStyle::ResizeColumn)),
            Axis::Vertical => div()
                .relative()
                .flex_none()
                .h(px(1.))
                .w_full()
                .bg(line)
                .child(grab.left_0().right_0().top(px(-3.)).h(px(7.)).cursor(CursorStyle::ResizeRow)),
        }
        .into_any_element()
    }

    fn render_tab(&self, index: usize, tab: &Tab, accent: Hsla, cx: &mut Context<Self>) -> AnyElement {
        let active = index == self.active;
        let title = tab.active.read(cx).terminal.read(cx).title();
        let id = tab.id;
        let text = gpui::white().opacity(if active { 0.92 } else { 0.5 });
        // A tab that isn't looked at says what happened in it, or that a command still runs there.
        let colors = &cx.global::<Config>().colors;
        let dot = match tab.news {
            Some(News::Failed) => Some(colors.normal.red.hsla()),
            Some(News::Done) => Some(colors.normal.green.hsla()),
            Some(News::Bell) => Some(accent),
            None => {
                let mut panes = Vec::new();
                tab.root.panes(&mut panes);
                let running =
                    |pane: &Pane| pane.read(cx).terminal.read(cx).commands.lock().unwrap().running().is_some();
                (!active && panes.iter().any(running)).then(|| gpui::white().opacity(0.32))
            },
        };
        // what the tab's panes use, where pulse tells: shown when it is worth a look
        let usage = Self::tab_use(tab, cx);
        let meter = usage.as_ref().filter(|usage| usage.notable()).map(|usage| {
            let soft = gpui::white().opacity(if active { 0.5 } else { 0.38 });
            // from two cores on, the tab is at work: its number is lit
            let work = if usage.cores >= 2. { accent.opacity(0.95) } else { soft };
            let memory = usage.memory as f64 / (1u64 << 30) as f64;
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.))
                .text_size(px(11.))
                .text_color(soft)
                .child(svg().path("icons/pulse.svg").size(px(11.)).text_color(work))
                .when(usage.cores >= 0.2, |this| {
                    this.child(div().text_color(work).child(SharedString::from(format!("{:.1}", usage.cores))))
                })
                .when(memory >= 1., |this| this.child(SharedString::from(format!("{memory:.1}G"))))
        });
        // a shell that runs nothing has nothing to say
        let tip = usage.filter(|usage| usage.notable() || usage.job.is_some()).map(|usage| {
            let mut lines = vec![SharedString::from(usage.describe())];
            if usage.processes > 1 {
                lines.push(format!("{} processes", usage.processes).into());
            }
            lines
        });
        div()
            .id(SharedString::from(format!("tab-{id}")))
            .group("tab")
            .relative()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(28.))
            .min_w(px(96.))
            .max_w(px(220.))
            .flex_basis(px(180.))
            .flex_shrink(1.)
            .pl(px(12.))
            .pr(px(6.))
            .rounded(px(8.))
            .text_size(px(12.5))
            .text_color(text)
            .when(active, |this| this.bg(gpui::white().opacity(0.09)))
            .when(!active, |this| this.hover(|style| style.bg(gpui::white().opacity(0.045))))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if let Some(at) = this.tabs.iter().position(|tab| tab.id == id) {
                        this.activate(at, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    if let Some(at) = this.tabs.iter().position(|tab| tab.id == id) {
                        this.close_tab_at(at, window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            // a tab is dragged to another one's place
            .on_drag(DraggedTab { id, title: title.clone().into() }, |tab, _, _, cx| {
                cx.new(|_| TabGhost(tab.title.clone()))
            })
            .drag_over::<DraggedTab>(|style, _, _, _| style.bg(gpui::white().opacity(0.14)))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| this.reorder(dragged.id, id, cx)))
            .children(dot.map(|color| div().flex_none().size(px(6.)).rounded(px(3.)).bg(color)))
            .child(div().flex_1().min_w_0().truncate().child(SharedString::from(title)))
            .children(meter)
            .when_some(tip, |this, lines| this.tooltip(move |_, cx| cx.new(|_| Tip(lines.clone())).into()))
            .child(
                div()
                    .id(SharedString::from(format!("tab-close-{id}")))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(18.))
                    .rounded(px(5.))
                    .when(!active, |this| this.invisible().group_hover("tab", |style| style.visible()))
                    .hover(|style| style.bg(gpui::white().opacity(0.12)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(at) = this.tabs.iter().position(|tab| tab.id == id) {
                                this.close_tab_at(at, window, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(svg().path("icons/close.svg").size(px(10.)).text_color(text)),
            )
            .when(active, |this| {
                // the lit tab: a line of the accent's light under it
                this.child(
                    div().absolute().left(px(14.)).right(px(14.)).bottom(px(-6.)).h(px(2.)).rounded(px(1.)).bg(accent),
                )
            })
            .into_any_element()
    }

    fn render_tab_bar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let accent = cx.global::<Config>().accent();
        let frame = Frame::of(window, cx);
        let client = frame.client;
        let controls = window.window_controls();
        let icon = gpui::white().opacity(0.6);
        let soft = gpui::white().opacity(0.08);
        let button = |id: &'static str, path: &'static str, lit: Hsla| {
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded(px(8.))
                .hover(move |style| style.bg(lit))
                .child(svg().path(path).size(px(12.)).text_color(icon))
        };
        let mut bar = div().id("tab-bar").flex().flex_none().items_center().gap(px(4.)).h(px(TAB_BAR)).px(px(8.)).when(
            frame.moves,
            |this| {
                this.on_mouse_down(MouseButton::Left, |event, window, _| {
                    if event.click_count == 2 {
                        window.zoom_window();
                    } else {
                        window.start_window_move();
                    }
                })
            },
        );
        for (index, tab) in self.tabs.iter().enumerate() {
            bar = bar.child(self.render_tab(index, tab, accent, cx));
        }
        bar = bar.child(button("new-tab", "icons/plus.svg", soft).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.new_tab(&NewTab, window, cx);
                cx.stop_propagation();
            }),
        ));
        bar = bar.child(div().flex_1());
        // The machine's light: lit when pulse calls the machine busy.
        let machine = cx.try_global::<Pulse>().and_then(|pulse| {
            let reading = pulse.0.read(cx).reading()?;
            Some((
                reading.busy,
                vec![reading.line.clone().into(), reading.machine().into(), "click opens pulse".into()],
            ))
        });
        if let Some((busy, lines)) = machine {
            let lines: Vec<SharedString> = lines;
            let light = div().size(px(7.)).rounded(px(3.5)).map(|this| {
                if busy {
                    this.bg(accent).shadow(vec![BoxShadow {
                        color: accent.opacity(0.55),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(7.),
                        spread_radius: px(1.),
                        inset: false,
                    }])
                } else {
                    this.bg(gpui::white().opacity(0.22))
                }
            });
            bar = bar.child(
                div()
                    .id("machine")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(28.))
                    .rounded(px(8.))
                    .hover(move |style| style.bg(soft))
                    .tooltip(move |_, cx| cx.new(|_| Tip(lines.clone())).into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.open_monitor(window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(light),
            );
        }
        if client {
            if controls.minimize {
                bar = bar.child(button("minimize", "icons/minimize.svg", soft).on_mouse_down(
                    MouseButton::Left,
                    |_, window, cx| {
                        window.minimize_window();
                        cx.stop_propagation();
                    },
                ));
            }
            if controls.maximize {
                let path = if window.is_maximized() { "icons/restore.svg" } else { "icons/maximize.svg" };
                bar = bar.child(button("maximize", path, soft).on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.zoom_window();
                    cx.stop_propagation();
                }));
            }
            bar = bar.child(button("close", "icons/close.svg", gpui::rgba(0xe5484dcc).into()).on_mouse_down(
                MouseButton::Left,
                |_, window, cx| {
                    window.remove_window();
                    cx.stop_propagation();
                },
            ));
        }
        bar.into_any_element()
    }
}

/// How the window is framed. Where lumen draws its own frame (Wayland, a
/// composited X11) the window has rounded corners and casts a shadow, which
/// takes a margin around it that the desktop is told about.
#[derive(Clone, Copy)]
struct Frame {
    /// lumen draws the title bar's buttons and the window's edges.
    client: bool,
    /// The desktop draws no title bar: the tab bar is where the window is
    /// taken to move it, whether lumen draws a frame or none.
    moves: bool,
    /// The edges that are against a screen edge or another tiled window:
    /// no margin, shadow or rounding there.
    tiling: Tiling,
}

/// The margin the shadow is drawn in, which is also where the window is
/// grabbed to resize it.
const SHADOW: f32 = 12.;
const RADIUS: f32 = 12.;

impl Frame {
    fn of(window: &Window, cx: &App) -> Frame {
        let none = cx.global::<Config>().window.decorations == crate::config::Decorations::None;
        let frame = match window.window_decorations() {
            Decorations::Client { tiling } => {
                let tiling = if window.is_maximized() || window.is_fullscreen() { Tiling::tiled() } else { tiling };
                Frame { client: true, moves: true, tiling }
            },
            Decorations::Server if crate::preview() => Frame { client: true, moves: true, tiling: Tiling::default() },
            Decorations::Server => Frame { client: false, moves: false, tiling: Tiling::tiled() },
        };
        // a window without a frame is drawn as one against every edge: no margin, no corners
        if none { Frame { client: false, tiling: Tiling::tiled(), ..frame } } else { frame }
    }

    fn floating(&self) -> bool {
        self.client && !self.tiling.is_tiled()
    }
}

/// The strips around a framed window that resize it.
fn resize_handles(frame: Frame) -> Option<AnyElement> {
    if !frame.client {
        return None;
    }
    let handle = |id: &'static str, edge: ResizeEdge, cursor: CursorStyle| {
        div().id(id).absolute().occlude().cursor(cursor).on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.start_window_resize(edge);
            cx.stop_propagation();
        })
    };
    let t = frame.tiling;
    let (thin, corner) = (px(SHADOW), px(SHADOW * 2.));
    let mut handles = div().absolute().size_full();
    if !t.top {
        handles = handles.child(
            handle("resize-top", ResizeEdge::Top, CursorStyle::ResizeUp).top_0().left(corner).right(corner).h(thin),
        );
    }
    if !t.bottom {
        handles = handles.child(
            handle("resize-bottom", ResizeEdge::Bottom, CursorStyle::ResizeDown)
                .bottom_0()
                .left(corner)
                .right(corner)
                .h(thin),
        );
    }
    if !t.left {
        handles = handles.child(
            handle("resize-left", ResizeEdge::Left, CursorStyle::ResizeLeft)
                .left_0()
                .top(corner)
                .bottom(corner)
                .w(thin),
        );
    }
    if !t.right {
        handles = handles.child(
            handle("resize-right", ResizeEdge::Right, CursorStyle::ResizeRight)
                .right_0()
                .top(corner)
                .bottom(corner)
                .w(thin),
        );
    }
    if !t.top && !t.left {
        handles = handles.child(
            handle("resize-top-left", ResizeEdge::TopLeft, CursorStyle::ResizeUpLeftDownRight)
                .top_0()
                .left_0()
                .size(corner),
        );
    }
    if !t.top && !t.right {
        handles = handles.child(
            handle("resize-top-right", ResizeEdge::TopRight, CursorStyle::ResizeUpRightDownLeft)
                .top_0()
                .right_0()
                .size(corner),
        );
    }
    if !t.bottom && !t.left {
        handles = handles.child(
            handle("resize-bottom-left", ResizeEdge::BottomLeft, CursorStyle::ResizeUpRightDownLeft)
                .bottom_0()
                .left_0()
                .size(corner),
        );
    }
    if !t.bottom && !t.right {
        handles = handles.child(
            handle("resize-bottom-right", ResizeEdge::BottomRight, CursorStyle::ResizeUpLeftDownRight)
                .bottom_0()
                .right_0()
                .size(corner),
        );
    }
    Some(handles.into_any_element())
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = cx.global::<Config>();
        let background = Hsla { a: config.window.opacity, ..config.colors.primary.background.hsla() };
        let frame = Frame::of(window, cx);
        let t = frame.tiling;
        // the corners that are free are round; the desktop is told, so that what it
        // draws behind the window (the blur) has the window's shape
        let radius = |free: bool| if frame.client && free { px(RADIUS) } else { px(0.) };
        let corners = Corners {
            top_left: radius(!t.top && !t.left),
            top_right: radius(!t.top && !t.right),
            bottom_left: radius(!t.bottom && !t.left),
            bottom_right: radius(!t.bottom && !t.right),
        };
        // the window is named after its active tab, for the desktop's task bar and window list
        let title = self.tabs.get(self.active).map(|tab| tab.active.read(cx).terminal.read(cx).title());
        if let Some(title) = title.filter(|title| *title != self.title) {
            window.set_window_title(&title);
            self.title = title;
        }
        // pulse is told which shells there are, whenever panes have come or gone
        let shells = self.shell_pids(cx);
        if shells != self.shells {
            self.shells = shells.clone();
            if let Some(monitor) = cx.try_global::<Pulse>().map(|pulse| pulse.0.clone()) {
                let id = cx.entity_id();
                monitor.update(cx, |monitor, _| monitor.want(id, shells));
            }
        }
        window.set_client_inset(px(if frame.floating() { SHADOW } else { 0. }));
        window.set_client_corner_radii(corners);

        let content = match self.tabs.get(self.active) {
            Some(tab) if tab.zoomed => tab.active.clone().into_any_element(),
            Some(tab) => self.render_node(&tab.root, tab, cx),
            None => div().into_any_element(),
        };
        let bar = cx.global::<Config>().tab_bar(self.tabs.len()).then(|| self.render_tab_bar(window, cx));

        let surface = div()
            .id("workspace")
            .key_context("Workspace")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(background)
            .font_family(crate::fonts::ui())
            .rounded_tl(corners.top_left)
            .rounded_tr(corners.top_right)
            .rounded_bl(corners.bottom_left)
            .rounded_br(corners.bottom_right)
            .when(frame.client, |this| this.border_1().border_color(gpui::white().opacity(0.1)))
            .when(frame.floating(), |this| {
                this.shadow(vec![
                    BoxShadow {
                        color: gpui::black().opacity(0.35),
                        offset: point(px(0.), px(3.)),
                        blur_radius: px(SHADOW),
                        spread_radius: px(0.),
                        inset: false,
                    },
                    BoxShadow {
                        color: gpui::black().opacity(0.25),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(3.),
                        spread_radius: px(0.),
                        inset: false,
                    },
                ])
            })
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| this.close_tab_at(this.active, window, cx)))
            .on_action(cx.listener(|this, _: &ClosePane, window, cx| {
                if let Some(pane) = this.tabs.get(this.active).map(|tab| tab.active.clone()) {
                    this.close_pane(&pane, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.step_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.step_tab(-1, window, cx)))
            .on_action(cx.listener(|this, _: &MoveTabLeft, _, cx| this.move_tab(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveTabRight, _, cx| this.move_tab(1, cx)))
            .on_action(cx.listener(|this, action: &GoToTab, window, cx| {
                // 9 is the last tab, however many there are
                let index = if action.0 >= 9 { this.tabs.len() - 1 } else { action.0 - 1 };
                this.activate(index, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| this.split(Axis::Horizontal, window, cx)))
            .on_action(cx.listener(|this, _: &SplitDown, window, cx| this.split(Axis::Vertical, window, cx)))
            .on_action(cx.listener(|this, _: &FocusLeft, window, cx| this.focus_toward(-1., 0., window, cx)))
            .on_action(cx.listener(|this, _: &FocusRight, window, cx| this.focus_toward(1., 0., window, cx)))
            .on_action(cx.listener(|this, _: &FocusUp, window, cx| this.focus_toward(0., -1., window, cx)))
            .on_action(cx.listener(|this, _: &FocusDown, window, cx| this.focus_toward(0., 1., window, cx)))
            .on_action(cx.listener(|this, _: &OpenPulse, window, cx| this.open_monitor(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleZoomPane, _, cx| {
                if let Some(tab) = this.tabs.get_mut(this.active) {
                    tab.zoomed = !tab.zoomed && matches!(tab.root, Node::Split { .. });
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(Self::drag_divider))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.drag = None;
                }),
            )
            .children(bar)
            .child(div().flex_1().min_h_0().child(content))
            .children(self.notice.clone().map(|text| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(18.))
                    .px(px(24.))
                    .flex()
                    .justify_center()
                    .child(pill(text, cx.global::<Config>()).max_w(px(620.)))
            }));

        // the margin around a floating window holds its shadow and the strips that resize it
        let margin = |free: bool| px(if frame.client && free { SHADOW } else { 0. });
        div()
            .id("window")
            .relative()
            .size_full()
            .pt(margin(!t.top))
            .pb(margin(!t.bottom))
            .pl(margin(!t.left))
            .pr(margin(!t.right))
            .child(surface)
            .children(resize_handles(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_is_heard_so_often_and_no_more() {
        let start = Instant::now();
        let noted = Noted { last: start, count: 1 };
        assert!(!noted.allows(start + Duration::from_secs(1)));
        assert!(noted.allows(start + NOTE_GAP));
        // and then not at all, until its tab is looked at
        let noted = Noted { last: start, count: NOTE_MOST };
        assert!(!noted.allows(start + Duration::from_secs(3600)));
        assert!(Noted { last: start, count: NOTE_MOST - 1 }.allows(start + NOTE_GAP));
    }

    #[test]
    fn a_notification_shows_its_text_as_it_stands() {
        assert_eq!(as_body("done in 12 s"), "done in 12 s");
        assert_eq!(as_body("a < b && b > c"), "a &lt; b &amp;&amp; b &gt; c");
        assert_eq!(
            as_body("<a href=\"https://example.com\">here</a>"),
            "&lt;a href=\"https://example.com\"&gt;here&lt;/a&gt;"
        );
        assert_eq!(as_body("&lt;"), "&amp;lt;");
    }
}
