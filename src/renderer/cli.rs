//! CLI renderer implementation (stdout with ANSI colors).

use std::io::{self, Write};

use tracing::error;

use super::Renderer;
use crate::errors::RenderError;
use crate::events::ChatEvent;

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_GREEN: &str = "\x1b[32m";

/// CLI renderer that outputs to stdout with ANSI colors.
#[expect(dead_code)]
pub struct CliRenderer {
    use_color: bool,
}

impl CliRenderer {
    /// Create a new CLI renderer.
    #[must_use]
    pub const fn new() -> Self {
        Self { use_color: true }
    }

    /// Create a new CLI renderer without colors.
    #[must_use]
    pub const fn no_color() -> Self {
        Self { use_color: false }
    }

    /// Queue colored output commands (must flush after calling).
    fn queue_colored(stdout: &mut impl Write, text: &str, color: &str) -> io::Result<()> {
        write!(stdout, "{color}{text}{ANSI_RESET}")
    }

    /// Queue text output without color (must flush after calling).
    fn queue_text(stdout: &mut impl Write, text: &str) -> io::Result<()> {
        write!(stdout, "{text}")
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
}

impl Default for CliRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for CliRenderer {
    fn render_event(&mut self, event: &ChatEvent) -> Result<(), RenderError> {
        let mut stdout = io::stdout();

        let result = match event {
            ChatEvent::Text(text) => Self::queue_text(&mut stdout, text),
            ChatEvent::Reasoning(content) | ChatEvent::ReasoningDelta(content) => {
                Self::queue_colored(&mut stdout, &format!("[思考] {content}"), ANSI_CYAN)
            },
            ChatEvent::ToolCall { arguments } => {
                if let Ok(cmd_obj) = serde_json::from_str::<serde_json::Value>(arguments) {
                    if let Some(command) = cmd_obj.get("command").and_then(|v| v.as_str()) {
                        Self::queue_colored(&mut stdout, &format!("[Bash] {command}"), ANSI_YELLOW)
                    } else {
                        Self::queue_colored(
                            &mut stdout,
                            &format!("[Bash] {arguments}"),
                            ANSI_YELLOW,
                        )
                    }
                } else {
                    Self::queue_colored(&mut stdout, &format!("[Bash] {arguments}"), ANSI_YELLOW)
                }
            },
            ChatEvent::ToolCallDelta(content) => {
                Self::queue_colored(&mut stdout, &format!("[工具调用] {content}"), ANSI_YELLOW)
            },
            ChatEvent::ToolResult {
                content,
                structured,
            } => {
                if let Some(data) = structured {
                    if let (Some(stdout_str), Some(stderr), Some(exit_code)) = (
                        data.get("stdout").and_then(|v| v.as_str()),
                        data.get("stderr").and_then(|v| v.as_str()),
                        data.get("exit_code").and_then(serde_json::Value::as_i64),
                    ) {
                        let formatted = Self::format_command_result(stdout_str, stderr, exit_code);
                        Self::queue_colored(&mut stdout, &formatted, ANSI_GREEN)
                    } else {
                        Self::queue_colored(
                            &mut stdout,
                            &format!("[工具结果] {content}\n"),
                            ANSI_GREEN,
                        )
                    }
                } else {
                    Self::queue_colored(&mut stdout, &format!("[工具结果] {content}\n"), ANSI_GREEN)
                }
            },
            ChatEvent::Usage(_) => Ok(()),
            ChatEvent::Done => Self::queue_text(&mut stdout, "\n"),
        };

        if result.is_err() {
            error!("Output error: failed to write to stdout");
        }
        stdout.flush()?;
        Ok(())
    }

    fn render_chunk(&mut self, chunk: &str, is_thinking: bool) -> Result<(), RenderError> {
        let mut stdout = io::stdout();
        if is_thinking {
            Self::queue_colored(&mut stdout, chunk, ANSI_CYAN)?;
        } else {
            Self::queue_text(&mut stdout, chunk)?;
        }
        stdout.flush()?;
        Ok(())
    }

    fn clear(&mut self) -> Result<(), RenderError> {
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        io::stdout().flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_command_result_empty() {
        let result = CliRenderer::format_command_result("", "", 0);
        assert_eq!(result, "");
    }

    #[test]
    fn test_format_command_result_only_stdout() {
        let stdout = "line1\nline2\nline3";
        let result = CliRenderer::format_command_result(stdout, "", 0);
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
        let result = CliRenderer::format_command_result(&long_output, "", 0);
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
        let result = CliRenderer::format_command_result("", &long_error, 0);
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
        let result = CliRenderer::format_command_result(&long_output, "", 0);
        let lines: Vec<&str> = result.lines().filter(|l| l.starts_with("  line")).collect();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_format_command_result_both_streams() {
        let stdout = "output";
        let stderr = "error";
        let result = CliRenderer::format_command_result(stdout, stderr, 0);
        assert!(result.contains("输出:"));
        assert!(result.contains("错误:"));
        assert!(result.contains("output"));
        assert!(result.contains("error"));
    }

    #[test]
    fn test_format_command_result_with_exit_code() {
        let result = CliRenderer::format_command_result("", "", 1);
        assert!(result.contains("退出码: 1"));
    }

    #[test]
    fn test_format_command_result_zero_exit_code_not_shown() {
        let result = CliRenderer::format_command_result("", "", 0);
        assert!(!result.contains("退出码"));
    }

    #[test]
    fn test_format_command_result_negative_exit_code() {
        let result = CliRenderer::format_command_result("", "", -1);
        assert!(result.contains("退出码: -1"));
    }
}
