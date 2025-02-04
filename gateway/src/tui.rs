use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use langdb_core::usage::InMemoryStorage;
use langdb_core::usage::LimitPeriod;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{
    fs::OpenOptions,
    io::{self, stdout, Write},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc::Receiver, Mutex};

use crate::LOGO;

pub struct Stats {
    pub uptime: Duration,
    pub total_requests: u64,
}

pub struct TuiState {
    stats: Stats,
    logs: Vec<String>,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            stats: Stats {
                uptime: Duration::from_secs(0),
                total_requests: 0,
            },
            logs: Vec::new(),
        }
    }

    pub fn add_log(&mut self, message: String) {
        self.logs.push(message);
        self.stats.total_requests += 1;
        if self.logs.len() > 15 {
            self.logs.remove(0);
        }
    }
}

#[derive(Default, Debug)]
pub struct Counters {
    total: f64,
    prompt: f64,
    completion: f64,
}

pub struct Tui {
    terminal: Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state: TuiState,
    log_receiver: Receiver<String>,
}

impl Tui {
    pub fn new(log_receiver: Receiver<String>) -> io::Result<Self> {
        let mut stdout = stdout();
        stdout.execute(EnterAlternateScreen)?;
        enable_raw_mode()?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            state: TuiState::new(),
            log_receiver,
        })
    }
    pub async fn spawn_counter_loop(
        storage: Arc<Mutex<InMemoryStorage>>,
        counters: Arc<RwLock<Counters>>,
    ) -> io::Result<()> {
        let mut debug_file = OpenOptions::new().append(true).open("tui_debug.log")?;

        writeln!(debug_file, "Spawn loop starting...")?;

        loop {
            if let Ok(storage) = storage.try_lock() {
                let total = storage
                    .get_value(LimitPeriod::Total, "default", "llm_usage")
                    .await
                    .unwrap_or(0.0);
                let prompt = storage
                    .get_value(LimitPeriod::Total, "default", "llm_usage")
                    .await
                    .unwrap_or(0.0);
                let completion = storage
                    .get_value(LimitPeriod::Total, "default", "llm_usage")
                    .await
                    .unwrap_or(1.0);

                let mut counters = counters.write().unwrap();
                counters.total = total;
                counters.prompt = prompt;
                counters.completion = completion;
            } else {
                writeln!(debug_file, "Cannot get storage...").unwrap();
            }
            if let Err(e) = writeln!(debug_file, "Spawn loop...") {
                eprint!("{e}");
                std::process::exit(1);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn run(&mut self, _counters: Arc<RwLock<Counters>>) -> io::Result<()> {
        let tick_rate = Duration::from_millis(200);
        let mut last_tick = Instant::now();
        let start_time = Instant::now();

        let mut debug_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("tui_debug.log")?;

        writeln!(debug_file, "TUI run starting...")?;

        loop {
            // Update uptime first
            self.state.stats.uptime = start_time.elapsed();

            // Process all available logs
            while let Ok(log) = self.log_receiver.try_recv() {
                self.state.add_log(log);
            }

            // Draw UI
            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        [
                            Constraint::Length(8),
                            Constraint::Length(5),
                            Constraint::Min(0),
                        ]
                        .as_ref(),
                    )
                    .split(f.size());

                let logo = Paragraph::new(LOGO.to_string());
                f.render_widget(logo, chunks[0]);

                // Stats section
                let total_requests = self.state.stats.total_requests;
                let stats = format!(
                    "Total Requests: {} | Uptime: {:.2?}",
                    total_requests, self.state.stats.uptime
                );
                let stats_widget = Paragraph::new(stats)
                    .block(Block::default().borders(Borders::ALL).title("Stats"));
                f.render_widget(stats_widget, chunks[1]);

                // // Counters section
                // let counters = counters.read().unwrap();
                // let counters_text = format!(
                //     "Tokens: {}\nPrompt Tokens: {}\nCompletion Tokens: {}",
                //     counters.total, counters.prompt, counters.completion
                // );
                // let counters_widget = Paragraph::new(counters_text)
                //     .block(Block::default().borders(Borders::ALL).title("Counters"));
                // f.render_widget(counters_widget, chunks[1]);

                // Logs section
                let logs: Vec<Line> = self
                    .state
                    .logs
                    .iter()
                    .map(|log| Line::from(vec![Span::raw(log)]))
                    .collect();
                let logs_widget = Paragraph::new(logs).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Logs ({})", self.state.logs.len())),
                );
                f.render_widget(logs_widget, chunks[2]);
            })?;

            // Handle events with a timeout
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                if let Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }) = event::read()?
                {
                    return Ok(());
                }

                if let Event::Key(KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                }) = event::read()?
                {
                    return Ok(());
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        disable_raw_mode().unwrap();
        stdout()
            .execute(LeaveAlternateScreen)
            .expect("Could not leave alternate screen");
    }
}
