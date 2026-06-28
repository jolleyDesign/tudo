//! Action-logic tests driving `App` directly (no terminal involved).

use std::path::PathBuf;

use tudo::app::{App, StatusFilter};
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
    assert_eq!(app.theme, ThemeKind::TokyoNight);

    let target = dir.path().join("data");
    app.commit_first_run(target.clone());

    // A default list is seeded and written to disk.
    assert_eq!(app.lists.len(), 1);
    assert_eq!(app.lists[0].name, "Tasks");
    assert_eq!(storage::load_lists(&target).unwrap().len(), 1);

    // The config pointer records the data dir + theme.
    let saved = config::load_config().unwrap().unwrap();
    assert_eq!(saved.data_dir, target);
    assert_eq!(saved.theme, ThemeKind::TokyoNight);

    // The theme picker previews live; cancel restores the original without saving.
    app.open_theme_picker();
    app.theme_picker_preview(1);
    assert_eq!(app.theme, ThemeKind::CatppuccinMocha);
    app.theme_picker_cancel();
    assert_eq!(app.theme, ThemeKind::TokyoNight);
    assert_eq!(
        config::load_config().unwrap().unwrap().theme,
        ThemeKind::TokyoNight
    );

    // Confirming applies and persists the highlighted theme.
    app.open_theme_picker();
    app.theme_picker_preview(1);
    app.theme_picker_confirm();
    assert_eq!(app.theme, ThemeKind::CatppuccinMocha);
    assert_eq!(
        config::load_config().unwrap().unwrap().theme,
        ThemeKind::CatppuccinMocha
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
    assert_eq!(saved.theme, ThemeKind::CatppuccinMocha); // theme preserved

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
