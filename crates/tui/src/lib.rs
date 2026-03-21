//! TUI renderer implementation using ratatui.

use std::io::{self, Stdout};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use neoco_core::errors::RenderError;
use neoco_core::events::ChatEvent;
use neoco_core::renderer::Renderer;

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// System message.
    System,
    /// Tool execution.
    Tool,
}

/// Chat message.
#[derive(Debug, Clone)]
pub struct Message {
    role: Role,
    content: String,
    #[expect(dead_code)]
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl Message {
    /// Create a new message.
    #[must_use]
    pub fn new(role: Role, content: String) -> Self {
        Self {
            role,
            content,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Get the message role.
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Get the message content.
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }
}

/// TUI application state.
pub struct AppState {
    messages: Vec<Message>,
    input: String,
    status_line: String,
    scroll_offset: usize,
    is_streaming: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            status_line: String::from("Neoco TUI - 输入消息开始聊天，Ctrl+C 退出"),
            scroll_offset: 0,
            is_streaming: false,
        }
    }
}

impl AppState {
    /// Create a new app state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a message to the history.
    pub fn add_message(&mut self, role: Role, content: String) {
        self.messages.push(Message::new(role, content));
        self.update_scroll();
    }

    /// Append to the last message (for streaming).
    pub fn append_to_last(&mut self, content: &str) {
        if let Some(msg) = self.messages.last_mut() {
            msg.content.push_str(content);
        }
    }

    /// Update the status line.
    pub fn set_status(&mut self, status: String) {
        self.status_line = status;
    }

    /// Set the streaming state.
    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
    }

    /// Get the current input.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Set the input.
    pub fn set_input(&mut self, input: String) {
        self.input = input;
    }

    /// Append to input.
    pub fn append_input(&mut self, ch: char) {
        self.input.push(ch);
    }

    /// Remove last character from input.
    pub fn backspace(&mut self) {
        self.input.pop();
    }

    /// Clear input.
    pub fn clear_input(&mut self) {
        self.input.clear();
    }

    /// Update scroll offset to show latest message.
    fn update_scroll(&mut self) {
        self.scroll_offset = self.messages.len().saturating_sub(1);
    }
}

/// TUI renderer.
pub struct TuiRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    state: AppState,
    event_tx: mpsc::Sender<TuiEvent>,
    event_rx: mpsc::Receiver<TuiEvent>,
}

/// TUI events.
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// Input character.
    Input(char),
    /// Backspace.
    Backspace,
    /// Enter key (submit).
    Enter,
    /// Quit.
    Quit,
}

impl TuiRenderer {
    /// Create a new TUI renderer.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal initialization fails.
    pub fn new() -> Result<Self, RenderError> {
        enable_raw_mode().map_err(|e| RenderError::Terminal(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).map_err(|e| RenderError::Terminal(e.to_string()))?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).map_err(|e| RenderError::Terminal(e.to_string()))?;

        let (event_tx, event_rx) = mpsc::channel();

        Ok(Self {
            terminal,
            state: AppState::new(),
            event_tx,
            event_rx,
        })
    }

    /// Run the TUI main loop.
    ///
    /// # Errors
    ///
    /// Returns an error if the TUI encounters an error or the user quits.
    pub fn run(&mut self) -> Result<String, RenderError> {
        loop {
            self.draw()?;

            // Handle events with timeout
            if let Ok(event) = self.event_rx.recv_timeout(Duration::from_millis(100)) {
                match event {
                    TuiEvent::Input(ch) => {
                        self.state.append_input(ch);
                    },
                    TuiEvent::Backspace => {
                        self.state.backspace();
                    },
                    TuiEvent::Enter => {
                        let input = self.state.input().to_string();
                        if !input.is_empty() {
                            self.state.clear_input();
                            return Ok(input);
                        }
                    },
                    TuiEvent::Quit => {
                        return Err(RenderError::RenderFailed("User quit".to_string()));
                    },
                }
            }

            // Check for crossterm events
            if event::poll(Duration::from_millis(10))
                .map_err(|e| RenderError::Terminal(e.to_string()))?
                && let Event::Key(key) =
                    event::read().map_err(|e| RenderError::Terminal(e.to_string()))?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('c') => {
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                        {
                            let _ = self.event_tx.send(TuiEvent::Quit);
                        }
                    },
                    KeyCode::Char(ch) => {
                        let _ = self.event_tx.send(TuiEvent::Input(ch));
                    },
                    KeyCode::Enter => {
                        let _ = self.event_tx.send(TuiEvent::Enter);
                    },
                    KeyCode::Backspace => {
                        let _ = self.event_tx.send(TuiEvent::Backspace);
                    },
                    _ => {},
                }
            }
        }
    }

    /// Draw the UI.
    fn draw(&mut self) -> Result<(), RenderError> {
        self.terminal
            .draw(|f| Self::render_ui(f, &self.state))
            .map_err(|e| RenderError::Terminal(e.to_string()))?;
        Ok(())
    }

    /// Render the UI.
    fn render_ui(f: &mut Frame, state: &AppState) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([Constraint::Min(0), Constraint::Length(3)].as_ref())
            .split(f.area());

        if let (Some(&messages_area), Some(&input_area)) = (chunks.first(), chunks.get(1)) {
            render_messages(f, messages_area, state);
            render_input(f, input_area, state);
        }
    }
}

