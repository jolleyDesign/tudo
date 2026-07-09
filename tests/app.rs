//! Action-logic tests driving `App` directly (no terminal involved).

use std::path::PathBuf;

use crossterm::ExecutableCommand;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tudo::app::{
    App, CopyWhat, Focus, InputField, Mode, NoteCopyWhat, NoteSidebarRow, NoteView, NotesFocus,
    SidebarRow, StatusFilter, Tab, View,
};
use tudo::keybind::{Keymap, Overrides};
use tudo::model::{NoteSort, Priority, Sort, SortDir};
use tudo::theme::ThemeKind;
use tudo::{config, event, storage};

fn app_in(dir: &std::path::Path) -> App {
    App::new(Some(dir.to_path_buf())).unwrap()
}

/// Type a string one character at a time through the real key handler.
fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        event::handle_key(app, KeyEvent::from(KeyCode::Char(c)));
    }
}

#[test]
fn add_list_then_task_persists_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    app.add_list("Work".to_string());
    assert_eq!(app.lists.len(), 1);
    assert_eq!(app.current_list().unwrap().name, "Work");

    app.add_task("First task".to_string());
    assert_eq!(app.current_task().unwrap().title, "First task");

    // Reload from disk via a fresh App to confirm persistence.
    let reloaded = app_in(dir.path());
    assert_eq!(reloaded.lists.len(), 1);
    assert_eq!(reloaded.lists[0].tasks.len(), 1);
    assert_eq!(reloaded.lists[0].tasks[0].title, "First task");
}

#[test]
fn quick_add_sets_fields_from_the_title_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());

    app.add_task("Ship it !! #work n:mind the changelog s:tag, publish".to_string());

    let task = app.current_task().unwrap();
    assert_eq!(task.title, "Ship it");
    assert_eq!(task.priority, Some(Priority::Medium));
    assert_eq!(task.tags, vec!["work"]);
    assert_eq!(task.notes, "mind the changelog");
    let subs: Vec<&str> = task.subtasks.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(subs, vec!["tag", "publish"]);

    // Fields survive a round-trip through disk.
    let reloaded = app_in(dir.path());
    let task = &reloaded.lists[0].tasks[0];
    assert_eq!(task.title, "Ship it");
    assert_eq!(task.priority, Some(Priority::Medium));
    assert_eq!(task.subtasks.len(), 2);
}

#[test]
fn rename_list_persists_and_keeps_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    app.add_list("Work".to_string());
    app.add_task("keep me".to_string());
    let slug = app.current_list().unwrap().slug.clone();

    app.rename_current_list("Career".to_string());
    assert_eq!(app.current_list().unwrap().name, "Career");
    // The on-disk file (slug) is unchanged; tasks are preserved.
    assert_eq!(app.current_list().unwrap().slug, slug);
    assert!(dir.path().join(format!("{slug}.json")).exists());

    // Reload from disk: the new name persisted, the task survived.
    let reloaded = app_in(dir.path());
    assert_eq!(reloaded.lists.len(), 1);
    assert_eq!(reloaded.lists[0].name, "Career");
    assert_eq!(reloaded.lists[0].tasks[0].title, "keep me");
}

#[test]
fn rename_list_resorts_and_follows_selection() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // Sorted by name: "Inbox" (0) then "Work" (1).
    app.add_list("Inbox".to_string());
    app.add_list("Work".to_string());
    app.select_list_index(0);
    assert_eq!(app.current_list().unwrap().name, "Inbox");

    // Rename "Inbox" -> "Zebra": it now sorts last, and stays selected.
    app.rename_current_list("Zebra".to_string());
    assert_eq!(app.current_list().unwrap().name, "Zebra");
    assert_eq!(app.selected_list(), 1);
}

#[test]
fn rename_list_ignores_blank_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());

    app.rename_current_list("   ".to_string());
    assert_eq!(app.current_list().unwrap().name, "Work");
}

#[test]
fn toggle_done_sets_and_clears_completion() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t".to_string());

    app.toggle_current_done();
    let t = app.current_task().unwrap();
    assert!(t.done);
    assert!(t.completed_at.is_some());

    app.toggle_current_done();
    let t = app.current_task().unwrap();
    assert!(!t.done);
    assert!(t.completed_at.is_none());
}

#[test]
fn completed_to_bottom_sinks_done_tasks_without_touching_stored_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());
    app.add_task("d".to_string());

    // Titles in current *display* order.
    let visible_titles = |app: &App| -> Vec<String> {
        let list = app.current_list().unwrap();
        app.visible_task_indices()
            .iter()
            .map(|&i| list.tasks[i].title.clone())
            .collect()
    };
    // Titles in *stored* order (what lands on disk).
    let stored_titles = |app: &App| -> Vec<String> {
        app.current_list()
            .unwrap()
            .tasks
            .iter()
            .map(|t| t.title.clone())
            .collect()
    };

    // Mark "b" and "d" done.
    app.select_task_visible(1);
    app.toggle_current_done();
    app.select_task_visible(3);
    app.toggle_current_done();

    // Off (default): display order matches stored order.
    assert_eq!(visible_titles(&app), ["a", "b", "c", "d"]);

    // On: done tasks sink, each group keeping its relative order.
    app.set_completed_to_bottom(true);
    assert_eq!(visible_titles(&app), ["a", "c", "b", "d"]);
    // Stored order is untouched — the change is display-only.
    assert_eq!(stored_titles(&app), ["a", "b", "c", "d"]);

    // It composes with an active sort (alphabetical here still sinks the done).
    app.set_sort(Sort::Alphabetical);
    assert_eq!(visible_titles(&app), ["a", "c", "b", "d"]);
    app.set_sort(Sort::Manual);

    // Turning it back off restores the original order (fully reversible).
    app.set_completed_to_bottom(false);
    assert_eq!(visible_titles(&app), ["a", "b", "c", "d"]);
}

#[test]
fn cycle_priority_walks_through_all_levels() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t".to_string());

    assert_eq!(app.current_task().unwrap().priority, None);
    app.cycle_current_priority();
    assert_eq!(app.current_task().unwrap().priority, Some(Priority::Low));
    app.cycle_current_priority();
    assert_eq!(app.current_task().unwrap().priority, Some(Priority::Medium));
    app.cycle_current_priority();
    assert_eq!(app.current_task().unwrap().priority, Some(Priority::High));
    app.cycle_current_priority();
    assert_eq!(app.current_task().unwrap().priority, None);
}

#[test]
fn set_due_valid_and_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t".to_string());

    app.set_current_due("2026-12-31");
    assert_eq!(
        app.current_task().unwrap().due,
        chrono::NaiveDate::from_ymd_opt(2026, 12, 31)
    );

    app.set_current_due("garbage");
    // unchanged, and a helpful status message is set
    assert_eq!(
        app.current_task().unwrap().due,
        chrono::NaiveDate::from_ymd_opt(2026, 12, 31)
    );
    assert!(!app.status.is_empty());

    app.set_current_due(""); // clears
    assert_eq!(app.current_task().unwrap().due, None);
}

#[test]
fn set_tags_parses_input() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t".to_string());

    app.set_current_tags("#a, B  a");
    assert_eq!(
        app.current_task().unwrap().tags,
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn set_points_valid_invalid_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t".to_string());

    app.set_current_points("5");
    assert_eq!(app.current_task().unwrap().points, Some(5));

    // A non-numeric entry is rejected and leaves the value untouched.
    app.set_current_points("lots");
    assert_eq!(app.current_task().unwrap().points, Some(5));
    assert!(!app.status.is_empty());

    app.set_current_points(""); // clears
    assert_eq!(app.current_task().unwrap().points, None);
}

#[test]
fn delete_task_clamps_selection() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());

    app.delete_current_task();
    assert_eq!(app.current_list().unwrap().tasks.len(), 1);
    assert!(app.current_task().is_some());
}

