//! Output handling.

use std::io::{self, Write};
use std::sync::Mutex;

use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::{ExecutableCommand, QueueableCommand};
use tracing::error;

use crate::events::ChatEvent;

/// Callback type for streaming output.
pub type OutputCallback<'a> = Box<dyn Fn(&str) + Send + Sync + 'a>;

/// Handler for output rendering.
pub struct OutputHandler {
    use_stdout: Mutex<bool>,
}

impl Clone for OutputHandler {
    fn clone(&self) -> Self {
        let value = self.use_stdout.lock().map_or_else(
            |_err| {
                error!("Output error: failed to acquire lock during clone");
                true
            },
            |guard| *guard,
        );

        Self {
            use_stdout: Mutex::new(value),
        }
    }
}

impl OutputHandler {
    /// Create a new `OutputHandler`.
    #[must_use]
    pub fn new(_line_count: u16) -> Self {
        Self {
            use_stdout: Mutex::new(true),
        }
    }

    /// Queue colored output commands (must flush after calling).
    fn queue_colored(stdout: &mut impl Write, text: &str, color: Color) -> io::Result<()> {
        stdout
            .queue(SetForegroundColor(color))?
            .queue(Print(text))?
            .queue(ResetColor)
            .map(|_| ())
    }

    /// Queue text output without color (must flush after calling).
    fn queue_text(stdout: &mut impl Write, text: &str) -> io::Result<()> {
        stdout.queue(Print(text)).map(|_| ())
    }

    /// Get output callback for streaming output.
    pub fn as_output_callback(&self) -> OutputCallback<'_> {
        let use_stdout = &self.use_stdout;

        Box::new(move |text: &str| {
            let Ok(use_stdout_guard) = use_stdout.lock() else {
                error!("Output error: failed to acquire lock for output callback");
                return;
            };
            if !*use_stdout_guard {
                return;
            }
            if io::stdout().execute(Print(text)).is_err() {
                error!("Output error: failed to write text");
            }
        })
    }

    /// Disable stdout output.
    pub fn disable_stdout(&self) {
        let Ok(mut use_stdout) = self.use_stdout.lock() else {
            error!("Output error: failed to acquire lock to disable stdout");
            return;
        };
        *use_stdout = false;
    }

    /// Render text to stdout with optional color.
    #[expect(clippy::unused_self)]
    fn render_with_color(&self, text: &str, color: Color) {
        if io::stdout()
            .execute(SetForegroundColor(color))
            .and_then(|s| s.execute(Print(text)))
            .and_then(|s| s.execute(ResetColor))
            .is_err()
        {
            error!("Output error: failed to write colored text");
        }
    }

    /// Render text to stdout (default grey color).
    ///
    /// Note: This method does not check the `use_stdout` flag.
    /// For event-based rendering that respects the flag, use the `handle` method instead.
    pub fn render(&self, text: &str) {
        self.render_with_color(text, Color::Grey);
    }

    /// Finalize output.
    ///
    /// PERF: Wait for terminal buffer to flush before proceeding.
    pub fn finalize(self) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

impl EventHandler for OutputHandler {
    fn handle(&self, event: ChatEvent) {
        let Ok(use_stdout_guard) = self.use_stdout.lock() else {
            error!("Output error: failed to acquire lock for event handling");
            return;
        };
        let use_stdout = *use_stdout_guard;
        drop(use_stdout_guard);

        if !use_stdout {
            return;
        }

        let mut stdout = io::stdout();

        let result = match event {
            ChatEvent::Text(text) => Self::queue_text(&mut stdout, &text),
            ChatEvent::Reasoning(content) | ChatEvent::ReasoningDelta(content) => {
                Self::queue_colored(&mut stdout, &format!("[思考] {content}"), Color::Cyan)
            },
            ChatEvent::ToolCall { name, arguments } => {
                Self::queue_colored(&mut stdout, &format!("[工具调用] {name}: "), Color::Yellow)
                    .and_then(|()| Self::queue_colored(&mut stdout, &arguments, Color::Grey))
            },
            ChatEvent::ToolCallDelta(content) => {
                Self::queue_colored(&mut stdout, &format!("[工具调用] {content}"), Color::Yellow)
            },
            ChatEvent::ToolResult { content } => Self::queue_colored(
                &mut stdout,
                &format!("[工具结果] {content:#?}\n"),
                Color::Green,
            ),
            ChatEvent::Usage(_) => Ok(()),
            ChatEvent::Done => Self::queue_text(&mut stdout, "\n"),
        };

        if result.is_err() {
            error!("Output error: failed to write to stdout");
        }
        let _ = stdout.flush();
    }
}

/// Trait for handling chat events.
pub trait EventHandler {
    /// Handle a chat event.
    fn handle(&self, event: ChatEvent);
}
