use anyhow::Result;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::collections::VecDeque;

/// GPU monitoring widget
pub struct GpuWidget {
    gpu_index: usize,
    hashrate_history: VecDeque<f64>,
    temperature_history: VecDeque<u32>,
    power_history: VecDeque<u32>,
}

impl GpuWidget {
    pub fn new(gpu_index: usize) -> Self {
        Self {
            gpu_index,
            hashrate_history: VecDeque::with_capacity(60),
            temperature_history: VecDeque::with_capacity(60),
            power_history: VecDeque::with_capacity(60),
        }
    }

    pub fn update(&mut self, stats: &GpuStats) {
        // Keep last 60 samples
        if self.hashrate_history.len() >= 60 {
            self.hashrate_history.pop_front();
        }
        if self.temperature_history.len() >= 60 {
            self.temperature_history.pop_front();
        }
        if self.power_history.len() >= 60 {
            self.power_history.pop_front();
        }

        self.hashrate_history.push_back(stats.hashrate);
        self.temperature_history.push_back(stats.temperature);
        self.power_history.push_back(stats.power_usage);
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let current_hashrate = self.hashrate_history.back().unwrap_or(&0.0);
        let current_temp = self.temperature_history.back().unwrap_or(&0);
        let current_power = self.power_history.back().unwrap_or(&0);

        let temp_color = match current_temp {
            0..=60 => Color::Green,
            61..=75 => Color::Yellow,
            76..=85 => Color::Rgb(255, 165, 0), // Orange
            _ => Color::Red,
        };

        let content = vec![
            Line::from(vec![
                Span::raw("Hashrate: "),
                Span::styled(
                    format!("{:.2} MH/s", current_hashrate / 1_000_000.0),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("Temperature: "),
                Span::styled(
                    format!("{}°C", current_temp),
                    Style::default().fg(temp_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Power: "),
                Span::styled(
                    format!("{} W", current_power),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("Shares: "),
                Span::styled(
                    "0/0/0", // accepted/rejected/stale - placeholder
                    Style::default().fg(Color::Green),
                ),
            ]),
        ];

        let gpu_block = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("GPU {}", self.gpu_index)),
            );

        frame.render_widget(gpu_block, area);
    }
}

/// Statistics widget
pub struct StatsWidget {
    start_time: std::time::Instant,
    total_shares: u64,
    accepted_shares: u64,
    rejected_shares: u64,
    stale_shares: u64,
}

impl StatsWidget {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            total_shares: 0,
            accepted_shares: 0,
            rejected_shares: 0,
            stale_shares: 0,
        }
    }

    pub fn update(&mut self, status: &MinerStatus) {
        self.total_shares = status.total_shares;
        self.accepted_shares = status.accepted_shares;
        self.rejected_shares = status.rejected_shares;
        self.stale_shares = status.stale_shares;
    }

    pub fn reset(&mut self) {
        self.start_time = std::time::Instant::now();
        self.total_shares = 0;
        self.accepted_shares = 0;
        self.rejected_shares = 0;
        self.stale_shares = 0;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let uptime = self.start_time.elapsed();
        let hours = uptime.as_secs() / 3600;
        let minutes = (uptime.as_secs() % 3600) / 60;
        let seconds = uptime.as_secs() % 60;

        let acceptance_rate = if self.total_shares > 0 {
            (self.accepted_shares as f64 / self.total_shares as f64) * 100.0
        } else {
            0.0
        };

        let content = vec![
            Line::from(vec![
                Span::raw("Uptime: "),
                Span::styled(
                    format!("{:02}:{:02}:{:02}", hours, minutes, seconds),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Shares: "),
                Span::styled(
                    format!("{}", self.accepted_shares),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("/"),
                Span::styled(
                    format!("{}", self.rejected_shares),
                    Style::default().fg(Color::Red),
                ),
                Span::raw("/"),
                Span::styled(
                    format!("{}", self.stale_shares),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("Acceptance: "),
                Span::styled(
                    format!("{:.1}%", acceptance_rate),
                    Style::default().fg(if acceptance_rate >= 95.0 {
                        Color::Green
                    } else if acceptance_rate >= 90.0 {
                        Color::Yellow
                    } else {
                        Color::Red
                    }),
                ),
            ]),
        ];

        let stats_block = Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Statistics"));

        frame.render_widget(stats_block, area);
    }
}

/// Log widget
pub struct LogWidget {
    logs: VecDeque<LogEntry>,
    scroll_offset: usize,
}

impl LogWidget {
    pub fn new() -> Self {
        Self {
            logs: VecDeque::with_capacity(100),
            scroll_offset: 0,
        }
    }

    pub fn update(&mut self, new_logs: Vec<LogEntry>) {
        for log in new_logs {
            if self.logs.len() >= 100 {
                self.logs.pop_front();
            }
            self.logs.push_back(log);
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        let max_offset = self.logs.len().saturating_sub(10);
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .logs
            .iter()
            .skip(self.scroll_offset)
            .take(area.height as usize - 2)
            .map(|log| {
                let color = match log.level.as_str() {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::White,
                    "DEBUG" => Color::Gray,
                    _ => Color::White,
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}]", log.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" "),
                    Span::styled(log.message.clone(), Style::default().fg(color)),
                ]))
            })
            .collect();

        let logs_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Logs"));

        frame.render_widget(logs_list, area);
    }
}

// Data structures used by widgets
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GpuStats {
    pub index: usize,
    pub hashrate: f64,
    pub temperature: u32,
    pub power_usage: u32,
    pub fan_speed: u32,
    pub memory_usage: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MinerStatus {
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
    pub stale_shares: u64,
    pub total_hashrate: f64,
    pub pool_connected: bool,
    pub is_mining: bool,
    pub algorithm: String,
    pub total_hashrate_mhs: f64,
    pub active_gpus: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}