#[test]
fn move_task_to_another_list_persists_and_clamps() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // add_list sorts by name; "Inbox" < "Work", so Inbox is index 0.
    app.add_list("Inbox".to_string());
    app.add_list("Work".to_string());

    // Add two tasks to Inbox.
    app.select_list_index(0);
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    assert_eq!(app.current_list().unwrap().name, "Inbox");

    // Move the selected task ("b", the last-added/selected) into Work.
    app.select_task_visible(1);
    app.start_move_task();
    app.move_picker_confirm(); // selected target defaults to the first other list (Work)

    // Inbox keeps "a" with a valid clamped selection; Work received "b".
    assert_eq!(app.current_list().unwrap().name, "Inbox");
    assert_eq!(app.current_list().unwrap().tasks.len(), 1);
    assert_eq!(app.current_task().unwrap().title, "a");

    app.select_list_index(1);
    assert_eq!(app.current_list().unwrap().name, "Work");
    assert_eq!(app.current_list().unwrap().tasks.len(), 1);
    assert_eq!(app.current_task().unwrap().title, "b");

    // Both lists were persisted.
    let reloaded = app_in(dir.path());
    let inbox = reloaded.lists.iter().find(|l| l.name == "Inbox").unwrap();
    let work = reloaded.lists.iter().find(|l| l.name == "Work").unwrap();
    assert_eq!(inbox.tasks.len(), 1);
    assert_eq!(work.tasks.len(), 1);
    assert_eq!(work.tasks[0].title, "b");
}

#[test]
fn reorder_and_send_task_within_list_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());
    let titles = |app: &App| -> Vec<String> {
        app.current_list()
            .unwrap()
            .tasks
            .iter()
            .map(|t| t.title.clone())
            .collect()
    };

    // Select "b" (middle) and nudge it up: [a, b, c] -> [b, a, c].
    app.select_task_visible(1);
    app.reorder_task(-1);
    assert_eq!(titles(&app), ["b", "a", "c"]);
    // Selection follows the moved task.
    assert_eq!(app.current_task().unwrap().title, "b");

    // Nudging up again at the top edge is a no-op.
    app.reorder_task(-1);
    assert_eq!(titles(&app), ["b", "a", "c"]);

    // Send it to the bottom: [b, a, c] -> [a, c, b], still selected.
    app.send_task(false);
    assert_eq!(titles(&app), ["a", "c", "b"]);
    assert_eq!(app.current_task().unwrap().title, "b");

    // Send it back to the top: [a, c, b] -> [b, a, c].
    app.send_task(true);
    assert_eq!(titles(&app), ["b", "a", "c"]);
    assert_eq!(app.current_task().unwrap().title, "b");

    // The new order round-trips through disk.
    let reloaded = app_in(dir.path());
    let order: Vec<String> = reloaded.lists[0]
        .tasks
        .iter()
        .map(|t| t.title.clone())
        .collect();
    assert_eq!(order, ["b", "a", "c"]);
}

#[test]
fn send_task_keeps_selection_and_stays_in_group_under_sink() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());
    app.add_task("d".to_string());

    // Display order in the tasks pane.
    let visible_titles = |app: &App| -> Vec<String> {
        let list = app.current_list().unwrap();
        app.visible_task_indices()
            .iter()
            .map(|&i| list.tasks[i].title.clone())
            .collect()
    };

    // Mark "b" and "d" done, then sink completed tasks: display is [a, c, b, d]
    // (incomplete first, done floated to the bottom).
    app.select_task_visible(1);
    app.toggle_current_done();
    app.select_task_visible(3);
    app.toggle_current_done();
    app.set_completed_to_bottom(true);
    assert_eq!(visible_titles(&app), ["a", "c", "b", "d"]);

    // Send incomplete "a" (top of its group) to the bottom. It can only reach
    // the bottom of the incomplete group — the done tasks re-sink below it — so
    // it lands last among incompletes, and the cursor follows it there.
    app.select_task_visible(0);
    app.send_task(false);
    assert_eq!(visible_titles(&app), ["c", "a", "b", "d"]);
    assert_eq!(app.current_task().unwrap().title, "a");

    // Send done "b" (top of the done group) to the bottom of that group: after
    // "d". Cursor stays on the moved task.
    app.select_task_visible(2);
    app.send_task(false);
    assert_eq!(visible_titles(&app), ["c", "a", "d", "b"]);
    assert_eq!(app.current_task().unwrap().title, "b");

    // Hoisting done "b" to the top of its group puts it first among done tasks
    // (still below every incomplete task), cursor following.
    app.send_task(true);
    assert_eq!(visible_titles(&app), ["c", "a", "b", "d"]);
    assert_eq!(app.current_task().unwrap().title, "b");

    // Turning sink off reveals the underlying stored order is intact and valid.
    app.set_completed_to_bottom(false);
    assert_eq!(visible_titles(&app), ["c", "a", "b", "d"]);
}

#[test]
fn reorder_task_steps_past_hidden_tasks_under_filter() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());

    // Mark "b" done, then filter to active: visible = [a, c] (raw 0 and 2).
    app.select_task_visible(1);
    app.toggle_current_done();
    app.cycle_status_filter(); // -> active
    assert_eq!(app.visible_task_indices(), vec![0, 2]);

    // Move "c" (visible index 1) up past the hidden "b": raw order becomes
    // [c, b, a], and "c" is now the first visible task.
    app.select_task_visible(1);
    assert_eq!(app.current_task().unwrap().title, "c");
    app.reorder_task(-1);
    let raw: Vec<String> = app
        .current_list()
        .unwrap()
        .tasks
        .iter()
        .map(|t| t.title.clone())
        .collect();
    assert_eq!(raw, ["c", "b", "a"]);
    assert_eq!(app.selected_visible_task(), 0);
    assert_eq!(app.current_task().unwrap().title, "c");
}

#[test]
fn reorder_list_persists_and_clamps_at_edges() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    let names = |app: &App| -> Vec<String> { app.lists.iter().map(|l| l.name.clone()).collect() };

    // Created out of order; the sidebar starts alphabetical.
    app.add_list("Banana".to_string());
    app.add_list("Apple".to_string());
    app.add_list("Cherry".to_string());
    assert_eq!(names(&app), ["Apple", "Banana", "Cherry"]);

    // Select "Apple" in the sidebar and nudge it down past "Banana".
    app.select_list_index(0);
    assert_eq!(app.current_list().unwrap().name, "Apple");
    app.reorder_list(1);
    assert_eq!(names(&app), ["Banana", "Apple", "Cherry"]);
    // Selection follows the moved list.
    assert_eq!(app.selected_list(), 1);
    assert_eq!(app.current_list().unwrap().name, "Apple");

    // Nudge it down once more, then hit the bottom edge (a no-op).
    app.reorder_list(1);
    assert_eq!(names(&app), ["Banana", "Cherry", "Apple"]);
    app.reorder_list(1);
    assert_eq!(names(&app), ["Banana", "Cherry", "Apple"]);

    // The custom order round-trips through disk (i.e. it beats the load-time
    // alphabetical fallback).
    let reloaded = app_in(dir.path());
    assert_eq!(names(&reloaded), ["Banana", "Cherry", "Apple"]);
}

#[test]
fn shift_j_reorders_the_list_when_the_sidebar_is_focused() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Apple".to_string());
    app.add_list("Banana".to_string());
    app.select_list_index(0); // focuses the sidebar on "Apple"

    // `J` (reorder-down) drives the whole key path, not just the App method.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('J')));
    let names: Vec<String> = app.lists.iter().map(|l| l.name.clone()).collect();
    assert_eq!(names, ["Banana", "Apple"]);
    assert_eq!(app.current_list().unwrap().name, "Apple");
}

#[test]
fn reorder_list_leaves_archive_pinned_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_list("Zebra".to_string());

    // Create the Archived list (pinned last) by archiving a task.
    app.select_list_index(0); // Work
    app.focus_tasks();
    app.add_task("t".to_string());
    app.archive_selection();
    assert!(app.lists.last().unwrap().is_archive());
    let before: Vec<String> = app.lists.iter().map(|l| l.name.clone()).collect();

    // The last user list can't be pushed below the archive.
    let zebra = app.lists.iter().position(|l| l.name == "Zebra").unwrap();
    app.select_list_index(zebra);
    app.reorder_list(1);
    assert_eq!(
        app.lists.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
        before
    );

    // The archive itself can't be dragged up out of the bottom slot.
    let arch = app.lists.iter().position(|l| l.is_archive()).unwrap();
    app.select_list_index(arch);
    app.reorder_list(-1);
    assert!(app.lists.last().unwrap().is_archive());
}

#[test]
fn archive_is_created_lazily_and_pinned_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // No archive until something is archived.
    app.add_list("Work".to_string());
    app.add_list("Zebra".to_string());
    assert!(!app.lists.iter().any(|l| l.is_archive()));

    // Archiving a task creates the Archived list, pinned after user lists
    // despite its name sorting early.
    app.add_task("t".to_string());
    app.archive_selection(); // `d` on a task archives it
    let names: Vec<String> = app.lists.iter().map(|l| l.name.clone()).collect();
    assert_eq!(names.last().unwrap(), "Archived");
    assert_eq!(app.lists.iter().filter(|l| l.is_archive()).count(), 1);
}

