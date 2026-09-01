//! Headless render checks using ratatui's TestBackend (no real terminal).

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::style::Modifier;
use tudo::app::{App, Mode, Tab};
use tudo::keybind::{Keymap, Overrides};
use tudo::model::Priority;
use tudo::{event, theme, ui};

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
fn tag_view_shows_tags_sidebar_and_cross_list_pane() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Ship the TUI #urgent".to_string());

    // A "Tags" section with the tag appears beneath the lists.
    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Tags"), "tags section header missing: {text}");
    assert!(text.contains("#urgent"), "tag row missing: {text}");

    // Selecting the tag row switches the pane to the cross-list view, which
    // names the tag and labels each task with its home list.
    let row = app
        .sidebar_rows()
        .iter()
        .position(|r| matches!(r, tudo::app::SidebarRow::Tag(t) if t == "urgent"))
        .unwrap();
    app.select_sidebar_row(row);
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("#urgent (all lists)"),
        "tag-view title missing: {text}"
    );
    assert!(text.contains("· Work"), "origin-list label missing: {text}");
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
    assert!(text.contains("points"), "points option missing");
    assert!(text.contains("due date"), "due date option missing");
    assert!(text.contains("newest first"), "created description missing");
    // Direction control is surfaced in the picker.
    assert!(text.contains("direction"), "direction line missing: {text}");
    assert!(text.contains("ascending"), "direction state missing");
    assert!(text.contains("flip"), "direction flip hint missing");
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

    // Any other sort is surfaced, along with an ascending direction arrow.
    app.set_sort(tudo::model::Sort::Priority);
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("sort:priority"),
        "active sort missing: {text}"
    );
    assert!(text.contains('\u{2191}'), "ascending arrow missing: {text}");

    // Flipping direction swaps the arrow (no High/Low task here, so the only
    // arrows in view come from the footer indicator).
    app.toggle_sort_dir();
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("sort:priority"),
        "active sort missing: {text}"
    );
    assert!(
        text.contains('\u{2193}'),
        "descending arrow missing: {text}"
    );
}

#[test]
fn task_line_and_detail_show_story_points() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.add_list("Work".to_string());
    app.add_task("Estimated task ~5".to_string());

    // The list row shows a compact "5pt" chip.
    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("5pt"), "points chip missing from row: {text}");

    // The detail view spells it out under the meta line.
    app.open_detail();
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("points 5"),
        "points missing from detail: {text}"
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
fn notes_tab_renders_tab_strip_notebooks_and_notes() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(tudo::app::Tab::Notes);
    app.add_notebook("Journal".to_string());
    app.add_note("Morning pages".to_string());

    let text = render_to_string(&mut app, 100, 30);
    // Tab strip is present with both tabs numbered.
    assert!(text.contains("1 Tasks"), "tab strip missing Tasks: {text}");
    assert!(text.contains("2 Notes"), "tab strip missing Notes: {text}");
    // Notebook and note both show.
    assert!(text.contains("Notebooks"), "notebooks pane missing: {text}");
    assert!(text.contains("Journal"), "notebook name missing: {text}");
    assert!(text.contains("Morning pages"), "note title missing: {text}");
}

#[test]
fn opening_a_note_opens_the_fullscreen_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf().into()).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("Journal".to_string());
    app.add_note("Deep thoughts".to_string());
    app.set_current_note_body("the body of the note".to_string());
    app.note_focus_notes();
    app.note_activate(); // Enter -> straight into the full-screen body editor

    assert!(matches!(app.mode, Mode::Input(_)), "should be editing");
    let text = render_to_string(&mut app, 100, 30);
    assert!(
        text.contains("Note · Deep thoughts"),
        "editor title: {text}"
    );
    assert!(
        text.contains("the body of the note"),
        "body missing: {text}"
    );
    assert!(text.contains("Ctrl+S"), "editor hint missing: {text}");
    // Full-screen: the sidebar is covered.
    assert!(
        !text.contains("Notebooks"),
        "sidebar should be hidden: {text}"
    );
}

#[test]
fn clicking_the_header_tab_switches() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    // Render once so the tab-chip rects are recorded.
    let _ = render_to_string(&mut app, 100, 30);
    let notes_rect = app
        .clickables
        .tab_rects
        .iter()
        .find(|(t, _)| *t == Tab::Notes)
        .map(|(_, r)| *r)
        .expect("Notes tab rect recorded");

    event::handle_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: notes_rect.x,
            row: notes_rect.y,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert_eq!(app.active_tab, Tab::Notes);
}

