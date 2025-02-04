use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use std::{
    io::{self, stdout},
    sync::mpsc::channel,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Receiver;

pub struct Stats {
    pub requests: u64,
    pub active_connections: u32,
    pub uptime: Duration,
}

pub struct TuiState {
    stats: Stats,
    logs: Vec<String>,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            stats: Stats {
                requests: 0,
                active_connections: 0,
                uptime: Duration::from_secs(0),
            },
            logs: Vec::new(),
        }
    }

    pub fn add_log(&mut self, message: String) {
        self.logs.push(message);
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }
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

    pub fn run(&mut self) -> io::Result<()> {
        let tick_rate = Duration::from_millis(200);
        let mut last_tick = Instant::now();

        println!("TUI run started");
        loop {
            // Handle logs first
            if let Ok(log) = self.log_receiver.try_recv() {
                // println!("TUI received log: {}", log);
                self.state.add_log(log);
            }

            // Then take the state reference for drawing
            let state = &self.state;
            self.terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                    .split(f.size());

                // Stats section
                let stats = format!(
                    "Requests: {} | Active Connections: {} | Uptime: {:?}",
                    state.stats.requests, state.stats.active_connections, state.stats.uptime
                );
                let stats_widget = Paragraph::new(stats)
                    .block(Block::default().borders(Borders::ALL).title("Stats"));
                f.render_widget(stats_widget, chunks[0]);

                // Logs section
                let logs: Vec<Line> = state
                    .logs
                    .iter()
                    .map(|log| Line::from(vec![Span::raw(log)]))
                    .collect();
                let logs_widget = Paragraph::new(logs)
                    .block(Block::default().borders(Borders::ALL).title("Logs"));
                f.render_widget(logs_widget, chunks[1]);
            })?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }
    }

    fn ui(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(f.size());

        // Stats section
        let stats = format!(
            "Requests: {} | Active Connections: {} | Uptime: {:?}",
            self.state.stats.requests, self.state.stats.active_connections, self.state.stats.uptime
        );
        let stats_widget =
            Paragraph::new(stats).block(Block::default().borders(Borders::ALL).title("Stats"));
        f.render_widget(stats_widget, chunks[0]);

        // Logs section
        let logs: Vec<Line> = self
            .state
            .logs
            .iter()
            .map(|log| Line::from(vec![Span::raw(log)]))
            .collect();
        let logs_widget =
            Paragraph::new(logs).block(Block::default().borders(Borders::ALL).title("Logs"));
        f.render_widget(logs_widget, chunks[1]);
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
