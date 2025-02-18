use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use langdb_core::usage::{InMemoryStorage, ProviderMetrics};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Terminal,
};
use std::{
    collections::BTreeMap,
    io::{self, stdout},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::sync::{mpsc::Receiver, Mutex};

use crate::LOGO;

pub struct Stats {
    pub total_requests: u64,
}

pub struct TuiState {
    stats: Stats,
    logs: Vec<String>,
    max_lines: u16,
    logs_counter: u64,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            stats: Stats { total_requests: 0 },
            logs: Vec::new(),
            max_lines: 15,
            logs_counter: 0,
        }
    }

    pub fn add_log(&mut self, message: String) {
        self.logs_counter += 1;
        self.logs.push(message);
        self.stats.total_requests += 1;
        if self.logs.len() > self.max_lines as usize {
            self.logs.remove(0);
        }
    }
}

#[derive(Debug, Default)]
pub struct Counters {
    pub total: f64,
    pub prompt: f64,
    pub completion: f64,
    pub cost: f64,
    pub avg_response_time: Option<f64>,
    pub metrics: BTreeMap<String, ProviderMetrics>,
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
        let mut interval = tokio::time::interval(Duration::from_millis(1000));
        interval.tick().await; // Tick immediately to start the interval

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(storage) = storage.try_lock() {
                           let metrics = storage.get_all_counters().await;

                           let mut total = 0.0;
                           let mut prompt = 0.0;
                           let mut completion = 0.0;
                           let mut cost = 0.0;
                           let mut total_requests = 0.0;
                           let mut total_response_time = 0.0;

                           for provider_metrics in metrics.values() {
                               for model_metrics in provider_metrics.models.values() {
                                   total += model_metrics.metrics.total.total_tokens.unwrap_or(0.0);
                                   prompt += model_metrics.metrics.total.input_tokens.unwrap_or(0.0);
                                   completion += model_metrics.metrics.total.output_tokens.unwrap_or(0.0);
                                   cost += model_metrics.metrics.total.llm_usage.unwrap_or(0.0);
                                   total_requests += model_metrics.metrics.total.requests.unwrap_or(0.0);
                                   total_response_time += model_metrics.metrics.total.latency.unwrap_or(0.0);
                               }
                           }

                           let avg_response_time = if total_requests > 0.0 {
                               Some(total_response_time / total_requests)
                           } else {
                               None
                           };

                           if let Ok(mut counters) = counters.write() {
                               counters.total = total;
                               counters.prompt = prompt;
                               counters.completion = completion;
                               counters.cost = cost;
                               counters.avg_response_time = avg_response_time;
                               counters.metrics = metrics;
                           }
                       }
                }
            }
        }
    }

    pub async fn run(&mut self, counters: Arc<RwLock<Counters>>) -> io::Result<()> {
        let tick_rate = Duration::from_millis(200);

        loop {
            // Process all available logs
            while let Ok(log) = self.log_receiver.try_recv() {
                self.state.add_log(log);
            }

            // Draw UI
            self.terminal.draw(|f| {
                let mut blocks = vec![
                    Constraint::Length(8),
                    Constraint::Length(3),
                ];

                let counters = counters.read().unwrap();

                let mut model_names = vec![];
                let mut model_cost = vec![];
                let mut widths = vec![];

                if !counters.metrics.is_empty() {
                    blocks.push(Constraint::Length(4));
                }

                for v in counters.metrics.values() {
                    for (model_name, metric) in &v.models {
                        blocks.push(Constraint::Length(3));
                        model_names.push(Cell::from(model_name.clone()));
                        model_names.push(Cell::from("|"));
                        model_cost.push(Cell::from(
                                format!("{:.4}$",
                                metric.metrics.total.llm_usage.unwrap_or(0.0)
                            )
                        ));
                        model_cost.push(Cell::from("|"));
                        widths.push(Constraint::Min(model_name.len() as u16));
                        widths.push(Constraint::Min(1));
                    }
                }


                blocks.push(Constraint::Min(0));

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        blocks,
                    )
                    .split(f.size());

                let logo = Paragraph::new(LOGO.to_string());
                f.render_widget(logo, chunks[0]);

                // Stats section
                let total_requests = self.state.stats.total_requests;

                // Counters section
                let avg_response_time = counters.avg_response_time.as_ref().map_or(String::from(""), |t| format!(" (avg {:.2?}ms)", t));

                let stats = format!(
                    "Tokens: {} (prompt: {}, completion: {}) | Total Requests: {}{} | Total cost: {:.4}$",
                    counters.total, counters.prompt, counters.completion, total_requests, avg_response_time, counters.cost
                );

                let stats_widget = Paragraph::new(stats)
                    .block(Block::default().borders(Borders::ALL).title("Stats"));
                f.render_widget(stats_widget, chunks[1]);

                let mut index = 2;
                if !counters.metrics.is_empty() {
                    let table = Table::new(vec![Row::new(model_cost)])
                        .header(Row::new(model_names))
                        .widths(&widths)
                        .block(Block::default().borders(Borders::ALL).title("Cost"));
                    f.render_widget(table, chunks[index]);
                    index += 1;
                }

                for (provider_name, v) in &counters.metrics {
                    for (model_name, m) in &v.models {
                        if m.metrics.total.requests.unwrap_or(0.0) > 0.0 {
                            let avg_time = match (m.metrics.total.requests, m.metrics.total.latency) {
                                (Some(requests), Some(duration)) => {
                                    Some(duration / requests)
                                }
                                _ => {
                                    None
                                }
                            };
                            let avg_response_time = avg_time.as_ref().map_or(String::from(""), |t| format!(" (avg {:.2?}ms)", t));
                            let stats = format!(
                                "Tokens: {} (prompt: {}, completion: {}) | Total Requests: {}{} | Total cost: {:.4}$",
                                m.metrics.total.total_tokens.unwrap_or(0.0), m.metrics.total.input_tokens.unwrap_or(0.0), m.metrics.total.output_tokens.unwrap_or(0.0), m.metrics.total.requests.unwrap_or(0.0), avg_response_time, m.metrics.total.llm_usage.unwrap_or(0.0)
                            );
                            let stats_widget = Paragraph::new(stats)
                                .block(Block::default().borders(Borders::ALL).title(format!("{}/{}", provider_name, model_name)));
                            f.render_widget(stats_widget, chunks[index]);
                            index += 1;
                        }
                    }
                }

                // Logs section
                let logs: Vec<Line> = self
                    .state
                    .logs
                    .iter()
                    .map(|log| {
                        let (level_color, log_content) = if log.len() > 13 {
                            let level = &log[..5].trim();
                            let content = &log[5..];
                            if level.contains("ERROR") {
                                (Color::Red, content)
                            } else if level.contains("INFO") {
                                (Color::Green, content)
                            } else {
                                (Color::Yellow, *level)
                            }
                        } else {
                            (Color::White, log.as_str())
                        };
                        Line::from(Span::styled(log_content, Style::default().fg(level_color)))
                    })
                    .collect();

                let logs_widget = Paragraph::new(logs)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!("Logs ({})", self.state.logs_counter)),
                );

                f.render_widget(logs_widget, chunks[index]);
            })?;

            // Use tokio::select! to handle both events and ticks
            tokio::select! {
                _ = tokio::time::sleep(tick_rate) => {
                }
                r = tokio::task::spawn_blocking(move || event::poll(tick_rate)) => {
                    if r.is_ok() && r.unwrap().unwrap() {
                        let event = event::read();
                        match event {
                            Ok(Event::Resize(_, rows)) => {
                                self.state.max_lines = rows - 18;
                            }
                            Ok(Event::Key(KeyEvent {
                                code: KeyCode::Char('c'),
                                modifiers: KeyModifiers::CONTROL,
                                ..
                            })) |
                            Ok(Event::Key(KeyEvent {
                                code: KeyCode::Char('q'),
                                modifiers: KeyModifiers::CONTROL,
                                ..
                            })) => {
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }
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
