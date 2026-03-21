//! Unified event bus for neoco.

use tokio::sync::broadcast;

use crate::event::UnifiedEvent;

/// Unified event bus for all event types.
///
/// The event bus uses a tokio broadcast channel to allow multiple
/// subscribers to receive events.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<UnifiedEvent>,
}

impl EventBus {
    /// Create a new event bus with the given capacity.
    ///
    /// The capacity is the maximum number of events that can be buffered
    /// before old events are dropped.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Send an event to the event bus.
    ///
    /// Returns the number of active receivers, or an error if there are no active receivers.
    ///
    /// # Errors
    ///
    /// Returns an error if there are no active receivers.
    pub fn send(&self, event: UnifiedEvent) -> Result<usize, String> {
        self.sender.send(event).map_err(|e| e.to_string())
    }

    /// Subscribe to events from the event bus.
    ///
    /// Returns a receiver that can be used to receive events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<UnifiedEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TerminalEvent, UIEvent};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[tokio::test]
    async fn test_event_bus_send_receive() {
        let bus = EventBus::new(10);
        let mut rx = bus.subscribe();

        let evt = UnifiedEvent::UI(UIEvent::SendMessage("hello".to_string()));
        bus.send(evt.clone()).unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            UnifiedEvent::UI(UIEvent::SendMessage(msg)) => assert_eq!(msg, "hello"),
            _ => panic!("Expected UI event"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_multiple_receivers() {
        let bus = EventBus::new(10);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let evt = UnifiedEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
        )));
        bus.send(evt).unwrap();

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        match (received1, received2) {
            (
                UnifiedEvent::Terminal(TerminalEvent::Key(key1)),
                UnifiedEvent::Terminal(TerminalEvent::Key(key2)),
            ) => {
                assert_eq!(key1.code, KeyCode::Char('a'));
                assert_eq!(key2.code, KeyCode::Char('a'));
            },
            _ => panic!("Expected terminal events"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_receiver_count() {
        let bus = EventBus::new(10);
        assert_eq!(bus.receiver_count(), 0);

        let _receiver1 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 1);

        let _receiver2 = bus.subscribe();
        assert_eq!(bus.receiver_count(), 2);
    }

    #[tokio::test]
    async fn test_event_bus_default() {
        let bus = EventBus::default();
        let mut rx = bus.subscribe();

        let evt = UnifiedEvent::UI(UIEvent::Exit);
        bus.send(evt).unwrap();

        let received = rx.recv().await.unwrap();
        match received {
            UnifiedEvent::UI(UIEvent::Exit) => {},
            _ => panic!("Expected exit event"),
        }
    }
}