#[test]
fn archive_moves_task_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("do it".to_string());
    app.add_task("later".to_string());
    app.select_task_visible(0); // "do it"

    app.archive_selection();

    let work = app.lists.iter().find(|l| l.name == "Work").unwrap();
    assert_eq!(work.tasks.len(), 1);
    assert_eq!(work.tasks[0].title, "later");
    let arch = app.lists.iter().find(|l| l.is_archive()).unwrap();
    assert_eq!(arch.tasks.len(), 1);
    assert_eq!(arch.tasks[0].title, "do it");

    // Both lists round-trip through disk (the archive file too).
    let reloaded = app_in(dir.path());
    let work = reloaded.lists.iter().find(|l| l.name == "Work").unwrap();
    let arch = reloaded.lists.iter().find(|l| l.is_archive()).unwrap();
    assert_eq!(work.tasks.len(), 1);
    assert_eq!(arch.tasks[0].title, "do it");
    // Still pinned last after a reload.
    assert!(reloaded.lists.last().unwrap().is_archive());
}

#[test]
fn archiving_an_already_archived_task_is_a_noop_hint() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t".to_string());
    app.archive_selection();

    let ai = app.lists.iter().position(|l| l.is_archive()).unwrap();
    app.select_list_index(ai);
    app.focus_tasks();
    app.select_task_visible(0);
    app.archive_selection(); // no-op on an already-archived task

    assert!(!app.status.is_empty());
    let arch = app.lists.iter().find(|l| l.is_archive()).unwrap();
    assert_eq!(arch.tasks.len(), 1);
}

#[test]
fn d_on_a_focused_list_neither_deletes_it_nor_archives_a_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("keep me".to_string());

    // Focus the Lists pane (a task stays selected underneath) and press `d`.
    app.select_list_index(0);
    app.archive_selection();

    // The list isn't deleted and no confirm prompt opens...
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.lists.len(), 1);
    assert_eq!(app.current_list().unwrap().name, "Work");
    // ...and the underlying task isn't archived (no Archived list appears).
    assert!(!app.lists.iter().any(|l| l.is_archive()));
    assert_eq!(app.current_list().unwrap().tasks.len(), 1);
    // A hint points the user at the explicit delete key instead.
    assert!(!app.status.is_empty());
}

#[test]
fn task_keys_are_inert_while_the_lists_pane_is_focused() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("keep me".to_string());

    // Focus the Lists pane; a task stays selected in the tasks pane underneath.
    app.select_list_index(0);

    // Space (toggle-done) and `p` (cycle-priority) must not reach that task.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char(' ')));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('p')));
    let task = app.current_task().unwrap();
    assert!(
        !task.done,
        "space toggled the task despite the lists pane focus"
    );
    assert!(
        task.priority.is_none(),
        "`p` changed the task priority despite the lists pane focus"
    );
    // No task-editing overlay opened either (e.g. due/tags/notes).
    assert!(matches!(app.mode, Mode::Normal));

    // Focusing the tasks pane restores the behaviour.
    app.focus_tasks();
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char(' ')));
    assert!(app.current_task().unwrap().done);
}

#[test]
fn d_on_a_selected_subtask_does_not_delete_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("parent".to_string());
    app.add_subtask("child".to_string());

    // Open the detail view with the subtask selected, then press `d`.
    app.open_detail();
    app.archive_selection();

    // The subtask survives (X is the only way to delete it) and a hint is set.
    assert!(matches!(app.mode, Mode::Detail));
    assert_eq!(app.current_task().unwrap().subtasks.len(), 1);
    assert!(!app.status.is_empty());
}

#[test]
fn unarchive_via_move_and_archive_hidden_from_targets() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t".to_string());
    app.archive_selection();

    // From a user list, the Archived list is not offered as a move target.
    let wi = app.lists.iter().position(|l| l.name == "Work").unwrap();
    app.select_list_index(wi);
    app.focus_tasks();
    app.add_task("keep".to_string());
    app.start_move_task();
    assert!(!matches!(app.mode, Mode::MovePicker(_)));
    app.close_overlay();

    // From the Archived list, moving out to Work unarchives the task.
    let ai = app.lists.iter().position(|l| l.is_archive()).unwrap();
    app.select_list_index(ai);
    app.focus_tasks();
    app.select_task_visible(0);
    app.start_move_task();
    assert!(matches!(app.mode, Mode::MovePicker(_)));
    app.move_picker_confirm(); // only non-archive target is Work

    let work = app.lists.iter().find(|l| l.name == "Work").unwrap();
    let arch = app.lists.iter().find(|l| l.is_archive()).unwrap();
    assert!(work.tasks.iter().any(|t| t.title == "t"));
    assert_eq!(arch.tasks.len(), 0);
}

#[test]
fn archived_list_is_protected_from_rename_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t".to_string());
    app.archive_selection();

    let ai = app.lists.iter().position(|l| l.is_archive()).unwrap();
    app.select_list_index(ai); // focuses the Lists pane

    app.start_rename_list();
    assert!(!matches!(app.mode, Mode::Input(_)));
    assert!(!app.status.is_empty());

    app.start_delete(); // `X` on the Archived list
    assert!(!matches!(app.mode, Mode::Confirm(_)));
    assert!(app.lists.iter().any(|l| l.is_archive()));
}

#[test]
fn permanent_delete_removes_an_archived_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t".to_string());
    app.archive_selection();

    let ai = app.lists.iter().position(|l| l.is_archive()).unwrap();
    app.select_list_index(ai);
    app.focus_tasks();
    app.select_task_visible(0);
    app.start_delete(); // `X` → confirm permanent delete
    assert!(matches!(app.mode, Mode::Confirm(_)));
    app.delete_current_task(); // simulate the confirm's accept path

    let arch = app.lists.iter().find(|l| l.is_archive()).unwrap();
    assert_eq!(arch.tasks.len(), 0);
}

#[test]
fn status_messages_expire_after_their_deadline() {
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    app.set_status("moved \"x\" to Work");
    assert_eq!(app.status, "moved \"x\" to Work");
    let deadline = app.status_deadline().expect("a deadline is set");

    // Just before the deadline the message stays.
    app.expire_status(deadline - Duration::from_millis(1));
    assert!(!app.status.is_empty());
    assert!(app.status_deadline().is_some());

    // At the deadline it clears and stops waking the loop.
    app.expire_status(deadline);
    assert!(app.status.is_empty());
    assert!(app.status_deadline().is_none());
}

#[test]
fn move_task_with_no_other_list_is_a_noop_with_hint() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Only".to_string());
    app.add_task("x".to_string());

    app.start_move_task();
    // No picker opened (still a single list) and a hint was set.
    assert!(!app.status.is_empty());
    assert_eq!(app.current_list().unwrap().tasks.len(), 1);
}

#[test]
fn copy_payload_builds_title_notes_and_json() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("buy milk".to_string());
    app.set_current_notes("2% from the corner shop".to_string());
    app.set_current_tags("errand");
    app.cycle_current_priority(); // -> low

    assert_eq!(
        app.copy_payload(CopyWhat::Title).unwrap(),
        "buy milk".to_string()
    );
    assert_eq!(
        app.copy_payload(CopyWhat::Notes).unwrap(),
        "2% from the corner shop".to_string()
    );

    // JSON carries the whole task and round-trips through serde_json.
    let json = app.copy_payload(CopyWhat::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["title"], "buy milk");
    assert_eq!(parsed["notes"], "2% from the corner shop");
    assert_eq!(parsed["priority"], "low");
    assert_eq!(parsed["tags"][0], "errand");
    assert!(parsed["id"].is_string());
    assert!(parsed["created"].is_string());
}

#[test]
fn copy_menu_opens_navigates_and_cancels() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    // No task yet: the menu refuses to open.
    app.start_copy();
    assert!(!matches!(app.mode, Mode::CopyMenu(_)));

    app.add_list("L".to_string());
    app.add_task("t".to_string());

    app.start_copy();
    let selected = |app: &App| match &app.mode {
        Mode::CopyMenu(s) => s.selected,
        other => panic!("expected the copy menu, got {other:?}"),
    };
    assert_eq!(selected(&app), 0);

    // Highlight wraps in both directions across the three options.
    app.copy_menu_move(-1);
    assert_eq!(selected(&app), CopyWhat::all().len() - 1);
    app.copy_menu_move(1);
    assert_eq!(selected(&app), 0);

    // Esc closes the menu without touching the clipboard.
    app.copy_menu_cancel();
    assert!(matches!(app.mode, Mode::Normal));
}

