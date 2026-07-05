//! Storage round-trips and pure helpers (slugify, parse_due, parse_tags).

use chrono::{NaiveDate, Utc};
use tudo::model::{self, List, Note, Priority, Task};
use tudo::storage;
use tudo::theme::ThemeKind;

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn slugify_handles_punctuation_and_collisions() {
    assert_eq!(storage::slugify("Work"), "work");
    assert_eq!(storage::slugify("My Big Project!"), "my-big-project");
    assert_eq!(storage::slugify("  spaced  out  "), "spaced-out");
    assert_eq!(storage::slugify("***"), "list");
}

#[test]
fn parse_due_accepts_absolute_relative_and_empty() {
    let today = date(2026, 6, 28);
    assert_eq!(model::parse_due("", today).unwrap(), None);
    assert_eq!(
        model::parse_due("2026-07-01", today).unwrap(),
        Some(date(2026, 7, 1))
    );
    assert_eq!(model::parse_due("today", today).unwrap(), Some(today));
    assert_eq!(
        model::parse_due("tomorrow", today).unwrap(),
        Some(date(2026, 6, 29))
    );
    assert_eq!(
        model::parse_due("+3", today).unwrap(),
        Some(date(2026, 7, 1))
    );
    assert!(model::parse_due("not-a-date", today).is_err());
}

#[test]
fn parse_tags_normalizes_and_dedupes() {
    let tags = model::parse_tags("Work, #Urgent  work\t#urgent");
    assert_eq!(tags, vec!["work".to_string(), "urgent".to_string()]);
    assert!(model::parse_tags("   ").is_empty());
}

#[test]
fn save_and_load_round_trips_all_fields() {
    let dir = tempfile::tempdir().unwrap();

    let mut list = List::new("Work");
    list.slug = "work".to_string();
    let mut task = Task::new("Ship the TUI");
    task.priority = Some(Priority::High);
    task.points = Some(8);
    task.due = Some(date(2026, 7, 1));
    task.tags = vec!["rust".to_string(), "urgent".to_string()];
    task.notes = "line one\nline two".to_string();
    task.toggle_done(); // sets done + completed_at
    list.tasks.push(task);

    storage::save_list(dir.path(), &list).unwrap();

    let loaded = storage::load_lists(dir.path()).unwrap();
    assert_eq!(loaded.len(), 1);
    let l = &loaded[0];
    assert_eq!(l.name, "Work");
    assert_eq!(l.slug, "work"); // slug comes from the filename
    assert_eq!(l.tasks.len(), 1);
    let t = &l.tasks[0];
    assert_eq!(t.title, "Ship the TUI");
    assert_eq!(t.priority, Some(Priority::High));
    assert_eq!(t.points, Some(8));
    assert_eq!(t.due, Some(date(2026, 7, 1)));
    assert_eq!(t.tags, vec!["rust".to_string(), "urgent".to_string()]);
    assert_eq!(t.notes, "line one\nline two");
    assert!(t.done);
    assert!(t.completed_at.is_some());
}

#[test]
fn save_is_atomic_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = List::new("Tasks");
    list.slug = "tasks".to_string();

    storage::save_list(dir.path(), &list).unwrap();
    list.tasks.push(Task::new("a"));
    storage::save_list(dir.path(), &list).unwrap(); // overwrite

    let entries: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(entries.contains(&"tasks.json".to_string()));
    assert!(
        !entries.iter().any(|e| e.ends_with(".tmp")),
        "no temp files should remain: {entries:?}"
    );

    let loaded = storage::load_lists(dir.path()).unwrap();
    assert_eq!(loaded[0].tasks.len(), 1);
}

#[test]
fn human_readable_json_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = List::new("Work");
    list.slug = "work".to_string();
    list.tasks.push(Task::new("readable"));
    storage::save_list(dir.path(), &list).unwrap();

    let raw = std::fs::read_to_string(dir.path().join("work.json")).unwrap();
    // pretty-printed (indented, multi-line) and not storing the skipped slug field
    assert!(raw.contains("\n  \"name\": \"Work\""));
    assert!(raw.contains("\"title\": \"readable\""));
    assert!(!raw.contains("slug"));
}

#[test]
fn theme_kinds_parse_and_cycle() {
    assert_eq!(
        ThemeKind::from_key("tokyo-night"),
        Some(ThemeKind::TokyoNight)
    );
    assert_eq!(
        ThemeKind::from_key("Catppuccin"),
        Some(ThemeKind::CatppuccinMocha)
    );
    assert_eq!(ThemeKind::from_key("NORD"), Some(ThemeKind::Nord));
    assert_eq!(ThemeKind::from_key("gruvbox"), Some(ThemeKind::GruvboxDark));
    assert_eq!(ThemeKind::from_key("dracula"), Some(ThemeKind::Dracula));
    assert_eq!(ThemeKind::from_key("rose-pine"), Some(ThemeKind::RosePine));
    assert_eq!(
        ThemeKind::from_key("gruvbox-material"),
        Some(ThemeKind::GruvboxMaterial)
    );
    assert_eq!(
        ThemeKind::from_key("black and white"),
        Some(ThemeKind::BlackWhite)
    );
    assert_eq!(ThemeKind::from_key("none"), Some(ThemeKind::Terminal));
    assert_eq!(ThemeKind::from_key("terminal"), Some(ThemeKind::Terminal));
    assert_eq!(ThemeKind::from_key("nonsense"), None);

    // next() wraps through every theme.
    let n = ThemeKind::all().len();
    assert_eq!(n, 12);
    let mut k = ThemeKind::default();
    let mut seen = vec![k];
    for _ in 0..n {
        k = k.next();
        seen.push(k);
    }
    assert_eq!(seen.first(), seen.last()); // wrapped back to start
}

