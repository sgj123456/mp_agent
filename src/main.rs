mod agent;
mod app;
mod config;
mod error;
mod mcp;
mod permission;
mod ui;

use std::io;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
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

    let mut app = App::new(config).await;

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
    // Use crossterm's async EventStream so input events are delivered
    // immediately without blocking on poll(). This eliminates the perceived
    // latency for both keyboard and mouse (wheel) input.
    let mut events = EventStream::new();

    loop {
        // Process any pending agent events first so the UI reflects the latest
        // state before we redraw.
        app.process_agent_events();

        // Draw the current frame.
        app.draw(terminal)?;

        // Wait for the next input event with a short timeout so we still make
        // progress when no input arrives (e.g. for streaming updates). The
        // timeout is only a fallback; events are delivered instantly otherwise.
        tokio::select! {
            Some(Ok(event)) = events.next() => {
                match event {
                    Event::Key(key) => app.handle_key_event(key),
                    Event::Mouse(mouse) => app.handle_mouse_event(mouse),
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // No input arrived within the window; just continue the loop
                // so the streaming buffer and status keep refreshing.
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}