#[test]
fn subtask_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("parent".to_string());

    app.add_subtask("child one".to_string());
    app.add_subtask("child two".to_string());
    assert_eq!(app.current_task().unwrap().subtasks.len(), 2);
    assert_eq!(app.current_task().unwrap().subtask_progress(), Some((0, 2)));

    // The newest subtask is selected; toggle and delete it.
    app.toggle_current_subtask();
    assert_eq!(app.current_task().unwrap().subtask_progress(), Some((1, 2)));

    app.delete_current_subtask();
    assert_eq!(app.current_task().unwrap().subtasks.len(), 1);
}

#[test]
fn filter_hides_and_search_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("buy milk".to_string());
    app.add_task("write report".to_string());

    // Mark "buy milk" done (it is index 0 after sorting by insertion order).
    app.select_task_visible(0);
    app.toggle_current_done();

    assert_eq!(app.visible_task_indices().len(), 2);

    app.cycle_status_filter(); // -> active
    assert_eq!(app.filter.status.0, StatusFilter::Active);
    assert_eq!(app.visible_task_indices().len(), 1);

    app.cycle_status_filter(); // -> completed
    assert_eq!(app.visible_task_indices().len(), 1);

    app.cycle_status_filter(); // -> all
    app.set_search("report".to_string());
    assert_eq!(app.visible_task_indices().len(), 1);

    app.clear_filter();
    assert_eq!(app.visible_task_indices().len(), 2);
}

// This is the only test that writes the config pointer, so it points
// $TUDO_CONFIG at a temp file to avoid touching the real ~/.config/tudo.
#[test]
fn first_run_and_theme_persist_to_config() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("config.json");
    // SAFETY: single-threaded within this test; no other test reads the config.
    unsafe { std::env::set_var("TUDO_CONFIG", &cfg_path) };

    // Start with no data dir -> first-run mode.
    let mut app = App::new(None).unwrap();
    assert!(app.lists.is_empty());
    assert_eq!(app.theme, ThemeKind::Terminal);

    let target = dir.path().join("data");
    app.commit_first_run(target.clone());

    // A default list is seeded and written to disk.
    assert_eq!(app.lists.len(), 1);
    assert_eq!(app.lists[0].name, "Tasks");
    assert_eq!(storage::load_lists(&target).unwrap().len(), 1);

    // The config pointer records the data dir + theme.
    let saved = config::load_config().unwrap().unwrap();
    assert_eq!(saved.data_dir, target);
    assert_eq!(saved.theme, ThemeKind::Terminal);

    // The theme picker previews live; cancel restores the original without saving.
    // The default theme (Terminal) is the last entry in `ThemeKind::all()`, so
    // stepping +1 wraps around to the first entry, Tokyo Night.
    app.open_theme_picker();
    app.theme_picker_preview(1);
    assert_eq!(app.theme, ThemeKind::TokyoNight);
    app.theme_picker_cancel();
    assert_eq!(app.theme, ThemeKind::Terminal);
    assert_eq!(
        config::load_config().unwrap().unwrap().theme,
        ThemeKind::Terminal
    );

    // Confirming applies and persists the highlighted theme.
    app.open_theme_picker();
    app.theme_picker_preview(1);
    app.theme_picker_confirm();
    assert_eq!(app.theme, ThemeKind::TokyoNight);
    assert_eq!(
        config::load_config().unwrap().unwrap().theme,
        ThemeKind::TokyoNight
    );

    // Relocating moves the lists to a new dir and repoints the config.
    app.add_task("keep me".to_string());
    let moved = dir.path().join("moved");
    app.relocate_data(moved.to_str().unwrap());

    assert_eq!(app.data_dir, moved);
    assert!(
        !target.join("tasks.json").exists(),
        "old file should be gone"
    );
    assert!(moved.join("tasks.json").exists(), "new file should exist");
    assert_eq!(app.current_task().unwrap().title, "keep me");
    let saved = config::load_config().unwrap().unwrap();
    assert_eq!(saved.data_dir, moved);
    assert_eq!(saved.theme, ThemeKind::TokyoNight); // theme preserved

    unsafe { std::env::remove_var("TUDO_CONFIG") };
}

#[test]
fn add_task_without_list_is_a_noop_with_hint() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // No list yet: starting the add flow should refuse and set a hint.
    app.start_add_task();
    assert!(!app.status.is_empty());
    let _ = PathBuf::new();
}

#[test]
fn custom_keybindings_drive_actions_through_the_event_handler() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());

    // Rebind quit onto 'x' and add-task onto 'n' (which normally edits notes).
    let mut ov = Overrides::new();
    ov.insert("quit".to_string(), vec!["x".to_string()]);
    ov.insert("add-task".to_string(), vec!["n".to_string()]);
    let (km, warnings) = Keymap::from_overrides(ov);
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    app.set_keymap(km);

    // 'q' no longer quits now that quit lives on 'x'.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('q')));
    assert!(!app.should_quit, "'q' should not quit after rebinding");

    // 'n' opens the add-task input rather than the notes editor.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('n')));
    match &app.mode {
        Mode::Input(state) => assert_eq!(state.field, InputField::NewTask),
        other => panic!("expected the add-task input, got {other:?}"),
    }
    app.close_overlay();

    // 'x' now quits.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('x')));
    assert!(app.should_quit, "'x' should quit after rebinding");
}

#[test]
fn ctrl_c_always_quits_even_when_rebound() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    // Point quit somewhere else entirely; Ctrl+C is a hardcoded safety net.
    let mut ov = Overrides::new();
    ov.insert("quit".to_string(), vec!["x".to_string()]);
    app.set_keymap(Keymap::from_overrides(ov).0);

    event::handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    assert!(app.should_quit, "Ctrl+C must always quit");
}

#[test]
fn config_round_trips_keybindings() {
    let mut kb = Overrides::new();
    kb.insert(
        "quit".to_string(),
        vec!["x".to_string(), "ctrl+q".to_string()],
    );
    let cfg = config::Config {
        data_dir: PathBuf::from("/tmp/tudo-data"),
        theme: ThemeKind::Dracula,
        keybindings: kb.clone(),
        completed_to_bottom: true,
    };
    let json = serde_json::to_string_pretty(&cfg).unwrap();
    assert!(json.contains("keybindings"), "keybindings missing: {json}");
    let back: config::Config = serde_json::from_str(&json).unwrap();
    assert_eq!(back.keybindings, kb);
    assert!(back.completed_to_bottom);

    // An older/minimal config with no keybindings still loads (defaults empty),
    // and the completed-to-bottom option defaults off.
    let minimal = r#"{"data_dir":"/tmp/x","theme":"nord"}"#;
    let parsed: config::Config = serde_json::from_str(minimal).unwrap();
    assert!(parsed.keybindings.is_empty());
    assert!(!parsed.completed_to_bottom);
    // And a default config omits the field entirely to stay minimal on disk.
    let empty = config::Config {
        data_dir: PathBuf::from("/tmp/x"),
        theme: ThemeKind::Terminal,
        keybindings: Overrides::new(),
        completed_to_bottom: false,
    };
    let json = serde_json::to_string(&empty).unwrap();
    assert!(!json.contains("keybindings"), "empty map should be omitted");
}

// --- sort -------------------------------------------------------------------

/// The titles of the currently visible tasks, in display order. Works in both
/// the list view and the cross-list tag view.
fn visible_titles(app: &App) -> Vec<String> {
    app.visible_task_refs()
        .iter()
        .map(|r| app.lists[r.list].tasks[r.task].title.clone())
        .collect()
}

/// Give the visible task at `vidx` a priority by cycling to it (none -> low ->
/// medium -> high).
fn set_priority(app: &mut App, vidx: usize, p: Priority) {
    app.select_task_visible(vidx);
    let steps = match p {
        Priority::Low => 1,
        Priority::Medium => 2,
        Priority::High => 3,
    };
    for _ in 0..steps {
        app.cycle_current_priority();
    }
}

#[test]
fn default_sort_is_manual_and_preserves_insertion_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("first".to_string());
    app.add_task("second".to_string());
    app.add_task("third".to_string());

    assert_eq!(app.sort, Sort::Manual);
    assert_eq!(visible_titles(&app), ["first", "second", "third"]);
}

