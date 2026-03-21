//! Syntax highlighting for code blocks in the TUI.
//!
//! This module provides syntax highlighting using syntect, converting
//! highlighted code into ratatui-compatible styled text.

use std::fmt;

use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// A syntax highlighter for code blocks.
pub struct CodeHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl fmt::Debug for CodeHighlighter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeHighlighter")
            .field("syntaxes", &self.syntax_set.syntaxes().len())
            .finish_non_exhaustive()
    }
}

impl CodeHighlighter {
    /// Create a new code highlighter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Highlight code with the given language.
    ///
    /// Returns a vector of styled lines.
    #[must_use]
    pub fn highlight(&self, code: &str, language: Option<&str>) -> Vec<Line<'static>> {
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = self
            .theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| self.theme_set.themes.values().next())
            .expect("at least one theme should be available");
        let mut highlighter = HighlightLines::new(syntax, theme);

        code.lines()
            .map(|line| {
                let ranges = highlighter.highlight_line(line, &self.syntax_set);
                match ranges {
                    Ok(ranges) => {
                        let spans: Vec<Span<'static>> = ranges
                            .into_iter()
                            .map(|(style, text)| {
                                let fg = style_to_color(style.foreground);
                                Span::styled(text.to_string(), Style::default().fg(fg))
                            })
                            .collect();
                        Line::from(spans)
                    },
                    Err(_) => Line::raw(line.to_string()),
                }
            })
            .collect()
    }
}

impl Default for CodeHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert syntect color to ratatui color.
fn to_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Convert syntect highlighting style to ratatui color.
fn style_to_color(c: syntect::highlighting::Color) -> Color {
    to_color(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlighter_creation() {
        let highlighter = CodeHighlighter::new();
        // Just verify we can create the highlighter
        let lines = highlighter.highlight("test", None);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_highlight_rust_code() {
        let highlighter = CodeHighlighter::new();
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let lines = highlighter.highlight(code, Some("Rust"));
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_highlight_plain_text() {
        let highlighter = CodeHighlighter::new();
        let code = "just plain text";
        let lines = highlighter.highlight(code, None);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_highlight_unknown_language() {
        let highlighter = CodeHighlighter::new();
        let code = "some code";
        let lines = highlighter.highlight(code, Some("unknown_language_xyz"));
        assert_eq!(lines.len(), 1);
    }
}