#[test]
fn markdown_markers_are_dimmed_in_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("T".to_string());
    app.set_current_note_body("# Heading".to_string());
    app.note_focus_notes();
    app.note_activate(); // full-screen editor

    let buf = render_to_buffer(&mut app, 100, 30);
    let (col, row) = find_needle(&buf, "# Heading");
    // The '#' marker recedes (muted); the heading text is bright (accent).
    assert_eq!(
        buf.cell((col, row)).unwrap().fg,
        theme::muted(),
        "# marker should be dimmed"
    );
    assert_eq!(
        buf.cell((col + 2, row)).unwrap().fg,
        theme::accent(),
        "heading text should be accent"
    );
}

#[test]
fn read_view_renders_markdown_and_hides_markers() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Target".to_string());
    app.add_note("Doc".to_string());
    app.set_current_note_body("# Heading\n\nSee [[Target]] here.".to_string());
    app.note_focus_notes();
    app.open_note_view();

    let text = render_to_string(&mut app, 80, 20);
    assert!(text.contains("Heading"), "heading text shown: {text}");
    assert!(!text.contains("# Heading"), "heading marker hidden: {text}");
    assert!(text.contains("Target"), "link text shown: {text}");
    assert!(!text.contains("[[Target]]"), "link brackets hidden: {text}");
}

#[test]
fn read_view_styles_headings_and_inline() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Doc".to_string());
    app.set_current_note_body("# One\n## Two\n\n~~gone~~ *slanted*".to_string());
    app.note_focus_notes();
    app.open_note_view();

    let buf = render_to_buffer(&mut app, 80, 20);

    // Every heading level renders the same: bold, not underlined.
    let (c1, r1) = find_needle(&buf, "One");
    let h1 = buf.cell((c1, r1)).unwrap().modifier;
    let (c2, r2) = find_needle(&buf, "Two");
    let h2 = buf.cell((c2, r2)).unwrap().modifier;
    assert!(
        h1.contains(Modifier::BOLD) && h2.contains(Modifier::BOLD),
        "headings are bold"
    );
    assert!(
        !h1.contains(Modifier::UNDERLINED) && !h2.contains(Modifier::UNDERLINED),
        "headings are uniform (no per-level underline)"
    );

    // Strikethrough and italic render (markers hidden).
    let text = buffer_text(&buf);
    assert!(
        text.contains("gone") && !text.contains("~~"),
        "strike markers hidden: {text}"
    );
    assert!(text.contains("slanted"), "italic text shown: {text}");
    let (cs, rs) = find_needle(&buf, "gone");
    assert!(
        buf.cell((cs, rs))
            .unwrap()
            .modifier
            .contains(Modifier::CROSSED_OUT),
        "strikethrough applied"
    );
    let (ci, ri) = find_needle(&buf, "slanted");
    assert!(
        buf.cell((ci, ri))
            .unwrap()
            .modifier
            .contains(Modifier::ITALIC),
        "italic applied"
    );
}

#[test]
fn note_link_picker_renders() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Target".to_string());
    app.add_note("Doc".to_string());
    app.set_current_note_body("[[Target]]".to_string());
    app.start_note_link_picker(false);

    let text = render_to_string(&mut app, 80, 24);
    assert!(text.contains("Follow link"), "picker title: {text}");
    assert!(text.contains("Target"), "link entry: {text}");
}

#[test]
fn long_note_editor_scrolls_to_the_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("T".to_string());
    let body = (0..100)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    app.set_current_note_body(body);
    app.note_focus_notes();
    app.note_activate(); // opens editor with the cursor at the end

    let text = render_to_string(&mut app, 80, 20);
    assert!(text.contains("line 99"), "should scroll to the end: {text}");
    assert!(!text.contains("line 0"), "the top should be scrolled off");
}