#[test]
fn sort_by_priority_puts_high_first_and_none_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("low".to_string()); // raw 0
    app.add_task("high".to_string()); // raw 1
    app.add_task("none".to_string()); // raw 2
    app.add_task("mid".to_string()); // raw 3
    set_priority(&mut app, 0, Priority::Low);
    set_priority(&mut app, 1, Priority::High);
    set_priority(&mut app, 3, Priority::Medium);

    app.set_sort(Sort::Priority);
    assert_eq!(visible_titles(&app), ["high", "mid", "low", "none"]);
}

#[test]
fn sort_by_due_orders_soonest_first_with_undated_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("undated".to_string()); // raw 0
    app.add_task("later".to_string()); // raw 1
    app.add_task("soon".to_string()); // raw 2

    app.select_task_visible(1);
    app.set_current_due("+10");
    app.select_task_visible(2);
    app.set_current_due("+1");

    app.set_sort(Sort::Due);
    assert_eq!(visible_titles(&app), ["soon", "later", "undated"]);
}

#[test]
fn sort_alphabetical_is_case_insensitive() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("banana".to_string());
    app.add_task("Apple".to_string());
    app.add_task("cherry".to_string());

    app.set_sort(Sort::Alphabetical);
    assert_eq!(visible_titles(&app), ["Apple", "banana", "cherry"]);
}

#[test]
fn ties_fall_back_to_stored_order() {
    // With no priorities set, every task ties under the priority sort, so the
    // (stable) result matches the stored/manual order.
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());

    app.set_sort(Sort::Priority);
    assert_eq!(visible_titles(&app), ["a", "b", "c"]);
}

#[test]
fn changing_sort_keeps_the_same_task_selected() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("low".to_string()); // raw 0
    app.add_task("high".to_string()); // raw 1
    set_priority(&mut app, 0, Priority::Low);
    set_priority(&mut app, 1, Priority::High);

    // Select "low" (top of the manual order), then sort by priority.
    app.select_task_visible(0);
    assert_eq!(app.current_task().unwrap().title, "low");
    app.set_sort(Sort::Priority);

    // "low" is now last, but the cursor still points at it.
    assert_eq!(visible_titles(&app), ["high", "low"]);
    assert_eq!(app.current_task().unwrap().title, "low");
    assert_eq!(app.selected_visible_task(), 1);
}

#[test]
fn reordering_is_refused_under_a_non_manual_sort() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());
    let stored = |app: &App| -> Vec<String> {
        app.current_list()
            .unwrap()
            .tasks
            .iter()
            .map(|t| t.title.clone())
            .collect()
    };

    app.set_sort(Sort::Alphabetical);
    app.select_task_visible(0);

    // Both the nudge and the send-to-edge keys are no-ops and leave a hint.
    app.reorder_task(1);
    assert_eq!(stored(&app), ["a", "b", "c"]);
    assert!(app.status.contains("manual"));

    app.send_task(false);
    assert_eq!(stored(&app), ["a", "b", "c"]);

    // Back on manual sort, reordering works again.
    app.set_sort(Sort::Manual);
    app.select_task_visible(0);
    app.reorder_task(1);
    assert_eq!(stored(&app), ["b", "a", "c"]);
}

#[test]
fn sort_picker_opens_and_quick_key_applies() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("x".to_string());

    // `o` opens the picker; `2` picks the second option (Priority).
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('o')));
    assert!(matches!(app.mode, Mode::SortPicker(_)));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('2')));
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.sort, Sort::Priority);

    // Reopen and cancel with Esc: the sort is unchanged.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('o')));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.sort, Sort::Priority);
}

#[test]
fn quick_add_reads_story_points_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());

    app.add_task("Estimate this ~8".to_string());
    let task = app.current_task().unwrap();
    assert_eq!(task.title, "Estimate this");
    assert_eq!(task.points, Some(8));

    // Survives a round-trip through disk.
    let reloaded = app_in(dir.path());
    assert_eq!(reloaded.lists[0].tasks[0].points, Some(8));
}

#[test]
fn sort_by_points_orders_fewest_first_with_unestimated_last() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("none".to_string()); // raw 0, unestimated
    app.add_task("three".to_string()); // raw 1
    app.add_task("one".to_string()); // raw 2
    app.add_task("two".to_string()); // raw 3

    app.select_task_visible(1);
    app.set_current_points("3");
    app.select_task_visible(2);
    app.set_current_points("1");
    app.select_task_visible(3);
    app.set_current_points("2");

    app.set_sort(Sort::Points);
    assert_eq!(visible_titles(&app), ["one", "two", "three", "none"]);
}

#[test]
fn toggling_direction_reverses_the_active_sort() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("none".to_string());
    app.add_task("three".to_string());
    app.add_task("one".to_string());
    app.add_task("two".to_string());
    app.select_task_visible(1);
    app.set_current_points("3");
    app.select_task_visible(2);
    app.set_current_points("1");
    app.select_task_visible(3);
    app.set_current_points("2");

    app.set_sort(Sort::Points);
    assert_eq!(app.sort_dir, SortDir::Ascending);
    assert_eq!(visible_titles(&app), ["one", "two", "three", "none"]);

    // Descending mirrors the whole order (unestimated floats to the front).
    app.toggle_sort_dir();
    assert_eq!(app.sort_dir, SortDir::Descending);
    assert_eq!(visible_titles(&app), ["none", "three", "two", "one"]);

    // And back again.
    app.toggle_sort_dir();
    assert_eq!(app.sort_dir, SortDir::Ascending);
    assert_eq!(visible_titles(&app), ["one", "two", "three", "none"]);
}

#[test]
fn direction_does_not_disturb_manual_order() {
    // Manual reports every pair equal, so flipping direction is a stable no-op.
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a".to_string());
    app.add_task("b".to_string());
    app.add_task("c".to_string());

    assert_eq!(app.sort, Sort::Manual);
    app.toggle_sort_dir();
    assert_eq!(visible_titles(&app), ["a", "b", "c"]);
}

#[test]
fn sort_picker_flip_key_and_points_quick_key() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("x".to_string());

    // `o` opens the picker; `r` flips the direction; `3` picks Points (3rd option).
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('o')));
    assert!(matches!(app.mode, Mode::SortPicker(_)));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('r')));
    assert_eq!(app.sort_dir, SortDir::Descending);
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('3')));
    assert!(matches!(app.mode, Mode::Normal));
    assert_eq!(app.sort, Sort::Points);
    assert_eq!(app.sort_dir, SortDir::Descending);
}

// --- cross-list tag view -------------------------------------------------

/// Rendered index of the sidebar row for tag `name`.
fn tag_row(app: &App, name: &str) -> usize {
    app.sidebar_rows()
        .iter()
        .position(|r| matches!(r, SidebarRow::Tag(t) if t == name))
        .unwrap_or_else(|| panic!("tag row #{name} not found in sidebar"))
}

#[test]
fn tags_appear_in_the_sidebar_with_cross_list_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // add_list sorts by name: Alpha = 0, Beta = 1.
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());

    app.select_list_index(0);
    app.add_task("a #x".to_string());
    app.select_list_index(1);
    app.add_task("b #x".to_string());
    app.add_task("c #y".to_string());

    // The flat sidebar order is every list, then the sorted union of tags.
    assert_eq!(
        app.sidebar_rows(),
        vec![
            SidebarRow::List(0),
            SidebarRow::List(1),
            SidebarRow::Tag("x".to_string()),
            SidebarRow::Tag("y".to_string()),
        ]
    );
    // Counts span lists (x is on a task in each of Alpha and Beta).
    assert_eq!(app.tag_counts("x"), (2, 2));
    assert_eq!(app.tag_counts("y"), (1, 1));
}

#[test]
fn tag_view_shows_matching_tasks_from_every_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(0);
    app.add_task("a #x".to_string());
    app.add_task("skip me".to_string());
    app.select_list_index(1);
    app.add_task("b #x".to_string());

    app.select_sidebar_row(tag_row(&app, "x"));
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
    // Only the tagged tasks, spanning both lists, in list-then-task order.
    assert_eq!(visible_titles(&app), ["a", "b"]);
}

