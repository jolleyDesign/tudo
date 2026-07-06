//! Translate crossterm key/mouse events into [`App`] actions.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::{
    App, ConfirmAction, CopyWhat, Focus, InputField, Mode, NoteCopyWhat, NotesFocus, Tab,
};
use crate::config;
use crate::keybind::Action;

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
        Mode::CopyMenu(_) => copy_menu_key(app, key),
        Mode::SortPicker(_) => sort_picker_key(app, key),
        Mode::NoteSortPicker(_) => note_sort_picker_key(app, key),
        Mode::NoteMovePicker(_) => note_move_picker_key(app, key),
        Mode::NoteCopyMenu(_) => note_copy_menu_key(app, key),
        Mode::NoteView(_) => note_view_key(app, key),
        Mode::NoteLinkPicker(_) => note_link_picker_key(app, key),
        Mode::Settings => settings_key(app, key),
        Mode::Help => app.close_overlay(),
        Mode::Normal | Mode::Detail => nav_key(app, key),
    }
}

fn settings_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('d') => app.start_change_data_dir(),
        KeyCode::Char('t') | KeyCode::Char('T') => app.open_theme_picker(),
        KeyCode::Char('c') | KeyCode::Char('C') => app.toggle_completed_to_bottom(),
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
    // Letters type into the filter, so navigation is arrows-only (plus Enter/Esc).
    match key.code {
        KeyCode::Down => app.move_picker_move(1),
        KeyCode::Up => app.move_picker_move(-1),
        KeyCode::Enter => app.move_picker_confirm(),
        KeyCode::Esc => app.move_picker_cancel(),
        KeyCode::Backspace => app.move_picker_backspace(),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.move_picker_type(c)
        }
        _ => {}
    }
}

fn note_move_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.note_move_picker_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.note_move_picker_move(-1),
        KeyCode::Enter => app.note_move_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.note_move_picker_cancel(),
        _ => {}
    }
}

fn note_view_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.note_view_scroll(1),
        KeyCode::Char('k') | KeyCode::Up => app.note_view_scroll(-1),
        KeyCode::PageDown | KeyCode::Char(' ') => app.note_view_scroll(10),
        KeyCode::PageUp => app.note_view_scroll(-10),
        // Tab / e / Enter drop into the raw editor.
        KeyCode::Tab | KeyCode::Char('e') | KeyCode::Enter => app.note_view_edit(),
        // f follows a link (returning to the read view on cancel).
        KeyCode::Char('f') => app.start_note_link_picker(true),
        KeyCode::Esc | KeyCode::Char('q') => app.close_overlay(),
        _ => {}
    }
}

fn note_link_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.note_link_picker_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.note_link_picker_move(-1),
        KeyCode::Enter => app.note_link_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.note_link_picker_cancel(),
        _ => {}
    }
}

fn note_copy_menu_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.note_copy_menu_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.note_copy_menu_move(-1),
        KeyCode::Char('1') => app.note_copy_selected(NoteCopyWhat::Body),
        KeyCode::Char('2') => app.note_copy_selected(NoteCopyWhat::Title),
        KeyCode::Char('3') => app.note_copy_selected(NoteCopyWhat::Markdown),
        KeyCode::Enter => app.note_copy_menu_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.note_copy_menu_cancel(),
        _ => {}
    }
}

fn copy_menu_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.copy_menu_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.copy_menu_move(-1),
        KeyCode::Char('1') => app.copy_selected(CopyWhat::Json),
        KeyCode::Char('2') => app.copy_selected(CopyWhat::Title),
        KeyCode::Char('3') => app.copy_selected(CopyWhat::Notes),
        KeyCode::Enter => app.copy_menu_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.copy_menu_cancel(),
        _ => {}
    }
}

fn sort_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.sort_picker_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.sort_picker_move(-1),
        KeyCode::Char(c @ '1'..='6') => app.sort_picker_select(c as usize - '1' as usize),
        KeyCode::Char('r') | KeyCode::Left | KeyCode::Right => app.toggle_sort_dir(),
        KeyCode::Enter => app.sort_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.sort_picker_cancel(),
        _ => {}
    }
}

fn note_sort_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.note_sort_picker_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.note_sort_picker_move(-1),
        KeyCode::Char(c @ '1'..='4') => app.note_sort_picker_select(c as usize - '1' as usize),
        KeyCode::Char('r') | KeyCode::Left | KeyCode::Right => app.toggle_note_sort_dir(),
        KeyCode::Enter => app.note_sort_picker_confirm(),
        KeyCode::Esc | KeyCode::Char('q') => app.note_sort_picker_cancel(),
        _ => {}
    }
}