#[test]
fn load_lists_ignores_a_config_pointer_in_the_data_dir() {
    // Reproduces the crash when the data dir == config dir (~/.config/tudo):
    // config.json must not be parsed as a list.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.json"),
        r#"{ "data_dir": "/somewhere", "theme": "nord" }"#,
    )
    .unwrap();
    let mut list = List::new("Work");
    list.slug = "work".to_string();
    storage::save_list(dir.path(), &list).unwrap();

    let loaded = storage::load_lists(dir.path()).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "Work");
}

#[test]
fn a_list_named_config_does_not_clobber_the_pointer() {
    assert_eq!(storage::slugify("config"), "config-list");
}

#[test]
fn unique_slug_avoids_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = List::new("Work");
    list.slug = storage::unique_slug(dir.path(), "Work");
    assert_eq!(list.slug, "work");
    storage::save_list(dir.path(), &list).unwrap();

    // Same display name -> different slug so the first file isn't clobbered.
    let second = storage::unique_slug(dir.path(), "Work");
    assert_eq!(second, "work-2");
}

// --- notes ------------------------------------------------------------------

#[test]
fn note_frontmatter_round_trips() {
    let mut note = Note::new("API redesign");
    note.slug = "api-redesign".to_string();
    note.body = "First line.\n\nSecond paragraph with a colon: here.".to_string();
    note.tags = vec!["work".to_string(), "backend".to_string()];
    note.pinned = true;
    note.order = Some(3);

    let text = note.to_file_string();
    assert!(text.starts_with("---\n"), "frontmatter fence: {text:?}");
    assert!(text.contains("title: API redesign"));
    assert!(text.contains("tags: [work, backend]"));
    assert!(text.contains("pinned: true"));

    let parsed = Note::from_file(&text, "api-redesign".to_string(), note.created);
    assert_eq!(parsed.title, "API redesign");
    assert_eq!(parsed.body, note.body);
    assert_eq!(parsed.tags, vec!["work", "backend"]);
    assert!(parsed.pinned);
    assert_eq!(parsed.order, Some(3));
}

#[test]
fn note_without_frontmatter_falls_back_to_heading_then_filename() {
    // A plain markdown file (externally created) still loads.
    let from_heading = Note::from_file("# My Heading\n\nbody", "some-file".to_string(), Utc::now());
    assert_eq!(from_heading.title, "My Heading");
    assert_eq!(from_heading.body, "# My Heading\n\nbody");

    let from_slug = Note::from_file(
        "just body, no heading",
        "meeting-notes".to_string(),
        Utc::now(),
    );
    assert_eq!(from_slug.title, "just body, no heading");

    let empty = Note::from_file("", "reading-list".to_string(), Utc::now());
    assert_eq!(empty.title, "reading list");
}

#[test]
fn notebooks_and_notes_round_trip_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    storage::create_notebook_dir(dir.path(), "Work Ideas").unwrap();

    let mut note = Note::new("Sprint plan");
    note.slug = "sprint-plan".to_string();
    note.body = "- ship notes\n- write tests".to_string();
    note.tags = vec!["planning".to_string()];
    storage::save_note(dir.path(), "Work Ideas", &note).unwrap();

    let notebooks = storage::load_notebooks(dir.path()).unwrap();
    assert_eq!(notebooks.len(), 1);
    assert_eq!(notebooks[0].name, "Work Ideas");
    assert_eq!(notebooks[0].slug, "Work Ideas");
    assert_eq!(notebooks[0].notes.len(), 1);
    let loaded = &notebooks[0].notes[0];
    assert_eq!(loaded.title, "Sprint plan");
    assert_eq!(loaded.body, "- ship notes\n- write tests");
    assert_eq!(loaded.tags, vec!["planning"]);
    assert_eq!(loaded.slug, "sprint-plan");
}

#[test]
fn parse_note_title_extracts_hash_tags() {
    let (title, tags) = model::parse_note_title("Meeting notes #work #ideas");
    assert_eq!(title, "Meeting notes");
    assert_eq!(tags, vec!["work", "ideas"]);

    // No `#` tokens -> the whole thing is the title, punctuation intact.
    let (title, tags) = model::parse_note_title("Lunch @ 1pm re: C# rewrite");
    assert_eq!(title, "Lunch @ 1pm re: C# rewrite");
    assert!(tags.is_empty());

    // Duplicates collapse and tags normalize.
    let (_, tags) = model::parse_note_title("x #Work work #work");
    assert_eq!(tags, vec!["work"]);
}

#[test]
fn sanitize_and_unique_notebook_names() {
    let dir = tempfile::tempdir().unwrap();
    // Readable folder names are kept (spaces, case), unsafe chars dropped and
    // any resulting whitespace runs collapsed to a single space.
    assert_eq!(
        storage::sanitize_notebook_name("Work / Ideas"),
        "Work Ideas"
    );
    assert_eq!(
        storage::sanitize_notebook_name("  Trip: 2026  "),
        "Trip 2026"
    );

    storage::create_notebook_dir(dir.path(), "Personal").unwrap();
    // A second notebook with the same name gets a numeric suffix.
    assert_eq!(
        storage::unique_notebook_slug(dir.path(), "Personal"),
        "Personal 2"
    );
}