#[test]
fn editing_a_task_in_the_tag_view_persists_to_its_home_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(0);
    app.add_task("a #x".to_string());
    app.select_list_index(1);
    app.add_task("b #x".to_string());

    app.select_sidebar_row(tag_row(&app, "x"));
    // Complete "a" (from Alpha) and prioritise "b" (from Beta) from the tag view.
    app.select_task_visible(0);
    app.toggle_current_done();
    app.select_task_visible(1);
    app.cycle_current_priority();

    // Both edits land on the right home list and survive a reload from disk.
    let reloaded = app_in(dir.path());
    let alpha = reloaded.lists.iter().find(|l| l.name == "Alpha").unwrap();
    let beta = reloaded.lists.iter().find(|l| l.name == "Beta").unwrap();
    assert!(alpha.tasks[0].done, "Alpha's task should be done");
    assert_eq!(beta.tasks[0].priority, Some(Priority::Low));
}

#[test]
fn clearing_the_last_use_of_a_tag_drops_it_and_reselects_safely() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t #z".to_string());

    // View the tag, then strip it from its only task.
    app.select_sidebar_row(tag_row(&app, "z"));
    assert_eq!(app.current_view(), Some(View::Tag("z".to_string())));
    app.set_current_tags("");

    // The tag is gone from the sidebar and the selection falls back to the list.
    assert!(app.all_tags().is_empty());
    assert!(
        !app.sidebar_rows()
            .iter()
            .any(|r| matches!(r, SidebarRow::Tag(_)))
    );
    assert_eq!(app.current_view(), Some(View::List(0)));
}

#[test]
fn sidebar_jk_round_robins_within_each_pane() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(0);
    app.add_task("a #x".to_string());
    app.select_list_index(1);
    app.add_task("b #y".to_string());
    assert_eq!(app.all_tags().len(), 2);

    // `j` off the last list wraps back to the first list rather than spilling
    // into the tag pane; `k` off the first list wraps to the last.
    app.select_list_index(1);
    app.move_selection(1);
    assert_eq!(app.selected_list_index(), Some(0));
    app.move_selection(-1);
    assert_eq!(app.selected_list_index(), Some(1));

    // The tags pane round-robins on its own, too.
    let tags = app.all_tags();
    let (first, last) = (tags[0].clone(), tags[tags.len() - 1].clone());
    app.select_sidebar_row(tag_row(&app, &last));
    app.move_selection(1);
    assert_eq!(app.current_view(), Some(View::Tag(first.clone())));
    app.move_selection(-1);
    assert_eq!(app.current_view(), Some(View::Tag(last)));
}

#[test]
fn toggle_sidebar_pane_flips_between_lists_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(1);
    app.add_task("a #x".to_string());

    // From a list the toggle jumps straight to the tags, no matter how many
    // lists sit in between. Drive it through the real key path (`\`) so the
    // keymap lookup and dispatch are exercised, not just the method.
    app.select_list_index(1);
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('\\')));
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
    // ...and back onto the list you were on.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('\\')));
    assert_eq!(app.selected_list_index(), Some(1));

    // From the tasks pane it first steps back into the sidebar.
    app.focus_tasks();
    assert_eq!(app.focus, Focus::Tasks);
    app.toggle_sidebar_pane();
    assert_eq!(app.focus, Focus::Lists);

    // With no tags at all, crossing into the (absent) tag pane is a no-op.
    app.set_current_tags("");
    assert!(app.all_tags().is_empty());
    app.toggle_sidebar_pane();
    assert_eq!(app.selected_list_index(), Some(1));
}

#[test]
fn task_list_jk_round_robins_but_wheel_clamps() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("one".to_string());
    app.add_task("two".to_string());
    app.add_task("three".to_string());

    // `k` off the first task wraps to the last; `j` off the last wraps to the
    // first.
    app.select_task_visible(0);
    app.move_selection(-1);
    assert_eq!(app.selected_visible_task(), 2);
    app.move_selection(1);
    assert_eq!(app.selected_visible_task(), 0);

    // The wheel clamps at the ends instead of wrapping.
    app.select_task_visible(2);
    app.scroll_selection(1);
    assert_eq!(app.selected_visible_task(), 2);
    app.select_task_visible(0);
    app.scroll_selection(-1);
    assert_eq!(app.selected_visible_task(), 0);
}

#[test]
fn note_list_jk_round_robins_but_wheel_clamps() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_notebook("N".to_string());
    app.add_note("one".to_string());
    app.add_note("two".to_string());
    app.add_note("three".to_string());
    app.note_focus_notes();

    // `k` off the first note wraps to the last; `j` off the last wraps back.
    app.select_note_visible(0);
    app.note_move_selection(-1);
    assert_eq!(app.selected_visible_note(), 2);
    app.note_move_selection(1);
    assert_eq!(app.selected_visible_note(), 0);

    // The wheel clamps at the ends.
    app.select_note_visible(2);
    app.note_scroll_selection(1);
    assert_eq!(app.selected_visible_note(), 2);
    app.select_note_visible(0);
    app.note_scroll_selection(-1);
    assert_eq!(app.selected_visible_note(), 0);
}

#[test]
fn scroll_wheel_stops_at_the_lists_tags_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(0);
    app.add_task("a #x".to_string());

    // Unlike `j`/`k`, scrolling to the end of the lists stays on the last list
    // instead of spilling into the tag pane.
    app.select_list_index(1);
    app.scroll_selection(1);
    assert_eq!(app.selected_list_index(), Some(1));
    assert_eq!(app.current_view(), Some(View::List(1)));

    // From within the tags, the wheel likewise stays on the tags.
    app.select_sidebar_row(tag_row(&app, "x"));
    app.scroll_selection(-1);
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
}

#[test]
fn adding_a_task_is_refused_in_the_tag_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t #x".to_string());

    app.select_sidebar_row(tag_row(&app, "x"));
    // The picker never opens...
    app.start_add_task();
    assert!(matches!(app.mode, Mode::Normal));
    assert!(!app.status.is_empty());
    // ...and a direct add is a no-op (still one task, still just #x).
    app.add_task("nope".to_string());
    assert_eq!(app.lists[0].tasks.len(), 1);
}

#[test]
fn status_filter_composes_with_the_tag_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("a #x".to_string());
    app.add_task("b #x".to_string());

    // Mark "b" done, then view tag x.
    app.select_task_visible(1);
    app.toggle_current_done();
    app.select_sidebar_row(tag_row(&app, "x"));
    assert_eq!(visible_titles(&app), ["a", "b"]);

    // The status filter narrows the cross-list set just like in a list.
    app.cycle_status_filter(); // -> active
    assert_eq!(visible_titles(&app), ["a"]);
    app.cycle_status_filter(); // -> completed
    assert_eq!(visible_titles(&app), ["b"]);
}

#[test]
fn tab_toggles_between_the_sidebar_and_tasks_without_touching_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t #x".to_string());

    // Start on the list in the sidebar (add_task leaves focus on Tasks).
    app.focus_lists();
    app.select_list_index(0);
    assert_eq!(app.current_view(), Some(View::List(0)));

    // Tab drops straight into the selected list's tasks — not the tags — and
    // toggling back leaves the same list selected.
    app.toggle_focus();
    assert_eq!(app.focus, Focus::Tasks);
    assert_eq!(app.current_view(), Some(View::List(0)));
    app.toggle_focus();
    assert_eq!(app.focus, Focus::Lists);
    assert_eq!(app.current_view(), Some(View::List(0)));
}

#[test]
fn focus_tags_jumps_to_the_tags_and_focus_lists_returns() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t #x".to_string());
    app.focus_lists();
    app.select_list_index(0);
    assert_eq!(app.current_view(), Some(View::List(0)));

    // `#` jumps the sidebar cursor onto the tag; `h` brings it back to the list.
    app.focus_tags();
    assert_eq!(app.focus, Focus::Lists);
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
    app.focus_lists();
    assert_eq!(app.current_view(), Some(View::List(0)));
}

#[test]
fn focus_tags_is_a_noop_when_there_are_no_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("no tags here".to_string());
    app.focus_lists();
    app.select_list_index(0);

    // Nothing to focus, so the cursor stays on the list.
    app.focus_tags();
    assert_eq!(app.focus, Focus::Lists);
    assert_eq!(app.current_view(), Some(View::List(0)));
}

#[test]
fn hopping_between_lists_and_tags_restores_your_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Alpha".to_string());
    app.add_list("Beta".to_string());
    app.select_list_index(0);
    app.add_task("a #x".to_string());

    // Sit on Beta (list 1), hop to the tags and back — land on Beta, not Alpha.
    app.focus_lists();
    app.select_list_index(1);
    app.focus_tags();
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
    app.focus_lists();
    assert_eq!(app.current_view(), Some(View::List(1)));
}