fn nav_key(app: &mut App, key: KeyEvent) {
    if let Some(action) = app.keymap.action_for(key) {
        dispatch(app, action);
    }
}

/// Route a bound [`Action`] to the active tab's handler. Tab switching works
/// from either tab regardless of focus or mode.
fn dispatch(app: &mut App, action: Action) {
    match action {
        Action::SwitchToTasks => {
            app.switch_tab(Tab::Tasks);
            return;
        }
        Action::SwitchToNotes => {
            app.switch_tab(Tab::Notes);
            return;
        }
        _ => {}
    }
    match app.active_tab {
        Tab::Tasks => dispatch_tasks(app, action),
        Tab::Notes => dispatch_notes(app, action),
    }
}

/// Run the (mode-aware) handler for a bound [`Action`] on the Tasks tab.
fn dispatch_tasks(app: &mut App, action: Action) {
    let detail = matches!(app.mode, Mode::Detail);
    // Task-scoped actions must not reach into the tasks pane while the lists
    // pane is focused (outside a task's detail view). Otherwise e.g. Space would
    // tick off the highlighted task even though the cursor is on the lists pane.
    if !detail && app.focus == Focus::Lists && is_task_action(action) {
        return;
    }
    match action {
        Action::Quit => app.should_quit = true,
        Action::Help => app.mode = Mode::Help,
        Action::Back => {
            if detail {
                app.close_overlay();
            } else if app.filter.is_active() {
                app.clear_filter();
            }
        }
        Action::ToggleFocus => app.toggle_focus(),
        // Pane focus is meaningless inside a task's detail view.
        Action::ToggleSidebarPane if !detail => app.toggle_sidebar_pane(),
        Action::FocusLists if !detail => app.focus_lists(),
        Action::FocusTasks if !detail => app.focus_tasks(),
        Action::FocusTags if !detail => app.focus_tags(),
        Action::ToggleSidebarPane | Action::FocusLists | Action::FocusTasks | Action::FocusTags => {
        }
        Action::MoveDown => app.move_selection(1),
        Action::MoveUp => app.move_selection(-1),
        Action::ReorderDown => app.reorder_selection(1),
        Action::ReorderUp => app.reorder_selection(-1),
        Action::SendTop => app.send_task(true),
        Action::SendBottom => app.send_task(false),
        Action::ToggleDone => app.toggle_selected(),
        Action::Activate => app.activate(),
        Action::AddTask => app.start_add_task(),
        Action::AddList => app.start_add_list(),
        Action::Edit => app.start_edit(),
        Action::Archive => app.archive_selection(),
        Action::Delete => app.start_delete(),
        Action::CyclePriority => app.cycle_current_priority(),
        Action::SetPoints => app.start_set_points(),
        Action::SetDue => app.start_set_due(),
        Action::SetTags => app.start_set_tags(),
        Action::SetNotes => app.start_set_notes(),
        Action::AddSubtask => app.start_add_subtask(),
        Action::MoveTask => app.start_move_task(),
        Action::Copy => app.start_copy(),
        Action::Search => app.start_search(),
        Action::CycleFilter => app.cycle_status_filter(),
        Action::SortPicker => app.open_sort_picker(),
        Action::ThemePicker => app.open_theme_picker(),
        Action::Settings => app.open_settings(),
        // Tab switching is handled by the router; note-only actions do nothing
        // on the Tasks tab.
        Action::SwitchToTasks
        | Action::SwitchToNotes
        | Action::TogglePin
        | Action::ExternalEdit => {}
    }
}

