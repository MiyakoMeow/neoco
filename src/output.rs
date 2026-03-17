use anyhow::Result;
use ratatui::{
    Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, prelude::Widget,
    widgets::Paragraph,
};
use std::sync::Mutex;

pub type OutputCallback<'a> = Box<dyn Fn(&str) + Send + Sync + 'a>;

pub struct OutputHandler {
    terminal: Mutex<Option<Terminal<CrosstermBackend<std::io::Stdout>>>>,
    use_stdout: Mutex<bool>,
}

impl Clone for OutputHandler {
    fn clone(&self) -> Self {
        Self {
            terminal: Mutex::new(None),
            use_stdout: Mutex::new(*self.use_stdout.lock().unwrap()),
        }
    }
}

impl OutputHandler {
    pub fn new(line_count: u16) -> Result<Self> {
        let terminal = Terminal::with_options(
            CrosstermBackend::new(std::io::stdout()),
            TerminalOptions {
                viewport: Viewport::Inline(line_count),
            },
        )?;

        Ok(Self {
            terminal: Mutex::new(Some(terminal)),
            use_stdout: Mutex::new(true),
        })
    }

    pub fn as_output_callback(&self) -> OutputCallback<'_> {
        let use_stdout = &self.use_stdout;

        Box::new(move |text: &str| {
            #[allow(clippy::print_stdout)]
            {
                let use_stdout_guard = use_stdout.lock().unwrap();
                if *use_stdout_guard {
                    print!("{text}");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
            }
        })
    }

    pub fn disable_stdout(&self) {
        let mut use_stdout = self.use_stdout.lock().unwrap();
        *use_stdout = false;
    }

    pub fn render(&self, text: &str) -> Result<()> {
        let mut terminal_guard = self.terminal.lock().unwrap();
        if let Some(ref mut terminal) = *terminal_guard {
            terminal.insert_before(0, |buf| {
                let para = Paragraph::new(text);
                para.render(buf.area, buf);
            })?;
        }
        Ok(())
    }

    pub fn finalize(self) -> Result<()> {
        let mut terminal_guard = self.terminal.lock().unwrap();
        if let Some(ref mut terminal) = *terminal_guard {
            terminal.flush()?;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok(())
    }
}

impl Drop for OutputHandler {
    fn drop(&mut self) {
        let mut terminal_guard = self.terminal.lock().unwrap();
        *terminal_guard = None;
    }
}