#[test]
fn tab_key_toggles_and_hash_key_jumps_to_the_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("Work".to_string());
    app.add_task("t #x".to_string());
    app.focus_lists();
    app.select_list_index(0);

    // Tab / BackTab (Shift+Tab) both toggle sidebar ↔ tasks, keeping the list.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Tasks);
    assert_eq!(app.current_view(), Some(View::List(0)));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::BackTab));
    assert_eq!(app.focus, Focus::Lists);
    assert_eq!(app.current_view(), Some(View::List(0)));

    // `#` jumps straight to the tag.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('#')));
    assert_eq!(app.current_view(), Some(View::Tag("x".to_string())));
}

#[test]
fn archiving_a_tagged_task_removes_the_tag_from_the_sidebar() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("L".to_string());
    app.add_task("t #x".to_string());
    assert_eq!(app.all_tags(), vec!["x".to_string()]);

    // Archived tasks are excluded from the tag scope, so the tag disappears.
    app.focus_tasks();
    app.archive_selection();
    assert!(app.all_tags().is_empty());
    assert!(
        !app.sidebar_rows()
            .iter()
            .any(|r| matches!(r, SidebarRow::Tag(_)))
    );
}

// --- Notes tab --------------------------------------------------------------

#[test]
fn number_keys_switch_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    assert_eq!(app.active_tab, Tab::Tasks);

    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.active_tab, Tab::Notes);
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('1')));
    assert_eq!(app.active_tab, Tab::Tasks);
}

#[test]
fn notebook_and_note_crud_persists_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);

    app.add_notebook("Journal".to_string());
    assert_eq!(app.notebooks.len(), 1);
    assert_eq!(app.current_notebook().unwrap().name, "Journal");

    app.add_note("First entry".to_string());
    assert_eq!(app.current_note().unwrap().title, "First entry");
    app.set_current_note_body("Body text here.".to_string());
    app.set_current_note_tags("diary, morning");
    assert_eq!(app.current_note().unwrap().tags, vec!["diary", "morning"]);

    // Reload from disk to confirm the .md file round-trips.
    let reloaded = app_in(dir.path());
    assert_eq!(reloaded.notebooks.len(), 1);
    assert_eq!(reloaded.notebooks[0].notes.len(), 1);
    let note = &reloaded.notebooks[0].notes[0];
    assert_eq!(note.title, "First entry");
    assert_eq!(note.body, "Body text here.");
    assert_eq!(note.tags, vec!["diary", "morning"]);
}

#[test]
fn note_search_filters_by_title_body_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Groceries".to_string());
    app.add_note("Meeting".to_string());
    app.set_current_note_body("discuss the roadmap".to_string());

    app.set_note_search("road".to_string());
    let visible = app.visible_note_refs();
    assert_eq!(visible.len(), 1);
    assert_eq!(app.current_note().unwrap().title, "Meeting");

    app.clear_note_filter();
    assert_eq!(app.visible_note_refs().len(), 2);
}

#[test]
fn note_tags_form_a_cross_notebook_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);

    app.add_notebook("A".to_string());
    app.add_note("a1".to_string());
    app.set_current_note_tags("idea");
    app.add_notebook("B".to_string());
    app.add_note("b1".to_string());
    app.set_current_note_tags("idea");

    assert_eq!(app.all_note_tags(), vec!["idea".to_string()]);
    // Selecting the tag row surfaces notes from both notebooks.
    let tag_row = app.note_sidebar_rows().len() - 1;
    app.select_note_sidebar_row(tag_row);
    assert_eq!(
        app.current_note_view(),
        Some(NoteView::Tag("idea".to_string()))
    );
    assert_eq!(app.visible_note_refs().len(), 2);
}

#[test]
fn pinned_notes_float_to_the_top() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("first".to_string());
    app.add_note("second".to_string());

    // Pin "second"; it should jump to the top of the visible order.
    app.note_focus_notes();
    // The most recently added note is selected; pin it.
    let before = app.current_note().unwrap().title.clone();
    app.toggle_current_note_pin();
    let top = &app.notebooks[0].notes[app.visible_note_refs()[0].note];
    assert!(top.pinned);
    assert_eq!(top.title, before);
    // The pin survives a reload.
    let reloaded = app_in(dir.path());
    assert!(reloaded.notebooks[0].notes.iter().any(|n| n.pinned));
}

#[test]
fn notes_reorder_under_manual_sort() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("alpha".to_string());
    app.add_note("beta".to_string());
    app.add_note("gamma".to_string());
    app.note_focus_notes();

    // Titles in current manual (insertion) order.
    let titles = |a: &App| {
        a.visible_note_refs()
            .iter()
            .map(|r| a.notebooks[r.notebook].notes[r.note].title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&app), vec!["alpha", "beta", "gamma"]);

    // Select the top note and send it to the bottom.
    app.note_state.select(Some(0));
    app.send_note(false);
    assert_eq!(titles(&app), vec!["beta", "gamma", "alpha"]);

    // Order survives a reload.
    let reloaded = app_in(dir.path());
    assert_eq!(titles(&reloaded), vec!["beta", "gamma", "alpha"]);
}

#[test]
fn note_sort_alphabetical_orders_by_title() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Charlie".to_string());
    app.add_note("Alpha".to_string());
    app.add_note("Bravo".to_string());

    app.set_note_sort(NoteSort::Alphabetical);
    let titles: Vec<_> = app
        .visible_note_refs()
        .iter()
        .map(|r| app.notebooks[r.notebook].notes[r.note].title.clone())
        .collect();
    assert_eq!(titles, vec!["Alpha", "Bravo", "Charlie"]);
}

#[test]
fn keys_drive_notes_creation_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    // 2 -> Notes tab.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('2')));
    assert_eq!(app.active_tab, Tab::Notes);

    // A -> add notebook, type name, Enter.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('A')));
    assert!(matches!(app.mode, Mode::Input(_)));
    type_str(&mut app, "Work");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.notebooks.len(), 1);

    // a -> add note, type title, Enter.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
    type_str(&mut app, "Hello");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    assert_eq!(app.current_note().unwrap().title, "Hello");

    // n -> edit body, type, Ctrl+S saves.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('n')));
    assert!(matches!(app.mode, Mode::Input(_)));
    type_str(&mut app, "world");
    event::handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
    );
    assert_eq!(app.current_note().unwrap().body, "world");

    // * -> pin the note.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('*')));
    assert!(app.current_note().unwrap().pinned);

    // Everything survives a reload.
    let reloaded = app_in(dir.path());
    let note = &reloaded.notebooks[0].notes[0];
    assert_eq!(note.title, "Hello");
    assert_eq!(note.body, "world");
    assert!(note.pinned);
}

#[test]
fn esc_autosaves_the_note_body() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('2')));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('A')));
    type_str(&mut app, "NB");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('a')));
    type_str(&mut app, "Note");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));

    // Enter opens the full-screen body editor; Esc saves and closes.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::Input(_)));
    type_str(&mut app, "auto saved body");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Esc));
    assert!(matches!(app.mode, Mode::Normal), "editor should close");
    assert_eq!(app.current_note().unwrap().body, "auto saved body");

    // And it's on disk.
    let reloaded = app_in(dir.path());
    assert_eq!(reloaded.notebooks[0].notes[0].body, "auto saved body");
}

#[test]
fn creating_a_note_parses_hash_tags() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Roadmap sync #work #planning".to_string());

    let note = app.current_note().unwrap();
    assert_eq!(note.title, "Roadmap sync");
    assert_eq!(note.tags, vec!["work", "planning"]);
    // The tags show up in the sidebar tag pane.
    assert_eq!(
        app.all_note_tags(),
        vec!["planning".to_string(), "work".to_string()]
    );
}

#[test]
fn archiving_a_note_moves_it_to_the_archived_notebook() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("Work".to_string());
    app.add_note("keeper".to_string());
    app.note_focus_notes();

    app.archive_current_note();

    // It left Work and landed in a new Archived notebook pinned to the bottom.
    let work = app.notebooks.iter().find(|n| n.name == "Work").unwrap();
    assert!(work.notes.is_empty());
    let archived = app.notebooks.iter().find(|n| n.is_archive()).unwrap();
    assert_eq!(archived.notes.len(), 1);
    assert_eq!(archived.notes[0].title, "keeper");
    assert!(
        app.notebooks.last().unwrap().is_archive(),
        "archive pinned bottom"
    );

    // Survives a reload.
    let reloaded = app_in(dir.path());
    assert!(
        reloaded
            .notebooks
            .iter()
            .any(|n| n.is_archive() && n.notes.len() == 1)
    );
    assert!(reloaded.notebooks.last().unwrap().is_archive());
}

