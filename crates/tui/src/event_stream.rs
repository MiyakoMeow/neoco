//! Event stream plumbing for the TUI.
//!
//! - [`EventBroker`] holds the shared crossterm stream so multiple callers reuse the same
//!   input source and can drop/recreate it on pause/resume without rebuilding consumers.
//! - [`TuiEventStream`] wraps a draw event subscription plus the shared [`EventBroker`] and maps crossterm
//!   events into [`TuiEvent`].

use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Context;
use std::task::Poll;

use crossterm::event::Event;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::WatchStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::tui::TuiEvent;

/// Shared crossterm input state for all [`TuiEventStream`] instances.
pub struct EventBroker {
    state: Mutex<EventBrokerState>,
    resume_events_tx: watch::Sender<()>,
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks state of underlying event source.
enum EventBrokerState {
    /// Underlying event source (i.e., crossterm `EventStream`) dropped.
    Paused,
    /// A new event source will be created on next poll.
    Start,
    /// Event source is currently running.
    Running(crossterm::event::EventStream),
}

impl EventBrokerState {
    /// Return the running event source, starting it if needed; `None` when paused.
    fn active_event_source_mut(&mut self) -> Option<&mut crossterm::event::EventStream> {
        match self {
            EventBrokerState::Paused => None,
            EventBrokerState::Start => {
                *self = EventBrokerState::Running(crossterm::event::EventStream::new());
                match self {
                    EventBrokerState::Running(events) => Some(events),
                    EventBrokerState::Paused | EventBrokerState::Start => unreachable!(),
                }
            },
            EventBrokerState::Running(events) => Some(events),
        }
    }
}

impl EventBroker {
    /// Create a new `EventBroker`.
    #[must_use]
    pub fn new() -> Self {
        let (resume_events_tx, _resume_events_rx) = watch::channel(());
        Self {
            state: Mutex::new(EventBrokerState::Start),
            resume_events_tx,
        }
    }

    /// Drop the underlying event source.
    pub fn pause_events(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = EventBrokerState::Paused;
    }

    /// Create a new instance of the underlying event source.
    pub fn resume_events(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = EventBrokerState::Start;
        let _ = self.resume_events_tx.send(());
    }

    /// Subscribe to a notification that fires whenever `resume_events` is called.
    #[must_use]
    pub fn resume_events_rx(&self) -> watch::Receiver<()> {
        self.resume_events_tx.subscribe()
    }
}

/// `TuiEventStream` is a struct for reading TUI events (draws and user input).
///
/// Each instance has its own draw subscription (the draw channel is broadcast, so
/// multiple receivers are fine), while crossterm input is funneled through a
/// single shared [`EventBroker`].
pub struct TuiEventStream {
    broker: Arc<EventBroker>,
    draw_stream: BroadcastStream<()>,
    resume_stream: WatchStream<()>,
    poll_draw_first: bool,
}

impl TuiEventStream {
    /// Create a new `TuiEventStream`.
    #[must_use]
    pub fn new(broker: Arc<EventBroker>, draw_rx: broadcast::Receiver<()>) -> Self {
        let resume_stream = WatchStream::from_changes(broker.resume_events_rx());
        Self {
            broker,
            draw_stream: BroadcastStream::new(draw_rx),
            resume_stream,
            poll_draw_first: false,
        }
    }

    /// Poll the shared crossterm stream for the next mapped `TuiEvent`.
    fn poll_crossterm_event(&mut self, cx: &mut Context<'_>) -> Poll<Option<TuiEvent>> {
        loop {
            let poll_result = {
                let mut state = self
                    .broker
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(events) = state.active_event_source_mut() else {
                    drop(state);
                    match Pin::new(&mut self.resume_stream).poll_next(cx) {
                        Poll::Ready(Some(())) => continue,
                        Poll::Ready(None) => return Poll::Ready(None),
                        Poll::Pending => return Poll::Pending,
                    }
                };
                match Pin::new(events).poll_next(cx) {
                    Poll::Ready(Some(Ok(event))) => Some(event),
                    Poll::Ready(Some(Err(_)) | None) => {
                        *state = EventBrokerState::Start;
                        return Poll::Ready(None);
                    },
                    Poll::Pending => {
                        drop(state);
                        match Pin::new(&mut self.resume_stream).poll_next(cx) {
                            Poll::Ready(Some(())) => continue,
                            Poll::Ready(None) => return Poll::Ready(None),
                            Poll::Pending => return Poll::Pending,
                        }
                    },
                }
            };

            if let Some(mapped) = poll_result.and_then(Self::map_crossterm_event) {
                return Poll::Ready(Some(mapped));
            }
        }
    }

    /// Poll the draw broadcast stream for the next draw event.
    fn poll_draw_event(&mut self, cx: &mut Context<'_>) -> Poll<Option<TuiEvent>> {
        match Pin::new(&mut self.draw_stream).poll_next(cx) {
            Poll::Ready(Some(Ok(()) | Err(BroadcastStreamRecvError::Lagged(_)))) => {
                Poll::Ready(Some(TuiEvent::Draw))
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    /// Map a crossterm event to a `TuiEvent`, skipping events we don't use.
    fn map_crossterm_event(event: Event) -> Option<TuiEvent> {
        match event {
            Event::Key(key_event) => Some(TuiEvent::Key(key_event)),
            Event::Resize(_, _) => Some(TuiEvent::Draw),
            Event::Paste(pasted) => Some(TuiEvent::Paste(pasted)),
            _ => None,
        }
    }
}

impl Unpin for TuiEventStream {}

impl Stream for TuiEventStream {
    type Item = TuiEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let draw_first = self.poll_draw_first;
        self.poll_draw_first = !self.poll_draw_first;

        if draw_first {
            if let Poll::Ready(event) = self.poll_draw_event(cx) {
                return Poll::Ready(event);
            }
            if let Poll::Ready(event) = self.poll_crossterm_event(cx) {
                return Poll::Ready(event);
            }
        } else {
            if let Poll::Ready(event) = self.poll_crossterm_event(cx) {
                return Poll::Ready(event);
            }
            if let Poll::Ready(event) = self.poll_draw_event(cx) {
                return Poll::Ready(event);
            }
        }

        Poll::Pending
    }
}
