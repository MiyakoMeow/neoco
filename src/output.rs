use std::sync::Mutex;

pub type OutputCallback<'a> = Box<dyn Fn(&str) + Send + Sync + 'a>;

pub struct OutputHandler {
    use_stdout: Mutex<bool>,
}

impl Clone for OutputHandler {
    fn clone(&self) -> Self {
        Self {
            use_stdout: Mutex::new(*self.use_stdout.lock().unwrap()),
        }
    }
}

impl OutputHandler {
    pub fn new(_line_count: u16) -> Self {
        Self {
            use_stdout: Mutex::new(true),
        }
    }

    pub fn as_output_callback(&self) -> OutputCallback<'_> {
        let use_stdout = &self.use_stdout;

        Box::new(move |text: &str| {
            let use_stdout_guard = use_stdout.lock().unwrap();
            if *use_stdout_guard {
                #[allow(clippy::print_stdout)]
                {
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

    #[allow(clippy::unused_self, clippy::print_stdout)]
    pub fn render(&self, text: &str) {
        print!("{text}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }

    #[allow(clippy::unused_self)]
    pub fn finalize(self) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