/// Run the (mode-aware) handler for a bound [`Action`] on the Notes tab. The
/// same keys as the Tasks tab drive the parallel notebook/note operations.
fn dispatch_notes(app: &mut App, action: Action) {
    let detail = matches!(app.mode, Mode::Detail);
    // Note-scoped actions must not reach into the note list while the notebooks
    // pane is focused (outside the full-screen note view).
    if !detail && app.notes_focus == NotesFocus::Notebooks && is_note_action(action) {
        return;
    }
    match action {
        Action::Quit => app.should_quit = true,
        Action::Help => app.mode = Mode::Help,
        Action::ThemePicker => app.open_theme_picker(),
        Action::Settings => app.open_settings(),
        Action::Back => {
            if detail {
                app.close_overlay();
            } else if app.note_filter_active() {
                app.clear_note_filter();
            }
        }
        Action::ToggleFocus => app.note_toggle_focus(),
        Action::ToggleSidebarPane if !detail => app.note_toggle_sidebar_pane(),
        Action::FocusLists if !detail => app.note_focus_notebooks(),
        Action::FocusTasks if !detail => app.note_focus_notes(),
        Action::FocusTags if !detail => app.note_focus_tags(),
        Action::ToggleSidebarPane | Action::FocusLists | Action::FocusTasks | Action::FocusTags => {
        }
        Action::MoveDown => app.note_move_selection(1),
        Action::MoveUp => app.note_move_selection(-1),
        Action::ReorderDown => app.note_reorder_selection(1),
        Action::ReorderUp => app.note_reorder_selection(-1),
        Action::SendTop => app.send_note(true),
        Action::SendBottom => app.send_note(false),
        Action::Activate => app.note_activate(),
        Action::AddTask => app.start_add_note(),
        Action::AddList => app.start_add_notebook(),
        Action::Edit => app.start_edit_note(),
        Action::SetNotes => app.start_edit_note_body(),
        Action::SetTags => app.start_set_note_tags(),
        Action::TogglePin => app.toggle_current_note_pin(),
        Action::ExternalEdit => app.open_note_in_editor(),
        Action::Archive => app.note_archive_selection(),
        Action::MoveTask => app.start_move_note(),
        Action::Copy => app.start_note_copy(),
        Action::Delete => app.start_delete_note(),
        Action::Search => app.start_note_search(),
        Action::SortPicker => app.open_note_sort_picker(),
        // Task-only actions have no meaning on the Notes tab.
        Action::ToggleDone
        | Action::CyclePriority
        | Action::SetPoints
        | Action::SetDue
        | Action::AddSubtask
        | Action::CycleFilter => {}
        // Handled by the tab router before reaching here.
        Action::SwitchToTasks | Action::SwitchToNotes => {}
    }
}

/// Notes-tab actions that operate on the selected note and so are inert while
/// the notebooks pane is focused. Pane-agnostic ones (navigation, add, rename,
/// delete, search, sort) are deliberately absent.
fn is_note_action(action: Action) -> bool {
    matches!(
        action,
        Action::TogglePin
            | Action::ExternalEdit
            | Action::SetNotes
            | Action::SetTags
            | Action::MoveTask
            | Action::Copy
            | Action::SendTop
            | Action::SendBottom
    )
}

