//! Translate crossterm key/mouse events into [`App`] actions.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::{App, ConfirmAction, InputField, Mode};
use crate::config;

/// Route a key event to the handler for the current mode.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ignore key-release / repeat noise (matters on some platforms).
    if key.kind == KeyEventKind::Release {
        return;
    }
    // Ctrl-C always quits.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match app.mode {
        Mode::FirstRun(_) => first_run_key(app, key),
        Mode::Input(_) => input_key(app, key),
        Mode::Confirm(_) => confirm_key(app, key),
        Mode::ThemePicker(_) => theme_picker_key(app, key),
        Mode::MovePicker(_) => move_picker_key(app, key),
        Mode::Settings => settings_key(app, key),
        Mode::Help => app.close_overlay(),
        Mode::Normal | Mode::Detail => nav_key(app, key),
    }
}

fn settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('d') => app.start_change_data_dir(),
        KeyCode::Char('t') | KeyCode::Char('T') => app.open_theme_picker(),
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('S') => app.close_overlay(),
        _ => {}
    }
}

fn theme_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.theme_picker_preview(1),
        KeyCode::Char('k') | KeyCode::Up => app.theme_picker_preview(-1),
        KeyCode::Enter => app.theme_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.theme_picker_cancel(),
        _ => {}
    }
}

fn move_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_picker_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_picker_move(-1),
        KeyCode::Enter => app.move_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.move_picker_cancel(),
        _ => {}
    }
}

fn nav_key(app: &mut App, key: KeyEvent) {
    let detail = matches!(app.mode, Mode::Detail);
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Esc => {
            if detail {
                app.close_overlay();
            } else if app.filter.is_active() {
                app.clear_filter();
            }
        }
        KeyCode::Tab => app.toggle_focus(),
        KeyCode::Char('h') | KeyCode::Left if !detail => app.focus_lists(),
        KeyCode::Char('l') | KeyCode::Right if !detail => app.focus_tasks(),
        KeyCode::Char('j') | KeyCode::Down => app.move_selection(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_selection(-1),
        KeyCode::Char(' ') => app.toggle_selected(),
        KeyCode::Enter => app.activate(),
        KeyCode::Char('a') => app.start_add_task(),
        KeyCode::Char('A') => app.start_add_list(),
        KeyCode::Char('e') => app.start_edit(),
        KeyCode::Char('d') => app.start_delete(),
        KeyCode::Char('p') => app.cycle_current_priority(),
        KeyCode::Char('D') => app.start_set_due(),
        KeyCode::Char('t') => app.start_set_tags(),
        KeyCode::Char('n') => app.start_set_notes(),
        KeyCode::Char('s') => app.start_add_subtask(),
        KeyCode::Char('m') => app.start_move_task(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('f') => app.cycle_status_filter(),
        KeyCode::Char('T') => app.open_theme_picker(),
        KeyCode::Char('S') => app.open_settings(),
        _ => {}
    }
}

fn input_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // Resolve commit/cancel first (needs to read fields, then mutate app).
    enum Step {
        None,
        Cancel,
        Commit,
    }
    let mut step = Step::None;

    if let Mode::Input(input) = &mut app.mode {
        match key.code {
            KeyCode::Esc => step = Step::Cancel,
            KeyCode::Enter if input.multiline => input.insert('\n'),
            KeyCode::Enter => step = Step::Commit,
            KeyCode::Char('s') if input.multiline && ctrl => step = Step::Commit,
            KeyCode::Char(c) => input.insert(c),
            KeyCode::Backspace => input.backspace(),
            KeyCode::Left => input.left(),
            KeyCode::Right => input.right(),
            KeyCode::Home => input.home(),
            KeyCode::End => input.end(),
            _ => {}
        }
    }

    match step {
        Step::None => {}
        Step::Cancel => {
            if let Mode::Input(input) = &app.mode {
                app.mode = return_mode(input.field, input.return_detail);
            }
        }
        Step::Commit => {
            if let Mode::Input(input) = &app.mode {
                let field = input.field;
                let buffer = input.buffer.clone();
                app.mode = return_mode(field, input.return_detail);
                apply_input(app, field, buffer);
            }
        }
    }
}

