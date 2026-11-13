//! Where the time goes between a key and the frame that shows it.
//!
//! A lumen built with `--features lag` and started with `LUMEN_LAG=<file>`
//! writes one JSON line per event to the file: the keys pressed (which key,
//! never the text it typed), the bytes handed to the program and the bytes it
//! printed back, each frame's cursor, and GPUI's own journal of the window's
//! input, draws, presents and slow tasks, all on one clock.
//! `scripts/lag.py <file>` turns that into a report.
//!
//! With `LUMEN_LAG_DRIVE=1` as well, lumen types a fixed scenario into its
//! shell by itself (nothing is run: it never presses Enter) and quits after
//! it, so that two builds can be compared on the same keys. `scripts/lag.sh`
//! does that on a display nobody sees.
//!
//! A lumen that measures is a process of its own: it neither hands its launch
//! to a running lumen nor answers later launches.
//!
//! Without the feature, every function here is empty.

#[cfg(feature = "lag")]
pub use on::*;

#[cfg(not(feature = "lag"))]
pub use off::*;

#[cfg(not(feature = "lag"))]
mod off {
    use gpui::{App, Keystroke};

    pub fn enabled() -> bool {
        false
    }
    pub fn start(_: &mut App) {}
    pub fn drive(_: &mut App) {}
    pub fn key(_: &Keystroke, _: bool) {}
    pub fn sent(_: usize) {}
    pub fn written(_: usize) {}
    pub fn read(_: std::time::Instant, _: usize) {}
    pub fn woke() {}
    pub fn paint(_: &Paint) {}

    // the element fills it in, and only the recorder reads it
    #[allow(dead_code)]
    pub struct Paint {
        pub pane: u64,
        pub start: std::time::Instant,
        pub cursor: Option<(usize, usize)>,
        pub drawn: Option<(f32, f32)>,
        pub moving: bool,
        pub content: u64,
    }
}

#[cfg(feature = "lag")]
mod on {
    use std::fmt::Write as _;
    use std::fs::File;
    use std::io::{BufWriter, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    use gpui::profiler::journal::{
        ForegroundEvent, ForegroundJournalCollector, ForegroundJournalEntry, IntervalBoundary,
    };
    use gpui::{App, FrameRequestSource, Keystroke, OperatingSystem};

    use crate::keys::{self, Press};
    use crate::workspace::Workspace;

    static ON: AtomicBool = AtomicBool::new(false);
    static BASE: OnceLock<Instant> = OnceLock::new();
    /// What lumen noted since the writer last looked: lines without their time yet.
    static NOTED: Mutex<Vec<(Instant, String)>> = Mutex::new(Vec::new());
    static WRITER: Mutex<Option<Writer>> = Mutex::new(None);

    struct Writer {
        out: BufWriter<File>,
        journal: ForegroundJournalCollector,
    }

    /// Whether this lumen measures.
    pub fn enabled() -> bool {
        ON.load(Ordering::Relaxed)
    }

    /// Opens the file `LUMEN_LAG` names and starts the thread that writes to it.
    pub fn start(cx: &mut App) {
        let Some(path) = std::env::var_os("LUMEN_LAG").filter(|p| !p.is_empty()) else { return };
        let file = match File::create(&path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("lumen: can't write the lag record to {}: {e}", path.to_string_lossy());
                return;
            },
        };
        BASE.get_or_init(Instant::now);
        // the frame's first invalidation is only known while GPUI traces
        gpui::profiler::set_trace_enabled(true);
        let mut out = BufWriter::new(file);
        let _ = writeln!(
            out,
            r#"{{"e":"start","t":0,"version":"{}","drive":{}}}"#,
            env!("CARGO_PKG_VERSION"),
            std::env::var_os("LUMEN_LAG_DRIVE").is_some()
        );
        *WRITER.lock().unwrap() = Some(Writer { out, journal: cx.foreground_journal().collector() });
        ON.store(true, Ordering::Relaxed);
        let _ = std::thread::Builder::new().name("lag record".into()).spawn(|| {
            loop {
                std::thread::sleep(Duration::from_millis(50));
                flush();
            }
        });
        eprintln!("lumen: writing the lag record to {}", path.to_string_lossy());
    }

