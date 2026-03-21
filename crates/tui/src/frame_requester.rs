//! Frame draw scheduling utilities for the TUI.
//!
//! This module exposes [`FrameRequester`], a lightweight handle that widgets and
//! background tasks can clone to request future redraws of the TUI.
//!
//! Internally it spawns a frame scheduler task that coalesces many requests
//! into a single notification on a broadcast channel used by the main TUI event
//! loop.

use std::time::Duration;
use std::time::Instant;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::frame_rate_limiter::FrameRateLimiter;

/// A requester for scheduling future frame draws on the TUI event loop.
///
/// Clones of this type can be freely shared across tasks to make it possible to trigger frame draws
/// from anywhere in the TUI code.
#[derive(Clone, Debug)]
pub struct FrameRequester {
    frame_schedule_tx: mpsc::UnboundedSender<Instant>,
}

impl FrameRequester {
    /// Create a new `FrameRequester` and spawn its associated `FrameScheduler` task.
    ///
    /// The provided `draw_tx` is used to notify the TUI event loop of scheduled draws.
    #[must_use]
    pub fn new(draw_tx: broadcast::Sender<()>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let scheduler = FrameScheduler::new(rx, draw_tx);
        tokio::spawn(scheduler.run());
        Self {
            frame_schedule_tx: tx,
        }
    }

    /// Schedule a frame draw as soon as possible.
    pub fn schedule_frame(&self) {
        let _ = self.frame_schedule_tx.send(Instant::now());
    }

    /// Schedule a frame draw to occur after the specified duration.
    pub fn schedule_frame_in(&self, dur: Duration) {
        let _ = self.frame_schedule_tx.send(Instant::now() + dur);
    }
}

/// A scheduler for coalescing frame draw requests and notifying the TUI event loop.
struct FrameScheduler {
    receiver: mpsc::UnboundedReceiver<Instant>,
    draw_tx: broadcast::Sender<()>,
    rate_limiter: FrameRateLimiter,
}

impl FrameScheduler {
    /// Create a new `FrameScheduler` with the provided receiver and draw notification sender.
    fn new(receiver: mpsc::UnboundedReceiver<Instant>, draw_tx: broadcast::Sender<()>) -> Self {
        Self {
            receiver,
            draw_tx,
            rate_limiter: FrameRateLimiter::default(),
        }
    }

    /// Run the scheduling loop, coalescing frame requests and notifying the TUI event loop.
    async fn run(mut self) {
        const ONE_YEAR: Duration = Duration::from_secs(60 * 60 * 24 * 365);
        let mut next_deadline: Option<Instant> = None;
        loop {
            let target = next_deadline.unwrap_or_else(|| Instant::now() + ONE_YEAR);
            let deadline = tokio::time::sleep_until(target.into());
            tokio::pin!(deadline);

            tokio::select! {
                draw_at = self.receiver.recv() => {
                    let Some(draw_at) = draw_at else {
                        break;
                    };
                    let draw_at = self.rate_limiter.clamp_deadline(draw_at);
                    next_deadline = Some(next_deadline.map_or(draw_at, |cur| cur.min(draw_at)));
                }
                () = &mut deadline => {
                    if next_deadline.is_some() {
                        next_deadline = None;
                        self.rate_limiter.mark_emitted(target);
                        let _ = self.draw_tx.send(());
                    }
                }
            }
        }
    }
}