#[test]
fn note_copy_menu_renders() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(Some(dir.path().to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Copy me".to_string());
    app.note_focus_notes();
    app.start_note_copy();

    let text = render_to_string(&mut app, 80, 24);
    assert!(text.contains("Copy me"), "note title in menu: {text}");
    assert!(
        text.contains("Body (markdown)"),
        "body option missing: {text}"
    );
    assert!(
        text.contains("Full note"),
        "markdown option missing: {text}"
    );
}

#[test]
fn note_sort_picker_renders() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf().into()).unwrap();
    app.switch_tab(tudo::app::Tab::Notes);
    app.add_notebook("N".to_string());
    app.open_note_sort_picker();

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Sort notes"), "picker title: {text}");
    assert!(text.contains("modified"), "sort option missing: {text}");
}

#[test]
fn help_overlay_is_tab_aware_for_notes() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(dir.path().to_path_buf().into()).unwrap();
    app.switch_tab(tudo::app::Tab::Notes);
    app.mode = tudo::app::Mode::Help;

    let text = render_to_string(&mut app, 100, 30);
    assert!(text.contains("Keybindings · Notes"), "title: {text}");
    assert!(text.contains("edit body"), "notes help row missing: {text}");
    assert!(
        text.contains("$EDITOR"),
        "external-edit row missing: {text}"
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

/// Open the full-screen note-body editor on a note with `body`.
fn editing_note_body(dir: &std::path::Path, body: &str) -> App {
    let mut app = App::new(Some(dir.to_path_buf())).unwrap();
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Doc".to_string());
    app.set_current_note_body(body.to_string());
    app.note_focus_notes();
    app.note_activate();
    app
}

fn editor_cursor(app: &App) -> usize {
    match &app.mode {
        Mode::Input(i) => i.cursor,
        other => panic!("not editing, got {other:?}"),
    }
}

fn mouse_at(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn clicking_in_the_note_editor_places_the_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = editing_note_body(dir.path(), "alpha beta\nsecond line");

    // Render first so the editor's text area is recorded for hit-testing.
    let buf = render_to_buffer(&mut app, 60, 20);
    let (col, row) = find_needle(&buf, "beta");
    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Down(MouseButton::Left), col, row),
    );
    assert_eq!(editor_cursor(&app), 6, "cursor lands on the 'b' of beta");

    // A click past the end of a line clamps to the line's last position.
    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Down(MouseButton::Left), col + 40, row),
    );
    assert_eq!(editor_cursor(&app), 10);
}

#[test]
fn dragging_in_the_note_editor_selects_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = editing_note_body(dir.path(), "alpha beta\nsecond line");

    let buf = render_to_buffer(&mut app, 60, 20);
    let (col, row) = find_needle(&buf, "beta");
    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Down(MouseButton::Left), col, row),
    );
    // Drag across "beta" and on into the next line.
    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Drag(MouseButton::Left), col + 4, row),
    );
    let selected = match &app.mode {
        Mode::Input(i) => i.selected_text().unwrap(),
        _ => panic!("not editing"),
    };
    assert_eq!(selected, "beta");

    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Drag(MouseButton::Left), col, row + 1),
    );
    let selected = match &app.mode {
        Mode::Input(i) => i.selected_text().unwrap(),
        _ => panic!("not editing"),
    };
    assert_eq!(selected, "beta\nsecond");

    // The highlighted cells carry the selection background.
    let buf = render_to_buffer(&mut app, 60, 20);
    let (bcol, brow) = find_needle(&buf, "beta");
    assert_eq!(buf.cell((bcol, brow)).unwrap().bg, theme::sel());
    assert_ne!(buf.cell((bcol - 2, brow)).unwrap().bg, theme::sel());
}

#[test]
fn the_note_editor_cursor_follows_the_wrapped_text() {
    let dir = tempfile::tempdir().unwrap();
    // A logical line longer than the pane, so it word-wraps on screen.
    let body = "aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll mmm nnn ooo ppp";
    let mut app = editing_note_body(dir.path(), body);

    let buf = render_to_buffer(&mut app, 30, 12);
    // Words are kept whole across the wrap (a hard char-wrap would split them).
    let text = buffer_text(&buf);
    assert!(text.contains("kkk lll"), "words stay whole: {text}");

    // The screen position just past the last character is the buffer's end, so
    // the drawn cursor and a click there agree with the wrapped layout.
    let (col, row) = find_needle(&buf, "ppp");
    event::handle_mouse(
        &mut app,
        mouse_at(MouseEventKind::Down(MouseButton::Left), col + 3, row),
    );
    assert_eq!(editor_cursor(&app), body.chars().count());
}
