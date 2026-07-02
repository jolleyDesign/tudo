//! tudo — a local-first terminal todo list.

use std::io::{self, Stdout};
use std::time::Instant;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, poll, read};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use tudo::app::App;
use tudo::config::Startup;
use tudo::keybind::Keymap;
use tudo::theme::ThemeKind;
use tudo::{config, event, ui};

type Term = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<()> {
    // Handle `--version` / `-V` before touching the terminal.
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let (data_dir, mut theme, keybindings) = match config::resolve()? {
        Startup::Open {
            data_dir,
            theme,
            keybindings,
        } => (Some(data_dir), theme, keybindings),
        Startup::FirstRun { theme, keybindings } => (None, theme, keybindings),
    };
    // $TUDO_THEME overrides the saved theme for this run.
    if let Some(kind) = std::env::var("TUDO_THEME")
        .ok()
        .and_then(|s| ThemeKind::from_key(&s))
    {
        theme = kind;
    }
    let (keymap, keymap_warnings) = Keymap::from_overrides(keybindings);
    let mut app = App::new(data_dir)?;
    app.set_theme(theme);
    app.set_keymap(keymap);
    // Write the full set of keybindings into an existing config so the user can
    // see and edit them all. Skipped under $TUDO_DIR (ephemeral / no config to
    // grow) so those runs don't create or repoint the saved config.
    if std::env::var_os("TUDO_DIR").is_none() {
        app.materialize_keybindings();
    }
    // Surface any bad keybinding entries once on the status line (the config is
    // still usable — the offending entries are just ignored).
    if !keymap_warnings.is_empty() {
        app.set_status(format!(
            "{} keybinding(s) in config ignored (press S for the config path)",
            keymap_warnings.len()
        ));
    }

    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let result = run(&mut terminal, &mut app);
    restore_terminal(&mut terminal)?;
    result
}

fn run(terminal: &mut Term, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::render(f, app))?;

        // Block until the next event, but if a transient status message is
        // showing, wake up at its deadline so we can clear it without input.
        let event_ready = match app.status_deadline() {
            Some(deadline) => poll(deadline.saturating_duration_since(Instant::now()))?,
            None => true,
        };
        if event_ready {
            match read()? {
                Event::Key(key) => event::handle_key(app, key),
                Event::Mouse(m) => event::handle_mouse(app, m),
                _ => {}
            }
        }
        app.expire_status(Instant::now());
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