/// Actions that operate on the selected task and so are inert while the lists
/// pane is focused. Dual-purpose actions (Edit/Delete/Archive rename or remove
/// the list instead) and pane-agnostic ones (navigation, AddTask, AddList,
/// Search, filters, overlays) are deliberately absent.
fn is_task_action(action: Action) -> bool {
    matches!(
        action,
        Action::ToggleDone
            | Action::CyclePriority
            | Action::SetPoints
            | Action::SetDue
            | Action::SetTags
            | Action::SetNotes
            | Action::AddSubtask
            | Action::MoveTask
            | Action::Copy
            | Action::SendTop
            | Action::SendBottom
    )
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
    // Tab in the note editor saves and flips to the rendered read view.
    let mut to_view = false;

    if let Mode::Input(input) = &mut app.mode {
        match key.code {
            // The full-screen note editor autosaves on Esc; every other field
            // treats Esc as cancel/discard.
            KeyCode::Esc if input.field == InputField::NoteBody => step = Step::Commit,
            KeyCode::Esc => step = Step::Cancel,
            KeyCode::Tab if input.field == InputField::NoteBody => {
                step = Step::Commit;
                to_view = true;
            }
            KeyCode::Enter if input.multiline => input.insert('\n'),
            KeyCode::Enter => step = Step::Commit,
            KeyCode::Char('s') if input.multiline && ctrl => step = Step::Commit,
            KeyCode::Char(c) => input.insert(c),
            KeyCode::Backspace => input.backspace(),
            KeyCode::Delete => input.delete(),
            KeyCode::Left => input.left(),
            KeyCode::Right => input.right(),
            KeyCode::Up if input.multiline => input.up(),
            KeyCode::Down if input.multiline => input.down(),
            KeyCode::PageUp if input.multiline => {
                for _ in 0..10 {
                    input.up();
                }
            }
            KeyCode::PageDown if input.multiline => {
                for _ in 0..10 {
                    input.down();
                }
            }
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
            if to_view {
                app.open_note_view();
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
        InputField::RenameList => app.rename_current_list(buffer),
        InputField::NewTask => app.add_task(buffer),
        InputField::EditTask => app.edit_current_task_title(buffer),
        InputField::Tags => app.set_current_tags(&buffer),
        InputField::Points => app.set_current_points(&buffer),
        InputField::Due => app.set_current_due(&buffer),
        InputField::Notes => app.set_current_notes(buffer),
        InputField::NewSubtask => app.add_subtask(buffer),
        InputField::EditSubtask => app.edit_current_subtask(buffer),
        InputField::Search => app.set_search(buffer),
        InputField::DataDir => app.relocate_data(&buffer),
        InputField::NewNotebook => app.add_notebook(buffer),
        InputField::RenameNotebook => app.rename_current_notebook(buffer),
        InputField::NewNote => app.add_note(buffer),
        InputField::EditNote => app.edit_current_note_title(buffer),
        InputField::NoteBody => app.set_current_note_body(buffer),
        InputField::NoteTags => app.set_current_note_tags(&buffer),
        InputField::NoteSearch => app.set_note_search(buffer),
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
        ConfirmAction::DeleteNote => {
            app.delete_current_note();
            app.mode = Mode::Normal;
        }
        ConfirmAction::DeleteNotebook => app.delete_current_notebook(),
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
    // The scroll wheel navigates the full-screen note editor by moving the
    // cursor a few lines (the view follows it), like scrolling moves the
    // selection elsewhere.
    if matches!(&app.mode, Mode::Input(input) if input.field == InputField::NoteBody) {
        if let Mode::Input(input) = &mut app.mode {
            match m.kind {
                MouseEventKind::ScrollDown => {
                    for _ in 0..3 {
                        input.down();
                    }
                }
                MouseEventKind::ScrollUp => {
                    for _ in 0..3 {
                        input.up();
                    }
                }
                _ => {}
            }
        }
        return;
    }
    // The rendered read view scrolls with the wheel and follows clicked links.
    if matches!(app.mode, Mode::NoteView(_)) {
        match m.kind {
            MouseEventKind::ScrollDown => app.note_view_scroll(3),
            MouseEventKind::ScrollUp => app.note_view_scroll(-3),
            MouseEventKind::Down(MouseButton::Left) => {
                let target = app
                    .clickables
                    .note_view_links
                    .iter()
                    .find(|(r, _)| hit(*r, m.column, m.row))
                    .map(|(_, t)| *t);
                if let Some(r) = target {
                    app.jump_to_note(r);
                }
            }
            _ => {}
        }
        return;
    }
    if !matches!(app.mode, Mode::Normal | Mode::Detail) {
        return;
    }
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let (col, row) = (m.column, m.row);
            // A click on a header tab chip switches tabs, from either tab.
            let clicked_tab = app
                .clickables
                .tab_rects
                .iter()
                .find(|(_, r)| hit(*r, col, row))
                .map(|(t, _)| *t);
            if let Some(tab) = clicked_tab {
                app.switch_tab(tab);
                return;
            }
            match app.active_tab {
                Tab::Tasks => task_click(app, col, row),
                Tab::Notes => note_click(app, col, row),
            }
        }
        MouseEventKind::ScrollDown => match app.active_tab {
            Tab::Tasks => app.scroll_selection(1),
            Tab::Notes => app.note_scroll_selection(1),
        },
        MouseEventKind::ScrollUp => match app.active_tab {
            Tab::Tasks => app.scroll_selection(-1),
            Tab::Notes => app.note_scroll_selection(-1),
        },
        _ => {}
    }
}

fn task_click(app: &mut App, col: u16, row: u16) {
    if let Some((rect, offset)) = app.clickables.lists_inner
        && hit(rect, col, row)
    {
        // Lists occupy the first rows of the flat sidebar order, so the
        // clicked row *is* the flat index.
        let idx = offset + (row - rect.y) as usize;
        app.select_sidebar_row(idx);
        return;
    }
    if let Some((rect, offset)) = app.clickables.tags_inner
        && hit(rect, col, row)
    {
        // Tag rows follow the lists in the flat order.
        let local = offset + (row - rect.y) as usize;
        app.select_sidebar_row(app.lists.len() + local);
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

fn note_click(app: &mut App, col: u16, row: u16) {
    if let Some((rect, offset)) = app.clickables.notebooks_inner
        && hit(rect, col, row)
    {
        let idx = offset + (row - rect.y) as usize;
        app.select_note_sidebar_row(idx);
        return;
    }
    if let Some((rect, offset)) = app.clickables.note_tags_inner
        && hit(rect, col, row)
    {
        let local = offset + (row - rect.y) as usize;
        app.select_note_sidebar_row(app.notebooks.len() + local);
        return;
    }
    if let Some((rect, offset)) = app.clickables.note_list_inner
        && hit(rect, col, row)
    {
        let vidx = offset + (row - rect.y) as usize;
        app.select_note_visible(vidx);
    }
}

fn hit(rect: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}
