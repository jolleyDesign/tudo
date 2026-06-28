//! Headless render checks using ratatui's TestBackend (no real terminal).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tudo::app::App;
use tudo::ui;

/// Flatten the rendered buffer into a single string for substring assertions.
fn buffer_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut s = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                s.push_str(cell.symbol());
            }
        }
    }
    s
}

fn render_to_string(app: &mut App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    buffer_text(terminal.backend().buffer())
}

#[test]
fn normal_view_shows_lists_and_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Ship the TUI".to_string());

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Lists"), "sidebar title missing: {text}");
    assert!(text.contains("Work"), "list name missing");
    assert!(text.contains("Ship the TUI"), "task title missing");
}

#[test]
fn first_run_screen_is_shown_without_data_dir() {
    let mut app = App::new(None).unwrap();
    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Welcome to tudo"), "first-run header missing");
    assert!(text.contains("Custom path"), "custom path option missing");
}

#[test]
fn detail_view_shows_notes_and_subtasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Parent".to_string());
    app.set_current_notes("remember the milk".to_string());
    app.add_subtask("a subtask".to_string());
    app.open_detail();

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("remember the milk"), "notes missing: {text}");
    assert!(text.contains("a subtask"), "subtask missing");
    assert!(text.contains("Subtasks"), "subtask section header missing");
    assert!(text.contains("0/1"), "subtask progress count missing");
}

#[test]
fn theme_picker_lists_themes() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.open_theme_picker();

    let text = render_to_string(&mut app, 100, 36);
    assert!(text.contains("Theme"), "picker title missing");
    assert!(text.contains("Dracula"), "Dracula missing");
    assert!(text.contains("Gotham"), "Gotham missing");
    assert!(text.contains("Black & White"), "monochrome missing");
    assert!(text.contains("preview"), "picker hint missing");
}

#[test]
fn settings_panel_shows_paths_and_format() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.open_settings();

    let text = render_to_string(&mut app, 110, 30);
    assert!(text.contains("Settings"), "title missing");
    assert!(text.contains("Data directory"), "data dir label missing");
    assert!(text.contains("Config file"), "config label missing");
    assert!(text.contains("JSON"), "format note missing");
    assert!(text.contains("1 lists"), "contents count missing");
}

#[test]
fn help_overlay_lists_keybindings() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.mode = tudo::app::Mode::Help;

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Keybindings"), "help title missing");
    assert!(text.contains("cycle priority"), "help body missing");
}
