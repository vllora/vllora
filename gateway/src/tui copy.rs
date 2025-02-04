use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use langdb_core::usage::InMemoryStorage;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::{
    fs::OpenOptions,
    io::Write,
    io::{self, stdout},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc::Receiver, Mutex};

pub struct Stats {
    pub uptime: Duration,
    pub total_logs: u64,
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
                total_logs: 0,
            },
            logs: Vec::new(),
        }
    }

    pub fn add_log(&mut self, message: String) {
        self.logs.push(message);
        self.stats.total_logs += 1;
        if self.logs.len() > 15 {
            self.logs.remove(0);
        }
    }
}

pub struct Tui {
    terminal: Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state: TuiState,
    storage: Arc<Mutex<InMemoryStorage>>,
    log_receiver: Receiver<String>,
}

impl Tui {
    pub fn new(
        log_receiver: Receiver<String>,
        storage: Arc<Mutex<InMemoryStorage>>,
    ) -> io::Result<Self> {
        let mut stdout = stdout();
        stdout.execute(EnterAlternateScreen)?;
        enable_raw_mode()?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            state: TuiState::new(),
            storage,
            log_receiver,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        let tick_rate = Duration::from_millis(200);
        let mut last_tick = Instant::now();
        let start_time = Instant::now();

        let mut debug_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open("tui_debug.log")?;

        writeln!(debug_file, "TUI run starting...")?;

        // Add a test log to verify display
        self.state.add_log("TUI Started".to_string());

        loop {
            writeln!(debug_file, "\n--- New tick ---")?;

            // Process all available logs
            while let Ok(log) = self.log_receiver.try_recv() {
                writeln!(debug_file, "Received log: {}", log)?;
                self.state.add_log(log);
                writeln!(debug_file, "Current log count: {}", self.state.logs.len())?;
            }

            // Update uptime
            self.state.stats.uptime = start_time.elapsed();

            // Draw UI
            if let Err(e) = self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                    .split(f.size());

                // Stats section
                let total_requests = if let Ok(storage) = self.storage.try_lock() {
                    0.0 // For now, just show 0
                } else {
                    0.0
                };

                let stats = format!(
                    "Total Requests: {:.0} | Total Logs: {} | Uptime: {:?}",
                    total_requests, self.state.stats.total_logs, self.state.stats.uptime
                );
                let stats_widget = Paragraph::new(stats)
                    .block(Block::default().borders(Borders::ALL).title("Stats"));
                f.render_widget(stats_widget, chunks[0]);

                // Logs section with scroll
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
                f.render_widget(logs_widget, chunks[1]);
            }) {
                writeln!(debug_file, "Draw error: {}", e)?;
            }

            writeln!(debug_file, "Checking events...")?;
            // Handle events with a timeout
            if let Err(e) = crossterm::event::poll(tick_rate) {
                writeln!(debug_file, "Event poll error: {}", e)?;
            } else if let Ok(Event::Key(key)) = crossterm::event::read() {
                writeln!(debug_file, "Key event: {:?}", key)?;
                if key.code == KeyCode::Char('q') {
                    writeln!(debug_file, "Quit requested")?;
                    disable_raw_mode()?;
                    return Ok(());
                }
            }

            // Sleep for the remaining time
            if let Some(timeout) = tick_rate.checked_sub(last_tick.elapsed()) {
                writeln!(debug_file, "Sleeping for {:?}", timeout)?;
                std::thread::sleep(timeout);
            }
            last_tick = Instant::now();
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
