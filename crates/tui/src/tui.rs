//! TUI terminal management and event handling.
//!
//! This module provides terminal initialization, event handling, and rendering
//! management for the TUI application.

use std::io::IsTerminal;
use std::io::Result;
use std::io::Stdout;
use std::io::stdin;
use std::io::stdout;
use std::panic;
use std::sync::Arc;

use crossterm::event::DisableBracketedPaste;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::KeyEvent;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::disable_raw_mode;
use ratatui::crossterm::terminal::enable_raw_mode;
use tokio::sync::broadcast;

use crate::event_stream::EventBroker;
use crate::event_stream::TuiEventStream;
use crate::frame_requester::FrameRequester;
use neoco_event::EventBus;

/// A type alias for the terminal type used in this application.
pub type Terminal = ratatui::Terminal<CrosstermBackend<Stdout>>;

/// TUI events that the application handles.
#[derive(Clone, Debug)]
pub enum TuiEvent {
    /// A key was pressed.
    Key(KeyEvent),
    /// Text was pasted.
    Paste(String),
    /// A draw is requested.
    Draw,
}

/// The main TUI manager that handles terminal initialization, events, and rendering.
pub struct Tui {
    frame_requester: FrameRequester,
    draw_tx: broadcast::Sender<()>,
    event_broker: Arc<EventBroker>,
    event_bus: EventBus,
    /// The terminal instance for rendering.
    pub terminal: Terminal,
}

impl Tui {
    /// Create a new `Tui` instance.
    ///
    /// # Arguments
    ///
    /// * `terminal` - The ratatui terminal instance for rendering.
    #[must_use]
    pub fn new(terminal: Terminal) -> Self {
        let (draw_tx, _) = broadcast::channel(16);
        let frame_requester = FrameRequester::new(draw_tx.clone());

        Self {
            frame_requester,
            draw_tx,
            event_broker: Arc::new(EventBroker::new()),
            event_bus: EventBus::default(),
            terminal,
        }
    }

    /// Get a handle to the frame requester for scheduling redraws.
    #[must_use]
    pub fn frame_requester(&self) -> FrameRequester {
        self.frame_requester.clone()
    }

    /// Create an event stream for receiving TUI events.
    #[must_use]
    pub fn event_stream(&self) -> TuiEventStream {
        let draw_rx = self.draw_tx.subscribe();
        TuiEventStream::new(Arc::clone(&self.event_broker), draw_rx)
    }

    /// Request an immediate frame draw.
    pub fn request_draw(&self) {
        self.frame_requester.schedule_frame();
    }

    /// Get a reference to the event bus.
    #[must_use]
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = restore();
    }
}

/// Set terminal modes for TUI operation.
///
/// # Errors
///
/// Returns an error if terminal operations fail.
pub fn set_modes() -> Result<()> {
    execute!(stdout(), EnableBracketedPaste)?;
    enable_raw_mode()?;
    let _ = execute!(stdout(), crossterm::cursor::Hide);
    Ok(())
}

/// Restore the terminal to its original state.
///
/// # Errors
///
/// Returns an error if terminal restoration fails.
pub fn restore() -> Result<()> {
    let _ = execute!(stdout(), DisableBracketedPaste);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), crossterm::cursor::Show);
    Ok(())
}

/// Initialize the terminal for TUI operation.
///
/// This does NOT use the alternate screen, keeping history in normal scrollback.
///
/// # Errors
///
/// Returns an error if terminal initialization fails.
pub fn init() -> Result<Terminal> {
    if !stdin().is_terminal() {
        return Err(std::io::Error::other("stdin is not a terminal"));
    }
    if !stdout().is_terminal() {
        return Err(std::io::Error::other("stdout is not a terminal"));
    }
    set_modes()?;
    set_panic_hook();

    let backend = CrosstermBackend::new(stdout());
    let terminal = ratatui::Terminal::new(backend)?;
    Ok(terminal)
}

fn set_panic_hook() {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore();
        hook(panic_info);
    }));
}
