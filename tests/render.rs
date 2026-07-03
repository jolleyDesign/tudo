//! Headless render checks using ratatui's TestBackend (no real terminal).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tudo::app::{App, Mode};
use tudo::keybind::{Keymap, Overrides};
use tudo::model::Priority;
use tudo::{theme, ui};

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
    buffer_text(&render_to_buffer(app, w, h))
}

fn render_to_buffer(app: &mut App, w: u16, h: u16) -> Buffer {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

/// Column/row of the first cell where `needle` starts on a single row.
fn find_needle(buf: &Buffer, needle: &str) -> (u16, u16) {
    let area = buf.area;
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            if let Some(cell) = buf.cell((x, y)) {
                row.push_str(cell.symbol());
            }
        }
        if let Some(byte) = row.find(needle) {
            return (row[..byte].chars().count() as u16, y);
        }
    }
    panic!("{needle:?} not found in rendered buffer");
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
fn quick_add_input_highlights_its_markers() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());

    // Open the add-task input and seed a quick-add string.
    app.start_add_task();
    if let Mode::Input(input) = &mut app.mode {
        input.buffer = "Ship !! #work".to_string();
        input.cursor = input.buffer.chars().count();
    } else {
        panic!("start_add_task did not open the input");
    }

    let buf = render_to_buffer(&mut app, 100, 30);
    let (col, row) = find_needle(&buf, "Ship !! #work");
    let fg_at = |dx: u16| buf.cell((col + dx, row)).unwrap().fg;

    // Offsets within "Ship !! #work": S=0, first !=5, #=8.
    assert_eq!(
        fg_at(0),
        theme::fg(),
        "plain title should use the base colour"
    );
    assert_eq!(
        fg_at(5),
        theme::priority_color(Priority::Medium),
        "!! should use the medium-priority colour"
    );
    assert_eq!(fg_at(8), theme::purple(), "#tag should use the tag colour");
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
fn copy_menu_lists_copy_options() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Ship the TUI".to_string());
    app.start_copy();

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Copy"), "copy menu title missing: {text}");
    assert!(text.contains("Ship the TUI"), "task title missing in menu");
    assert!(text.contains("Full task (JSON)"), "JSON option missing");
    assert!(
        text.contains("Description (notes)"),
        "description option missing"
    );
    assert!(text.contains("Enter copy"), "menu hint missing");
}

#[test]
fn sort_picker_lists_sort_options() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Ship the TUI".to_string());
    app.open_sort_picker();

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Sort tasks"), "picker title missing: {text}");
    assert!(text.contains("priority"), "priority option missing");
    assert!(text.contains("due date"), "due date option missing");
    assert!(text.contains("newest first"), "created description missing");
    assert!(text.contains("Enter apply"), "picker hint missing");
}

#[test]
fn footer_shows_active_sort_only_when_not_manual() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Ship the TUI".to_string());

    // Manual (the default) stays out of the footer.
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        !text.contains("sort:"),
        "manual sort should be hidden: {text}"
    );

    // Any other sort is surfaced.
    app.set_sort(tudo::model::Sort::Priority);
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("sort:priority"),
        "active sort missing: {text}"
    );
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
    // The completed-to-bottom preference and its toggle key are shown.
    assert!(text.contains("Completed"), "completed label missing");
    assert!(text.contains("stay in place"), "completed state missing");
    assert!(
        text.contains("toggle sinking completed tasks"),
        "completed toggle hint missing"
    );
}

#[test]
fn help_overlay_lists_keybindings() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.mode = tudo::app::Mode::Help;

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Keybindings"), "help title missing");
    // Grouped, two-column layout: check a section header and a binding.
    assert!(
        text.contains("Navigate"),
        "help section header missing: {text}"
    );
    assert!(text.contains("priority"), "help body missing");
    assert!(
        text.contains("press any key to close"),
        "help footer missing"
    );
}

#[test]
fn help_overlay_reflects_custom_keybindings() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());

    // Rebind search to a distinctive chord; the help must show it, not the "/".
    let mut ov = Overrides::new();
    ov.insert("search".to_string(), vec!["ctrl+f".to_string()]);
    app.set_keymap(Keymap::from_overrides(ov).0);
    app.mode = tudo::app::Mode::Help;

    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("Ctrl+f"),
        "help should show the rebound search key: {text}"
    );
}
