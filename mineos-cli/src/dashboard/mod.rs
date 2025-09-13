mod ui;
pub mod widgets;
mod layout;

use anyhow::Result;
use clap::Args;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::{io, time::Duration};

pub use ui::Dashboard;

/// Real-time monitoring dashboard
#[derive(Args)]
pub struct DashboardArgs {
    /// Update interval in milliseconds
    #[arg(short, long, default_value = "1000")]
    interval: u64,

    /// Start in compact mode
    #[arg(short, long)]
    compact: bool,
}

pub async fn execute(args: DashboardArgs) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create dashboard
    let mut dashboard = Dashboard::new(args.compact);

    // Run dashboard
    let res = run_dashboard(&mut terminal, &mut dashboard, args.interval).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_dashboard(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    dashboard: &mut Dashboard,
    interval_ms: u64,
) -> Result<()> {
    let mut last_update = std::time::Instant::now();
    let update_interval = Duration::from_millis(interval_ms);

    loop {
        // Draw UI
        terminal.draw(|f| dashboard.draw(f))?;

        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('p') => dashboard.toggle_pause(),
                    KeyCode::Char('r') => dashboard.reset_stats(),
                    KeyCode::Char('h') | KeyCode::Char('?') => dashboard.toggle_help(),
                    KeyCode::Char('c') => dashboard.toggle_compact(),
                    KeyCode::Up => dashboard.scroll_up(),
                    KeyCode::Down => dashboard.scroll_down(),
                    KeyCode::Tab => dashboard.next_tab(),
                    _ => {}
                }
            }
        }

        // Update data
        if last_update.elapsed() >= update_interval {
            dashboard.update().await?;
            last_update = std::time::Instant::now();
        }
    }

    Ok(())
}