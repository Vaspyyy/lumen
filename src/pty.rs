//! The thread between a terminal and its program.
//!
//! It reads what the program prints, takes the images out of it (see
//! [`crate::graphics`]) and gives the rest to the terminal's parser, with
//! what a shell says about its commands taken on where it stands (see
//! [`crate::commands`]), and it writes what is typed. This is alacritty_terminal's event loop (Apache-2.0)
//! with that one step added: its own can't be told about escape sequences
//! the parser doesn't know.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite};
use alacritty_terminal::vte::ansi;
use polling::{Event as Interest, Events, PollMode, Poller};

use crate::graphics::{Filter, Graphics, Piece, Shared};
use crate::{commands, queries};

const READ_BUFFER: usize = 0x10_0000;
/// How much is parsed in one go before the window gets a turn at the grid.
const MAX_LOCKED_READ: usize = u16::MAX as usize;
/// What a read on a PTY fails with on Linux once the program has hung up.
const EIO: i32 = 5;
/// The keys alacritty_terminal registers the PTY and its child's exit under.
const PTY_READ_WRITE: usize = 0;
const PTY_CHILD_EVENT: usize = 1;

/// Sends an answer to the program the way the parser sends its own: by the
/// window, which writes them in the order they were asked for. A program
/// that asks several things at once tells the answers apart by that order.
fn answer<L: EventListener>(listener: &L, text: String) {
    listener.send_event(Event::PtyWrite(text));
}

pub enum Msg {
    /// Bytes for the program.
    Input(Cow<'static, [u8]>),
    /// The grid's new size, and a cell's exact size in pixels.
    Resize(WindowSize, (f32, f32)),
    Shutdown,
}

/// The way to reach a terminal's I/O thread.
#[derive(Clone)]
pub struct Sender {
    tx: mpsc::Sender<Msg>,
    poller: Arc<Poller>,
}

impl Sender {
    pub fn send(&self, msg: Msg) {
        if self.tx.send(msg).is_ok() {
            let _ = self.poller.notify();
        }
    }
}

pub struct IoLoop<L: EventListener> {
    poller: Arc<Poller>,
    pty: tty::Pty,
    rx: Receiver<Msg>,
    tx: mpsc::Sender<Msg>,
    terminal: Arc<FairMutex<Term<L>>>,
    listener: L,
    filter: Filter,
    graphics: Graphics,
    /// What finds a shell's marks in the output, and the commands they tell of.
    marks: commands::Scanner,
    /// What finds the questions the parser leaves unanswered.
    queries: queries::Scanner,
    commands: commands::Shared,
    /// A cell's size in pixels, which an image's size in cells follows from.
    cell: (f32, f32),
}

#[derive(Default)]
struct State {
    queue: VecDeque<Cow<'static, [u8]>>,
    /// What is being written, and how much of it has been.
    writing: Option<(Cow<'static, [u8]>, usize)>,
    parser: ansi::Processor,
}

impl<L: EventListener + Send + 'static> IoLoop<L> {
    pub fn new(
        terminal: Arc<FairMutex<Term<L>>>,
        listener: L,
        pty: tty::Pty,
        images: Shared,
        commands: commands::Shared,
        cell: (f32, f32),
    ) -> io::Result<IoLoop<L>> {
        let (tx, rx) = mpsc::channel();
        Ok(IoLoop {
            poller: Poller::new()?.into(),
            pty,
            rx,
            tx,
            terminal,
            listener,
            filter: Filter::default(),
            graphics: Graphics::new(images),
            marks: commands::Scanner::default(),
            queries: queries::Scanner::default(),
            commands,
            cell,
        })
    }

    pub fn sender(&self) -> Sender {
        Sender { tx: self.tx.clone(), poller: self.poller.clone() }
    }

    /// Reads all there is to read. The grid is locked while it is parsed
    /// into, and let go of while an image is decoded.
    fn read(&mut self, state: &mut State, buf: &mut [u8]) -> io::Result<()> {
        let mut unprocessed = 0;
        let mut processed = 0;
        let started = Instant::now();
        // the next lock of the grid is this thread's
        let mut lease = Some(self.terminal.lease());
        let mut terminal = None;

        loop {
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(e) => match e.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        if unprocessed == 0 {
                            break;
                        }
                    },
                    _ => return Err(e),
                },
            }

            if terminal.is_none() {
                terminal = match self.terminal.try_lock_unfair() {
                    // the window is drawing: read on, unless the buffer is full
                    None if unprocessed >= READ_BUFFER => Some(self.terminal.lock_unfair()),
                    None => continue,
                    locked => locked,
                };
            }