impl Default for TuiRenderer {
    fn default() -> Self {
        Self::new().expect("Failed to initialize TUI renderer")
    }
}

impl Renderer for TuiRenderer {
    fn render_event(&mut self, event: &ChatEvent) -> Result<(), RenderError> {
        match event {
            ChatEvent::Text(text) => {
                self.state.add_message(Role::Assistant, text.clone());
                self.draw()?;
            },
            ChatEvent::Reasoning(content) | ChatEvent::ReasoningDelta(content) => {
                self.state.set_status(format!("思考中: {content}"));
                self.draw()?;
            },
            ChatEvent::ToolCall { arguments } => {
                self.state
                    .add_message(Role::Tool, format!("执行: {arguments}"));
                self.draw()?;
            },
            ChatEvent::ToolCallDelta(content) => {
                self.state.set_status(format!("工具调用: {content}"));
                self.draw()?;
            },
            ChatEvent::ToolResult {
                content,
                structured: _,
            } => {
                self.state
                    .add_message(Role::Tool, format!("结果: {content}"));
                self.draw()?;
            },
            ChatEvent::Usage(usage) => {
                self.state.set_status(format!(
                    "使用: {} tokens",
                    usage.input_tokens + usage.output_tokens
                ));
                self.draw()?;
            },
            ChatEvent::Done => {
                self.state.set_streaming(false);
                self.draw()?;
            },
            _ => {},
        }
        Ok(())
    }

    fn render_chunk(&mut self, chunk: &str, is_thinking: bool) -> Result<(), RenderError> {
        if is_thinking {
            self.state.set_status(format!("思考: {chunk}"));
        } else {
            self.state.append_to_last(chunk);
        }
        self.draw()?;
        Ok(())
    }

    fn clear(&mut self) -> Result<(), RenderError> {
        self.terminal
            .clear()
            .map_err(|e| RenderError::Terminal(e.to_string()))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.draw()?;
        Ok(())
    }

    fn shutdown(mut self) -> Result<(), RenderError> {
        disable_raw_mode().map_err(|e| RenderError::Terminal(e.to_string()))?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
            .map_err(|e| RenderError::Terminal(e.to_string()))?;
        self.terminal
            .show_cursor()
            .map_err(|e| RenderError::Terminal(e.to_string()))?;
        Ok(())
    }
}

/// Render the messages area.
fn render_messages(f: &mut Frame, area: Rect, state: &AppState) {
    let title = Line::from(vec![Span::styled(
        "聊天",
        Style::default().add_modifier(Modifier::BOLD),
    )]);

    let messages: Vec<Line> = state
        .messages
        .iter()
        .flat_map(|msg| {
            let prefix = match msg.role {
                Role::User => "你",
                Role::Assistant => "助手",
                Role::System => "系统",
                Role::Tool => "工具",
            };
            let style = match msg.role {
                Role::User => Style::default().fg(Color::Cyan),
                Role::Assistant => Style::default().fg(Color::Green),
                Role::System => Style::default().fg(Color::Yellow),
                Role::Tool => Style::default().fg(Color::Gray),
            };

            let mut lines = vec![Line::from(vec![
                Span::styled(format!("[{prefix}]"), style),
                Span::from(" "),
                Span::from(msg.content.clone()),
            ])];

            for line in msg.content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("  ", style),
                    Span::from(line.to_string()),
                ]));
            }

            lines
        })
        .collect();

    let paragraph = Paragraph::new(messages)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Render the input area.
fn render_input(f: &mut Frame, area: Rect, state: &AppState) {
    let input = Paragraph::new(state.input().to_string())
        .block(Block::default().borders(Borders::ALL).title("输入"));

    f.render_widget(input, area);
    let x = area
        .x
        .saturating_add(u16::try_from(state.input().len()).unwrap_or(u16::MAX));
    let x = x.saturating_add(2);
    f.set_cursor_position((x, area.y.saturating_add(1)));
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