#[test]
fn archived_note_tags_leave_the_sidebar() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("t #x".to_string());
    assert_eq!(app.all_note_tags(), vec!["x".to_string()]);

    app.note_focus_notes();
    app.archive_current_note();
    // Archived notes are excluded from the tag scope, so the tag disappears.
    assert!(app.all_note_tags().is_empty());
}

#[test]
fn moving_a_note_out_of_the_archive_unarchives_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("Inbox".to_string());
    app.add_note("resurface me".to_string());
    app.note_focus_notes();
    app.archive_current_note();

    // Select the Archived notebook and its note.
    let archive_row = app
        .note_sidebar_rows()
        .iter()
        .position(|r| matches!(r, NoteSidebarRow::Notebook(i) if app.notebooks[*i].is_archive()))
        .unwrap();
    app.select_note_sidebar_row(archive_row);
    app.note_focus_notes();

    app.start_move_note();
    assert!(matches!(app.mode, Mode::NoteMovePicker(_)));
    app.note_move_picker_confirm(); // the only target is Inbox

    let inbox = app.notebooks.iter().find(|n| n.name == "Inbox").unwrap();
    assert_eq!(inbox.notes.len(), 1, "note is back in Inbox");
    let archived = app.notebooks.iter().find(|n| n.is_archive()).unwrap();
    assert!(archived.notes.is_empty(), "archive is empty again");
}

#[test]
fn archived_notebook_cannot_be_renamed_or_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("x".to_string());
    app.note_focus_notes();
    app.archive_current_note();

    let archive_row = app
        .note_sidebar_rows()
        .iter()
        .position(|r| matches!(r, NoteSidebarRow::Notebook(i) if app.notebooks[*i].is_archive()))
        .unwrap();
    app.select_note_sidebar_row(archive_row); // selects it, focuses notebooks pane

    app.start_rename_notebook();
    assert!(matches!(app.mode, Mode::Normal), "rename refused");
    app.start_delete_note();
    assert!(matches!(app.mode, Mode::Normal), "delete refused");
}

#[test]
fn multiline_editor_supports_up_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("t".to_string());
    app.note_focus_notes();
    app.start_edit_note_body();

    // Type "abc", newline, "de" -> cursor at end of the second line.
    type_str(&mut app, "abc");
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    type_str(&mut app, "de");

    // Up keeps the column (2), landing between 'b' and 'c' on the first line.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Up));
    let Mode::Input(input) = &app.mode else {
        panic!("not editing");
    };
    assert_eq!(input.cursor, 2);

    // Delete removes the character under the cursor ('c').
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Delete));
    let Mode::Input(input) = &app.mode else {
        panic!("not editing");
    };
    assert_eq!(input.buffer, "ab\nde");
}

#[test]
fn scroll_wheel_moves_the_cursor_in_the_note_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("t".to_string());
    app.set_current_note_body("l0\nl1\nl2\nl3\nl4\nl5".to_string());
    app.note_focus_notes();
    app.note_activate(); // full-screen editor, cursor at the end

    let before = match &app.mode {
        Mode::Input(i) => i.cursor,
        _ => panic!("not editing"),
    };
    let scroll = |kind| MouseEvent {
        kind,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    event::handle_mouse(&mut app, scroll(MouseEventKind::ScrollUp));
    let after = match &app.mode {
        Mode::Input(i) => i.cursor,
        _ => panic!("not editing"),
    };
    assert!(after < before, "scrolling up moved the cursor up");
}

#[test]
fn reordering_notebooks_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("Alpha".to_string());
    app.add_notebook("Beta".to_string());
    app.add_notebook("Gamma".to_string());

    let names = |a: &App| {
        a.notebooks
            .iter()
            .map(|n| n.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&app), vec!["Alpha", "Beta", "Gamma"]);

    // Focus the notebooks pane, select Alpha, and nudge it down twice via `J`.
    app.note_focus_notebooks();
    app.notebook_state.select(Some(0));
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('J')));
    assert_eq!(names(&app), vec!["Beta", "Alpha", "Gamma"]);
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Char('J')));
    assert_eq!(names(&app), vec!["Beta", "Gamma", "Alpha"]);

    // The manual order survives a reload.
    let reloaded = app_in(dir.path());
    assert_eq!(names(&reloaded), vec!["Beta", "Gamma", "Alpha"]);
}

#[test]
fn note_copy_payload_builds_body_title_and_markdown() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Title here #tag".to_string());
    app.set_current_note_body("body line".to_string());

    assert_eq!(
        app.note_copy_payload(NoteCopyWhat::Title).unwrap(),
        "Title here"
    );
    assert_eq!(
        app.note_copy_payload(NoteCopyWhat::Body).unwrap(),
        "body line"
    );
    let md = app.note_copy_payload(NoteCopyWhat::Markdown).unwrap();
    assert!(md.contains("title: Title here"), "frontmatter: {md}");
    assert!(md.contains("tags: [tag]"), "tags: {md}");
    assert!(md.contains("body line"), "body: {md}");
}

#[test]
fn note_links_resolve_and_backlink() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Target".to_string());
    app.add_note("Source".to_string());
    app.set_current_note_body("see [[Target]]".to_string());

    // Resolution is case-insensitive.
    let target = app.resolve_note_link("target").unwrap();
    assert_eq!(
        app.notebooks[target.notebook].notes[target.note].title,
        "Target"
    );

    // Target's backlinks include Source.
    let back = app.note_backlinks("Target", None);
    assert_eq!(back.len(), 1);
    assert_eq!(
        app.notebooks[back[0].notebook].notes[back[0].note].title,
        "Source"
    );
}

#[test]
fn following_a_link_jumps_and_opens_the_read_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("Target".to_string());
    app.add_note("Source".to_string());
    app.set_current_note_body("[[Target]]".to_string());

    app.start_note_link_picker(false);
    assert!(matches!(app.mode, Mode::NoteLinkPicker(_)));
    app.note_link_picker_confirm();

    assert!(matches!(app.mode, Mode::NoteView(_)), "landed in read view");
    assert_eq!(app.current_note().unwrap().title, "Target");
}

#[test]
fn tab_toggles_between_editor_and_read_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("N".to_string());
    app.add_note("N1".to_string());
    app.note_focus_notes();

    // Enter opens the raw editor.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Enter));
    assert!(matches!(app.mode, Mode::Input(_)));
    // Tab flips to the rendered read view (saving on the way).
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert!(matches!(app.mode, Mode::NoteView(_)));
    // Tab flips back to the editor.
    event::handle_key(&mut app, KeyEvent::from(KeyCode::Tab));
    assert!(matches!(app.mode, Mode::Input(_)));
}

#[test]
fn deleting_a_notebook_removes_its_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.switch_tab(Tab::Notes);
    app.add_notebook("Temp".to_string());
    app.add_note("note".to_string());
    assert!(dir.path().join("notes").join("Temp").exists());

    app.note_focus_notebooks();
    app.delete_current_notebook();
    assert!(app.notebooks.is_empty());
    assert!(!dir.path().join("notes").join("Temp").exists());
    assert_eq!(app.notes_focus, NotesFocus::Notebooks);
}

#[test]
fn deleting_all_done_tasks_deletes_done_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("my_list".to_string());
    app.select_list_index(0);
    app.add_task("foo".to_string());
    app.add_task("done".to_string());
    app.toggle_current_done();

    app.delete_done_tasks();
    assert!(app.lists[0].tasks.len() == 1);
}

#[test]
fn deleting_all_done_tasks_is_scoped_to_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    app.add_list("my_list".to_string());
    app.select_list_index(0);
    app.add_task("foo".to_string());
    app.add_task("done".to_string());
    app.toggle_current_done();

    app.add_list("to_delete_from".to_string());
    app.select_list_index(1);
    app.add_task("foo2".to_string());
    app.add_task("done2".to_string());
    app.toggle_current_done();

    app.delete_done_tasks();
    assert!(app.lists[0].tasks.len() == 2);
    assert!(app.lists[0].tasks.iter().filter(|t| t.done).count() == 1);
    assert!(app.lists[1].tasks.len() == 1);
}

