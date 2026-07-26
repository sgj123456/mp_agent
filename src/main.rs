#[allow(dead_code)]
mod agent;
mod app;
mod config;
mod error;
#[allow(dead_code)]
mod mcp;
#[allow(dead_code)]
mod permission;
#[allow(dead_code)]
mod ui;

use std::io;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::App;
use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    error::install_hooks();

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("mp_agent.log")
        .expect("Failed to open log file");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mp_agent=info".into()),
        )
        .with_writer(std::sync::Mutex::new(log_file))
        .with_ansi(false)
        .init();

    let config = Config::from_env()?;
    tracing::info!("Loaded config, model: {}", config.model);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);

    let result = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.draw(terminal)?;

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => app.handle_key_event(key),
                Event::Mouse(mouse) => app.handle_mouse_event(mouse),
                _ => {}
            }
        }

        app.process_agent_events();

        if !app.running {
            break;
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    Ok(())
}
