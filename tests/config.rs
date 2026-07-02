//! Config-file loading of keybindings, isolated in its own test binary so it
//! can set `$TUDO_CONFIG` without racing the env-sensitive tests elsewhere.

use std::io::Write;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tudo::app::App;
use tudo::config;
use tudo::keybind::{Action, Keymap};

// One combined test: this is the only place that sets `$TUDO_CONFIG`, so keeping
// it to a single test avoids racing on the process-wide env var.
#[test]
fn keybindings_load_from_disk_and_materialize_the_full_set() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let cfg_path = dir.path().join("config.json");
    let mut f = std::fs::File::create(&cfg_path).unwrap();
    // A partial config: only two actions bound, the rest missing.
    write!(
        f,
        r#"{{
          "data_dir": "{}",
          "theme": "dracula",
          "keybindings": {{ "quit": ["Q"], "search": ["ctrl+f"] }}
        }}"#,
        data_dir.display()
    )
    .unwrap();

    // SAFETY: this is the only test in this binary, so nothing else touches env.
    unsafe { std::env::set_var("TUDO_CONFIG", &cfg_path) };

    let loaded = config::load_config().unwrap().expect("config present");
    assert_eq!(
        loaded.keybindings.get("quit").unwrap(),
        &vec!["Q".to_string()]
    );

    // The loaded overrides build a working keymap: 'Q' quits, plain 'q' doesn't.
    let (km, warnings) = Keymap::from_overrides(loaded.keybindings);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(
        km.action_for(KeyEvent::from(KeyCode::Char('Q'))),
        Some(Action::Quit)
    );
    assert_eq!(km.action_for(KeyEvent::from(KeyCode::Char('q'))), None);
    assert_eq!(
        km.action_for(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
        Some(Action::Search)
    );
    assert!(
        !km.covers_all_actions(),
        "partial config isn't complete yet"
    );

    // Startup path: install the keymap + theme, then materialize into the file.
    let mut app = App::new(Some(data_dir)).unwrap();
    app.set_theme(loaded.theme);
    app.set_keymap(km);
    app.materialize_keybindings();

    // The config on disk now lists every action, keeps the user's override, and
    // preserves the theme.
    let refreshed = config::load_config().unwrap().expect("config present");
    unsafe { std::env::remove_var("TUDO_CONFIG") };

    assert_eq!(refreshed.theme, tudo::theme::ThemeKind::Dracula);
    assert_eq!(
        refreshed.keybindings.get("quit").unwrap(),
        &vec!["Q".to_string()]
    );
    assert_eq!(
        refreshed.keybindings.get("move-down").unwrap(),
        &vec!["j".to_string(), "down".to_string()]
    );
    for action in Action::ALL {
        assert!(
            refreshed.keybindings.contains_key(action.config_name()),
            "materialized config missing {}",
            action.config_name()
        );
    }
}
