//! Output handling.

use std::io::{self, Write};
use std::sync::Mutex;

use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::{ExecutableCommand, QueueableCommand};
use tracing::error;

use crate::events::ChatEvent;
use tracing::trace;

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

    /// Format command result for display (first 5 lines of stdout/stderr).
    fn format_command_result(stdout: &str, stderr: &str, exit_code: i64) -> String {
        use std::fmt::Write as _;

        let estimated_capacity = stdout.len().min(300) + stderr.len().min(300) + 128;
        let mut result = String::with_capacity(estimated_capacity);

        let mut stdout_iter = stdout.lines();
        let stdout_lines: Vec<_> = stdout_iter.by_ref().take(5).collect::<Vec<_>>();
        if !stdout_lines.is_empty() {
            let _ = writeln!(result, "输出:");
            for line in &stdout_lines {
                let _ = writeln!(result, "  {line}");
            }
            if stdout_iter.next().is_some() {
                let _ = writeln!(result, "  ... (更多输出已省略)");
            }
        }

        let mut stderr_iter = stderr.lines();
        let stderr_lines: Vec<_> = stderr_iter.by_ref().take(5).collect::<Vec<_>>();
        if !stderr_lines.is_empty() {
            let _ = writeln!(result, "错误:");
            for line in &stderr_lines {
                let _ = writeln!(result, "  {line}");
            }
            if stderr_iter.next().is_some() {
                let _ = writeln!(result, "  ... (更多错误已省略)");
            }
        }

        if exit_code != 0 {
            let _ = writeln!(result, "退出码: {exit_code}");
        }

        result
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
            ChatEvent::ToolCall { arguments } => {
                if let Ok(cmd_obj) = serde_json::from_str::<serde_json::Value>(&arguments) {
                    if let Some(command) = cmd_obj.get("command").and_then(|v| v.as_str()) {
                        Self::queue_colored(
                            &mut stdout,
                            &format!("[Bash] {command}"),
                            Color::Yellow,
                        )
                    } else {
                        Self::queue_colored(
                            &mut stdout,
                            &format!("[Bash] {arguments}"),
                            Color::Yellow,
                        )
                    }
                } else {
                    Self::queue_colored(&mut stdout, &format!("[Bash] {arguments}"), Color::Yellow)
                }
            },
            ChatEvent::ToolCallDelta(content) => {
                Self::queue_colored(&mut stdout, &format!("[工具调用] {content}"), Color::Yellow)
            },
            ChatEvent::ToolResult {
                content,
                structured,
            } => {
                if let Some(data) = structured {
                    trace!("完整工具结果 (JSON): {data:#?}");
                    if let (Some(stdout_str), Some(stderr), Some(exit_code)) = (
                        data.get("stdout").and_then(|v| v.as_str()),
                        data.get("stderr").and_then(|v| v.as_str()),
                        data.get("exit_code").and_then(serde_json::Value::as_i64),
                    ) {
                        let formatted = Self::format_command_result(stdout_str, stderr, exit_code);
                        Self::queue_colored(&mut stdout, &formatted, Color::Green)
                    } else {
                        Self::queue_colored(
                            &mut stdout,
                            &format!("[工具结果] {content}\n"),
                            Color::Green,
                        )
                    }
                } else {
                    Self::queue_colored(
                        &mut stdout,
                        &format!("[工具结果] {content}\n"),
                        Color::Green,
                    )
                }
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_command_result_empty() {
        let result = OutputHandler::format_command_result("", "", 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_command_result_only_stdout() {
        let stdout = "line1\nline2\nline3";
        let result = OutputHandler::format_command_result(stdout, "", 0);
        assert!(result.contains("输出:"));
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));
        assert!(result.contains("line3"));
        assert!(!result.contains("错误:"));
    }

    #[test]
    fn test_format_command_result_truncates_long_stdout() {
        let long_output = (0..10)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = OutputHandler::format_command_result(&long_output, "", 0);
        assert!(result.contains("line0"));
        assert!(result.contains("line4"));
        assert!(result.contains("更多输出已省略"));
    }

    #[test]
    fn test_format_command_result_truncates_long_stderr() {
        let long_error = (0..10)
            .map(|i| format!("error{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = OutputHandler::format_command_result("", &long_error, 0);
        assert!(result.contains("error0"));
        assert!(result.contains("error4"));
        assert!(result.contains("更多错误已省略"));
    }

    #[test]
    fn test_format_command_result_max_5_lines_shown() {
        let long_output = (0..100)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = OutputHandler::format_command_result(&long_output, "", 0);
        let lines: Vec<&str> = result.lines().filter(|l| l.starts_with("  line")).collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_format_command_result_both_streams() {
        let stdout = "output";
        let stderr = "error";
        let result = OutputHandler::format_command_result(stdout, stderr, 0);
        assert!(result.contains("输出:"));
        assert!(result.contains("错误:"));
        assert!(result.contains("output"));
        assert!(result.contains("error"));
    }

    #[test]
    fn test_format_command_result_with_exit_code() {
        let result = OutputHandler::format_command_result("", "", 1);
        assert!(result.contains("退出码: 1"));
    }

    #[test]
    fn test_format_command_result_zero_exit_code_not_shown() {
        let result = OutputHandler::format_command_result("", "", 0);
        assert!(!result.contains("退出码"));
    }

    #[test]
    fn test_format_command_result_negative_exit_code() {
        let result = OutputHandler::format_command_result("", "", -1);
        assert!(result.contains("退出码: -1"));
    }
}