/// Where an input field returns to when it closes.
fn return_mode(field: InputField, detail: bool) -> Mode {
    match field {
        // Cancelling a data-dir change goes back to settings; a successful move
        // reloads into Normal (see relocate_data).
        InputField::DataDir => Mode::Settings,
        _ if detail => Mode::Detail,
        _ => Mode::Normal,
    }
}

fn apply_input(app: &mut App, field: InputField, buffer: String) {
    match field {
        InputField::NewList => app.add_list(buffer),
        InputField::NewTask => app.add_task(buffer),
        InputField::EditTask => app.edit_current_task_title(buffer),
        InputField::Tags => app.set_current_tags(&buffer),
        InputField::Due => app.set_current_due(&buffer),
        InputField::Notes => app.set_current_notes(buffer),
        InputField::NewSubtask => app.add_subtask(buffer),
        InputField::EditSubtask => app.edit_current_subtask(buffer),
        InputField::Search => app.set_search(buffer),
        InputField::DataDir => app.relocate_data(&buffer),
    }
}

fn confirm_key(app: &mut App, key: KeyEvent) {
    let (accept, action, return_detail) = match &app.mode {
        Mode::Confirm(c) => (
            matches!(
                key.code,
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
            ),
            c.action,
            c.return_detail,
        ),
        _ => return,
    };

    // Any non-accept key cancels.
    app.mode = if return_detail {
        Mode::Detail
    } else {
        Mode::Normal
    };

    if !accept {
        return;
    }
    match action {
        ConfirmAction::DeleteTask => {
            app.delete_current_task();
            // A deleted task can't have a detail view.
            app.mode = Mode::Normal;
        }
        ConfirmAction::DeleteList => app.delete_current_list(),
        ConfirmAction::DeleteSubtask => app.delete_current_subtask(),
    }
}

fn first_run_key(app: &mut App, key: KeyEvent) {
    // Editing the custom path.
    let editing = matches!(&app.mode, Mode::FirstRun(fr) if fr.editing_custom);
    if editing {
        let mut commit: Option<String> = None;
        if let Mode::FirstRun(fr) = &mut app.mode {
            match key.code {
                KeyCode::Esc => fr.editing_custom = false,
                KeyCode::Enter => commit = Some(fr.custom.clone()),
                KeyCode::Char(c) => fr.custom.push(c),
                KeyCode::Backspace => {
                    fr.custom.pop();
                }
                _ => {}
            }
        }
        if let Some(path) = commit
            && !path.trim().is_empty()
        {
            app.commit_first_run(config::expand_tilde(&path));
        }
        return;
    }

    // Selecting from the menu.
    let mut chosen: Option<std::path::PathBuf> = None;
    let mut start_custom = false;
    if let Mode::FirstRun(fr) = &mut app.mode {
        let n = fr.entry_count();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Char('k') | KeyCode::Up => fr.selected = (fr.selected + n - 1) % n,
            KeyCode::Char('j') | KeyCode::Down => fr.selected = (fr.selected + 1) % n,
            KeyCode::Enter => {
                if fr.selected < fr.options.len() {
                    chosen = Some(fr.options[fr.selected].1.clone());
                } else {
                    start_custom = true;
                }
            }
            _ => {}
        }
    }
    if let Some(path) = chosen {
        app.commit_first_run(path);
    } else if start_custom && let Mode::FirstRun(fr) = &mut app.mode {
        fr.editing_custom = true;
    }
}

/// Handle a mouse event using the clickable rects recorded during render.
pub fn handle_mouse(app: &mut App, m: MouseEvent) {
    if !matches!(app.mode, Mode::Normal | Mode::Detail) {
        return;
    }
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let (col, row) = (m.column, m.row);
            if let Some((rect, offset)) = app.clickables.lists_inner
                && hit(rect, col, row)
            {
                let idx = offset + (row - rect.y) as usize;
                app.select_list_index(idx);
                return;
            }
            if let Some((rect, offset)) = app.clickables.tasks_inner
                && hit(rect, col, row)
            {
                let vidx = offset + (row - rect.y) as usize;
                app.select_task_visible(vidx);
                // Clicking the leading checkbox toggles done.
                if col <= rect.x + 2 {
                    app.toggle_current_done();
                }
            }
        }
        MouseEventKind::ScrollDown => app.move_selection(1),
        MouseEventKind::ScrollUp => app.move_selection(-1),
        _ => {}
    }
}

fn hit(rect: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}
