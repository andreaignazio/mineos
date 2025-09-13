use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::client::MinerClient;
use super::widgets::{GpuWidget, StatsWidget, LogWidget};

pub struct Dashboard {
    compact: bool,
    paused: bool,
    show_help: bool,
    current_tab: usize,
    client: Option<MinerClient>,
    gpu_widgets: Vec<GpuWidget>,
    stats_widget: StatsWidget,
    log_widget: LogWidget,
}

impl Dashboard {
    pub fn new(compact: bool) -> Self {
        Self {
            compact,
            paused: false,
            show_help: false,
            current_tab: 0,
            client: None,
            gpu_widgets: Vec::new(),
            stats_widget: StatsWidget::new(),
            log_widget: LogWidget::new(),
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(10),    // Main content
                Constraint::Length(3),  // Footer
            ])
            .split(frame.size());

        self.draw_header(frame, chunks[0]);
        self.draw_content(frame, chunks[1]);
        self.draw_footer(frame, chunks[2]);

        if self.show_help {
            self.draw_help(frame);
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("MineOS ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw("v"),
                Span::raw(env!("CARGO_PKG_VERSION")),
                Span::raw(" | "),
                Span::styled(
                    if self.paused { "PAUSED" } else { "RUNNING" },
                    Style::default().fg(if self.paused { Color::Yellow } else { Color::Green })
                ),
            ]),
        ])
        .block(Block::default().borders(Borders::ALL));

        frame.render_widget(header, area);
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(60),  // GPUs
                Constraint::Percentage(40),  // Stats/Logs
            ])
            .split(area);

        // GPU section
        self.draw_gpus(frame, chunks[0]);

        // Bottom section (stats and logs)
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(chunks[1]);

        self.stats_widget.draw(frame, bottom_chunks[0]);
        self.log_widget.draw(frame, bottom_chunks[1]);
    }

    fn draw_gpus(&mut self, frame: &mut Frame, area: Rect) {
        let gpu_count = self.gpu_widgets.len();
        if gpu_count == 0 {
            let no_gpus = Paragraph::new("No GPUs detected")
                .block(Block::default().borders(Borders::ALL).title("GPUs"));
            frame.render_widget(no_gpus, area);
            return;
        }

        let constraints: Vec<Constraint> = (0..gpu_count)
            .map(|_| Constraint::Percentage((100 / gpu_count) as u16))
            .collect();

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area);

        for (i, gpu_widget) in self.gpu_widgets.iter_mut().enumerate() {
            gpu_widget.draw(frame, chunks[i]);
        }
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let footer = Paragraph::new(vec![
            Line::from(vec![
                Span::raw("[Q]uit "),
                Span::raw("[P]ause "),
                Span::raw("[R]eset "),
                Span::raw("[H]elp "),
                Span::raw("[C]ompact "),
                Span::raw("[Tab] Next"),
            ]),
        ])
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));

        frame.render_widget(footer, area);
    }

    fn draw_help(&self, frame: &mut Frame) {
        // Draw help overlay in center
        let area = centered_rect(60, 40, frame.size());

        let help_text = vec![
            Line::from("Keyboard Shortcuts"),
            Line::from(""),
            Line::from("Q/Esc    - Quit dashboard"),
            Line::from("P        - Pause/Resume updates"),
            Line::from("R        - Reset statistics"),
            Line::from("H/?      - Toggle this help"),
            Line::from("C        - Toggle compact mode"),
            Line::from("Tab      - Switch tabs"),
            Line::from("↑/↓      - Scroll logs"),
            Line::from(""),
            Line::from("Press any key to close"),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Help")
                    .style(Style::default().bg(Color::Black))
            )
            .style(Style::default().fg(Color::White));

        frame.render_widget(help, area);
    }

    pub async fn update(&mut self) -> Result<()> {
        if self.paused {
            return Ok(());
        }

        // Connect to miner if not connected
        if self.client.is_none() {
            self.client = MinerClient::connect().await.ok();
        }

        if let Some(ref client) = self.client {
            // Update GPU data
            let gpu_stats = client.get_gpu_statistics().await?;

            // Create/update GPU widgets
            if self.gpu_widgets.len() != gpu_stats.len() {
                self.gpu_widgets.clear();
                for gpu in &gpu_stats {
                    self.gpu_widgets.push(GpuWidget::new(gpu.index));
                }
            }

            for (widget, stats) in self.gpu_widgets.iter_mut().zip(gpu_stats.iter()) {
                widget.update(stats);
            }

            // Update stats
            let status = client.get_status().await?;
            self.stats_widget.update(&status);

            // Update logs
            let logs = client.get_recent_logs(10).await?;
            self.log_widget.update(logs);
        }

        Ok(())
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn reset_stats(&mut self) {
        self.stats_widget.reset();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn toggle_compact(&mut self) {
        self.compact = !self.compact;
    }

    pub fn scroll_up(&mut self) {
        self.log_widget.scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.log_widget.scroll_down();
    }

    pub fn next_tab(&mut self) {
        self.current_tab = (self.current_tab + 1) % 3;
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}