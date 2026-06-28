//! tudo — a local-first terminal todo list.

use std::io::{self, Stdout};

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, read};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use tudo::app::App;
use tudo::config::Startup;
use tudo::theme::ThemeKind;
use tudo::{config, event, ui};

type Term = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<()> {
    let (data_dir, mut theme) = match config::resolve()? {
        Startup::Open { data_dir, theme } => (Some(data_dir), theme),
        Startup::FirstRun { theme } => (None, theme),
    };
    // $TUDO_THEME overrides the saved theme for this run.
    if let Some(kind) = std::env::var("TUDO_THEME")
        .ok()
        .and_then(|s| ThemeKind::from_key(&s))
    {
        theme = kind;
    }
    let mut app = App::new(data_dir)?;
    app.set_theme(theme);

    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::render(f, app))?;
        match read()? {
            Event::Key(key) => event::handle_key(app, key),
            Event::Mouse(m) => event::handle_mouse(app, m),
            _ => {}
        }
    }
    Ok(())
}

fn setup_terminal() -> Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Term) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

/// Restore the terminal on panic so a crash doesn't leave a broken shell.
fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original(info);
    }));
}