    /// Writes what was noted and journaled since the last time.
    pub fn flush() {
        let mut writer = WRITER.lock().unwrap();
        let Some(writer) = writer.as_mut() else { return };
        let mut lines: Vec<(Instant, String)> = std::mem::take(&mut *NOTED.lock().unwrap());
        for entry in writer.journal.collect_unseen().entries {
            if let Some(line) = journal_line(&entry) {
                lines.push(line);
            }
        }
        lines.sort_by_key(|(at, _)| *at);
        for (at, line) in lines {
            let _ = writeln!(writer.out, r#"{{"t":{},{line}}}"#, us(at));
        }
        let _ = writer.out.flush();
    }

    /// Microseconds since the record began.
    fn us(at: Instant) -> u64 {
        at.saturating_duration_since(*BASE.get().unwrap()).as_micros() as u64
    }

    fn dur(d: Duration) -> u64 {
        d.as_micros() as u64
    }

    fn note(line: String) {
        NOTED.lock().unwrap().push((Instant::now(), line));
    }

    /// A key reached the pane. A key that types text is only "char": what was typed stays out.
    pub fn key(keystroke: &Keystroke, repeat: bool) {
        if !enabled() {
            return;
        }
        let m = &keystroke.modifiers;
        let chord = m.control || m.alt || m.platform;
        let name = if !chord && keystroke.key.chars().count() == 1 {
            "char".to_string()
        } else {
            let mut name = String::new();
            for (held, prefix) in [(m.control, "ctrl-"), (m.alt, "alt-"), (m.platform, "super-"), (m.shift, "shift-")] {
                if held {
                    name.push_str(prefix);
                }
            }
            name.push_str(&keystroke.key);
            name
        };
        note(format!(r#""e":"key","key":{},"repeat":{repeat}"#, json(&name)));
    }

    /// Bytes were handed to the program's thread.
    pub fn sent(bytes: usize) {
        if enabled() {
            note(format!(r#""e":"sent","bytes":{bytes}"#));
        }
    }

    /// The program's thread wrote bytes to the PTY.
    pub fn written(bytes: usize) {
        if enabled() {
            note(format!(r#""e":"written","bytes":{bytes}"#));
        }
    }

    /// The program's thread read and parsed what the program printed, from `start` until now.
    pub fn read(start: Instant, bytes: usize) {
        if enabled() {
            note(format!(r#""e":"read","bytes":{bytes},"from":{}"#, us(start)));
        }
    }

    /// The window heard that the grid changed.
    pub fn woke() {
        if enabled() {
            note(r#""e":"wake""#.to_string());
        }
    }

    /// One pane's grid, as one frame draws it.
    pub struct Paint {
        pub pane: u64,
        /// When the pane's prepaint began.
        pub start: Instant,
        /// The cursor's cell (column, row), when it is on screen.
        pub cursor: Option<(usize, usize)>,
        /// Where the cursor is drawn, in cells: on its way there while it glides.
        pub drawn: Option<(f32, f32)>,
        pub moving: bool,
        /// A hash of the text on screen and the cursor's cell.
        pub content: u64,
    }

    pub fn paint(paint: &Paint) {
        if !enabled() {
            return;
        }
        let mut line = format!(
            r#""e":"paint","pane":{},"from":{},"moving":{},"content":"{:016x}""#,
            paint.pane,
            us(paint.start),
            paint.moving,
            paint.content
        );
        if let Some((col, row)) = paint.cursor {
            let _ = write!(line, r#","col":{col},"row":{row}"#);
        }
        if let Some((x, y)) = paint.drawn {
            let _ = write!(line, r#","x":{x:.3},"y":{y:.3}"#);
        }
        note(line);
    }

    fn json(text: &str) -> String {
        serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
    }

    fn opt(at: Option<Instant>) -> String {
        at.map_or_else(|| "null".into(), |at| us(at).to_string())
    }

    fn journal_line(entry: &ForegroundJournalEntry) -> Option<(Instant, String)> {
        Some(match entry {
            ForegroundJournalEntry::Event(event) => match event {
                ForegroundEvent::Input(input) => (
                    input.start,
                    format!(
                        r#""e":"input","kind":"{}","end":{},"invalidated":{}"#,
                        input.kind,
                        us(input.end),
                        input.caused_invalidation
                    ),
                ),
                ForegroundEvent::TaskPoll(poll) => (
                    poll.start,
                    format!(
                        r#""e":"poll","end":{},"at":{}"#,
                        us(poll.end.0),
                        json(&format!("{}:{}", poll.location.file(), poll.location.line()))
                    ),
                ),
                ForegroundEvent::Action(action) => {
                    (action.start, format!(r#""e":"action","name":{},"end":{}"#, json(action.name), us(action.end)))
                },
                ForegroundEvent::View(view) => (
                    view.start,
                    format!(
                        r#""e":"view","name":{},"phase":"{}","end":{},"self":{}"#,
                        json(view.name),
                        view.phase.as_str(),
                        us(view.end),
                        dur(view.self_duration)
                    ),
                ),
                // a draw is written with its presentation, below
                ForegroundEvent::Draw(_) | ForegroundEvent::Present(_) | ForegroundEvent::SmallPolls(_) => return None,
            },
            ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(presented)) => {
                let (frame, present) = (&presented.frame, &presented.presentation);
                (
                    frame.draw_start,
                    format!(
                        concat!(
                            r#""e":"frame","window":{},"dirty":{},"invalidations":{},"signal":{},"#,
                            r#""draw_end":{},"layout":{},"prepaint":{},"paint":{},"refresh":{},"#,
                            r#""present":{},"presented":{},"animation":{}"#
                        ),
                        frame.window_id.as_u64(),
                        opt(frame.dirty_at),
                        frame.invalidations,
                        opt(frame.signal_at),
                        us(frame.draw_end),
                        dur(frame.phases.request_layout),
                        dur(frame.phases.prepaint),
                        dur(frame.phases.paint),
                        frame.refresh_interval.map_or_else(|| "null".into(), |d| dur(d).to_string()),
                        us(present.present_start),
                        us(present.present_end),
                        present.animation_interval.map_or_else(|| "null".into(), |d| dur(d).to_string()),
                    ),
                )
            },
            ForegroundJournalEntry::Boundary(IntervalBoundary::FrameSkipped(skipped)) => {
                (skipped.at, format!(r#""e":"skip","reason":"{:?}""#, skipped.reason))
            },
            ForegroundJournalEntry::PlatformSignal(signal) => (
                signal.handled_at,
                format!(
                    r#""e":"signal","at":{},"source":"{}""#,
                    opt(signal.signal_at),
                    match signal.source {
                        FrameRequestSource::NativeCallback => "native",
                        FrameRequestSource::LocalSchedule => "local",
                    }
                ),
            ),
            ForegroundJournalEntry::Discontinuity { lost } => {
                (Instant::now(), format!(r#""e":"lost","entries":{lost}"#))
            },
            _ => return None,
        })
    }

    /// One step of the scenario `LUMEN_LAG_DRIVE` types.
    enum Step {
        /// Text, one key per character, this many milliseconds apart.
        Type(&'static str, u64),
        /// A key (GPUI's keystroke syntax), this many times, this many milliseconds apart.
        Press(&'static str, usize, u64),
        Wait(u64),
    }

    /// Typing at about 140 words a minute, deleting word by word, key repeat
    /// and a held Backspace: the moments that were seen to stutter.
    const SCENARIO: &[Step] = &[
        Step::Wait(2500),
        Step::Type("echo the quick brown fox jumps over the lazy dog", 85),
        Step::Wait(500),
        Step::Press("ctrl-backspace", 6, 300),
        Step::Wait(500),
        Step::Press("ctrl-u", 1, 0),
        Step::Wait(300),
        Step::Type("printf '%s\\n' alpha beta gamma delta epsilon", 40),
        Step::Wait(500),
        Step::Press("ctrl-u", 1, 0),
        Step::Wait(300),
        Step::Type("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 33),
        Step::Wait(300),
        Step::Press("backspace", 30, 33),
        Step::Wait(500),
        Step::Press("ctrl-u", 1, 0),
        Step::Wait(800),
    ];

    /// Types [`SCENARIO`] into the pane that has the keyboard, then quits.
    pub fn drive(cx: &mut App) {
        if !enabled() || std::env::var_os("LUMEN_LAG_DRIVE").is_none() {
            return;
        }
        cx.spawn(async move |cx| {
            for step in SCENARIO {
                let (keys, gap): (Vec<String>, u64) = match step {
                    Step::Wait(ms) => {
                        cx.background_executor().timer(Duration::from_millis(*ms)).await;
                        continue;
                    },
                    Step::Type(text, gap) => {
                        (text.chars().map(|c| if c == ' ' { "space".into() } else { c.to_string() }).collect(), *gap)
                    },
                    Step::Press(key, times, gap) => (vec![key.to_string(); *times], *gap),
                };
                for (i, key) in keys.iter().enumerate() {
                    if i > 0 {
                        cx.background_executor().timer(Duration::from_millis(gap)).await;
                    }
                    cx.update(|cx| press(key, cx));
                }
            }
            cx.update(|cx| {
                flush();
                cx.quit();
            });
        })
        .detach();
    }

    /// A key as the window would have it: as the pane's key handler, or as
    /// text from the input method when the handler leaves it be.
    fn press(key: &str, cx: &mut App) {
        let Ok(keystroke) = Keystroke::parse_with_os(key, OperatingSystem::Linux) else { return };
        let window = cx.windows().into_iter().find_map(|window| window.downcast::<Workspace>());
        let Some(pane) =
            window.and_then(|window| window.read_with(cx, |workspace, _| workspace.active_pane()).ok().flatten())
        else {
            return;
        };
        pane.update(cx, |view, cx| {
            self::key(&keystroke, false);
            let bytes = keys::encode(&keystroke, view.terminal.read(cx).mode(), Press::Down).or_else(|| {
                let text = if keystroke.key == "space" { " " } else { keystroke.key.as_str() };
                (text.chars().count() == 1).then(|| text.as_bytes().to_vec())
            });
            if let Some(bytes) = bytes {
                view.send(bytes, cx);
            }
        });
    }
}
