//! Action-logic tests driving `App` directly (no terminal involved).

use std::path::PathBuf;

use tudo::app::{App, CopyWhat, Mode, StatusFilter};
use tudo::model::Priority;
use tudo::theme::ThemeKind;
use tudo::{config, storage};

fn app_in(dir: &std::path::Path) -> App {
    App::new(Some(dir.to_path_buf())).unwrap()
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
    let raw: Vec<String> = app.current_list()
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