            let mut pieces = Vec::new();
            self.filter.feed(&buf[..unprocessed], |piece| pieces.push(piece));
            for piece in pieces {
                match piece {
                    Piece::Text(bytes) => {
                        let mut queries = Vec::new();
                        self.queries.feed(bytes, |end, query| queries.push((end, query)));
                        let term = terminal.get_or_insert_with(|| self.terminal.lock_unfair());
                        let mut done = 0;
                        for (end, query) in queries {
                            commands::advance(
                                &mut self.marks,
                                &self.commands,
                                &mut state.parser,
                                &mut **term,
                                &bytes[done..end],
                            );
                            done = end;
                            // the parser answers what was asked before this when it gets to it,
                            // which is at the end of a synchronized update: that is now
                            if state.parser.sync_bytes_count() > 0 {
                                state.parser.stop_sync(&mut **term);
                            }
                            answer(&self.listener, query.answer(self.cell));
                        }
                        commands::advance(
                            &mut self.marks,
                            &self.commands,
                            &mut state.parser,
                            &mut **term,
                            &bytes[done..],
                        );
                    },
                    Piece::Command(command) => {
                        // reading an image takes a while: the window may have the grid meanwhile
                        terminal = None;
                        drop(lease.take());
                        let step = self.graphics.receive(&command);
                        lease = Some(self.terminal.lease());
                        if let Some(step) = step {
                            let term = terminal.get_or_insert_with(|| self.terminal.lock_unfair());
                            // Inside a synchronized update the parser holds the text back until
                            // the update ends. The image belongs between that text, so the
                            // update is ended here: in order matters more than all at once.
                            if state.parser.sync_bytes_count() > 0 {
                                state.parser.stop_sync(&mut **term);
                            }
                            if let Some(said) = self.graphics.apply(step, &mut **term, self.cell) {
                                answer(&self.listener, String::from_utf8_lossy(&said).into_owned());
                            }
                            // an image moves what stands above it up by its rows, for a few bytes
                            self.commands.lock().unwrap().wrote(crate::graphics::MAX_ROWS);
                        }
                    },
                }
            }

            processed += unprocessed;
            unprocessed = 0;
            if processed >= MAX_LOCKED_READ {
                break;
            }
        }
        drop(terminal);
        drop(lease);
        if processed > 0 {
            crate::lag::read(started, processed);
        }

        // nothing to draw if all of it was inside a synchronized update
        if state.parser.sync_bytes_count() < processed && processed > 0 {
            self.listener.send_event(Event::Wakeup);
        }
        Ok(())
    }

    fn write(&mut self, state: &mut State) -> io::Result<()> {
        loop {
            let Some((bytes, written)) = state.writing.take().or_else(|| state.queue.pop_front().map(|b| (b, 0)))
            else {
                return Ok(());
            };
            match self.pty.writer().write(&bytes[written..]) {
                Ok(0) => {
                    state.writing = Some((bytes, written));
                    return Ok(());
                },
                Ok(n) if written + n < bytes.len() => {
                    crate::lag::written(n);
                    state.writing = Some((bytes, written + n))
                },
                Ok(n) => crate::lag::written(n),
                Err(e) => {
                    state.writing = Some((bytes, written));
                    return match e.kind() {
                        ErrorKind::Interrupted | ErrorKind::WouldBlock => Ok(()),
                        _ => Err(e),
                    };
                },
            }
        }
    }

    pub fn spawn(mut self) {
        let thread = std::thread::Builder::new().name("PTY reader".into());
        let _ = thread.spawn(move || {
            let mut state = State::default();
            let mut buf = vec![0u8; READ_BUFFER];
            let mut interest = Interest::readable(0);
            // SAFETY: the PTY outlives its registration, which is taken back before this returns
            if unsafe { self.pty.register(&self.poller, interest, PollMode::Level) }.is_err() {
                return;
            }
            let mut events = Events::with_capacity(NonZeroUsize::new(1024).unwrap());

            'run: loop {
                // a synchronized update that never ends is ended for it
                let timeout =
                    state.parser.sync_timeout().sync_timeout().map(|at| at.saturating_duration_since(Instant::now()));
                events.clear();
                if let Err(e) = self.poller.wait(&mut events, timeout) {
                    match e.kind() {
                        ErrorKind::Interrupted => continue,
                        _ => break,
                    }
                }

                let messages: Vec<Msg> = self.rx.try_iter().collect();
                if events.is_empty() && messages.is_empty() {
                    state.parser.stop_sync(&mut *self.terminal.lock());
                    self.listener.send_event(Event::Wakeup);
                    continue;
                }
                for message in messages {
                    match message {
                        Msg::Input(bytes) => state.queue.push_back(bytes),
                        Msg::Resize(size, cell) => {
                            self.cell = cell;
                            self.pty.on_resize(size);
                        },
                        Msg::Shutdown => break 'run,
                    }
                }

                for event in events.iter() {
                    match event.key {
                        PTY_CHILD_EVENT => {
                            if let Some(tty::ChildEvent::Exited(status)) = self.pty.next_child_event() {
                                if let Some(status) = status {
                                    self.listener.send_event(Event::ChildExit(status));
                                }
                                // what the program said last is still to be read
                                let _ = self.read(&mut state, &mut buf);
                                self.terminal.lock().exit();
                                self.listener.send_event(Event::Wakeup);
                                break 'run;
                            }
                        },
                        PTY_READ_WRITE => {
                            if event.is_interrupt() {
                                continue;
                            }
                            if event.readable
                                && let Err(e) = self.read(&mut state, &mut buf)
                            {
                                // the program hung up: its exit is the next event
                                if e.raw_os_error() == Some(EIO) {
                                    continue;
                                }
                                break 'run;
                            }
                            if event.writable && self.write(&mut state).is_err() {
                                break 'run;
                            }
                        },
                        _ => {},
                    }
                }

                // an answer to the program may have been queued while reading
                let needs_write = state.writing.is_some() || !state.queue.is_empty();
                if needs_write != interest.writable {
                    interest.writable = needs_write;
                    if self.pty.reregister(&self.poller, interest, PollMode::Level).is_err() {
                        break;
                    }
                }
            }
            let _ = self.pty.deregister(&self.poller);
        });
    }
}
