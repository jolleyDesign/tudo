//! Unit tests for the quick-add parser (`model::parse_quick_add`), driven
//! purely — no `App` or terminal involved.

use chrono::{Days, NaiveDate};
use tudo::model::{Priority, QuickAddSpan, parse_quick_add, quick_add_spans};

/// Fixed "today" so relative dates (`@today`, `@+3`) are deterministic.
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, 2).unwrap()
}

fn parse(input: &str) -> tudo::model::ParsedAdd {
    parse_quick_add(input, today())
}

#[test]
fn plain_title_is_untouched() {
    let p = parse("Buy milk");
    assert_eq!(p.title, "Buy milk");
    assert_eq!(p.priority, None);
    assert_eq!(p.due, None);
    assert!(p.tags.is_empty());
    assert!(p.notes.is_empty());
    assert!(p.subtasks.is_empty());
}

#[test]
fn full_example_with_all_markers() {
    let p = parse(
        "Ship the release !! ~5 #work @tomorrow  n:check the changelog  s:tag it, publish, announce",
    );
    assert_eq!(p.title, "Ship the release");
    assert_eq!(p.priority, Some(Priority::Medium));
    assert_eq!(p.points, Some(5));
    assert_eq!(p.tags, vec!["work"]);
    assert_eq!(p.due, Some(today() + Days::new(1)));
    assert_eq!(p.notes, "check the changelog");
    assert_eq!(p.subtasks, vec!["tag it", "publish", "announce"]);
}

#[test]
fn original_examples_from_the_request() {
    let a = parse("Task title !!! #tag s:subtask one, subtask two");
    assert_eq!(a.title, "Task title");
    assert_eq!(a.priority, Some(Priority::High));
    assert_eq!(a.tags, vec!["tag"]);
    assert_eq!(a.subtasks, vec!["subtask one", "subtask two"]);
    assert!(a.notes.is_empty());

    let b = parse("Task title ! #tag @today n:This is a note within a task. s:a subtask");
    assert_eq!(b.title, "Task title");
    assert_eq!(b.priority, Some(Priority::Low));
    assert_eq!(b.tags, vec!["tag"]);
    assert_eq!(b.due, Some(today()));
    assert_eq!(b.notes, "This is a note within a task.");
    assert_eq!(b.subtasks, vec!["a subtask"]);
}

#[test]
fn priority_levels_and_clamp() {
    assert_eq!(parse("x !").priority, Some(Priority::Low));
    assert_eq!(parse("x !!").priority, Some(Priority::Medium));
    assert_eq!(parse("x !!!").priority, Some(Priority::High));
    // 4+ marks clamp to high.
    assert_eq!(parse("x !!!!!").priority, Some(Priority::High));
    // Last standalone bang token wins.
    assert_eq!(parse("x ! !!!").priority, Some(Priority::High));
}

#[test]
fn story_points_marker() {
    assert_eq!(parse("x ~5").points, Some(5));
    assert_eq!(parse("x ~0").points, Some(0));
    assert_eq!(parse("x ~13").points, Some(13));
    // Last points token wins, matching the priority rule.
    assert_eq!(parse("x ~3 ~8").points, Some(8));
    // A `~` that isn't a whole number stays in the title.
    let p = parse("resize ~/photos ~big");
    assert_eq!(p.title, "resize ~/photos ~big");
    assert_eq!(p.points, None);
}

#[test]
fn bangs_must_be_a_standalone_token() {
    // Trailing punctuation on a word is not a priority marker.
    let p = parse("We won!");
    assert_eq!(p.title, "We won!");
    assert_eq!(p.priority, None);
}

#[test]
fn hash_only_starts_a_tag_at_word_start() {
    // "C#" starts with a letter, so it is not a tag.
    let p = parse("C# rewrite");
    assert_eq!(p.title, "C# rewrite");
    assert!(p.tags.is_empty());
}

#[test]
fn tags_are_lowercased_and_deduped() {
    let p = parse("Plan #Work #work #URGENT");
    assert_eq!(p.title, "Plan");
    assert_eq!(p.tags, vec!["work", "urgent"]);
}

#[test]
fn markers_are_order_independent_in_the_head() {
    let a = parse("#work @today !! Fix bug");
    let b = parse("Fix bug !! #work @today");
    assert_eq!(a.title, "Fix bug");
    assert_eq!(a.priority, Some(Priority::Medium));
    assert_eq!(a.tags, vec!["work"]);
    assert_eq!(a.due, Some(today()));
    assert_eq!(a.priority, b.priority);
    assert_eq!(a.tags, b.tags);
    assert_eq!(a.due, b.due);
}

#[test]
fn date_variants_and_invalid_dates() {
    assert_eq!(parse("x @today").due, Some(today()));
    assert_eq!(parse("x @tomorrow").due, Some(today() + Days::new(1)));
    assert_eq!(parse("x @+3").due, Some(today() + Days::new(3)));
    assert_eq!(
        parse("x @2026-12-31").due,
        Some(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
    );

    // An unparseable @token is left in the title, not silently dropped.
    let p = parse("Feed the @frog");
    assert_eq!(p.title, "Feed the @frog");
    assert_eq!(p.due, None);
}

#[test]
fn notes_keep_commas_but_subtasks_split_on_them() {
    let p = parse("x n:one, two, three");
    assert_eq!(p.notes, "one, two, three");
    assert!(p.subtasks.is_empty());

    let q = parse("x s:one, two, three");
    assert_eq!(q.subtasks, vec!["one", "two", "three"]);
}

#[test]
fn repeated_phrase_markers_combine() {
    // Repeated n: joins with a newline; repeated s: accumulate.
    let p = parse("x n:first n:second s:a s:b, c");
    assert_eq!(p.notes, "first\nsecond");
    assert_eq!(p.subtasks, vec!["a", "b", "c"]);
}

#[test]
fn word_markers_after_a_phrase_marker_are_literal_text() {
    // Once the tail starts, "#tag" and "!!" are just note text.
    let p = parse("Deploy s:build the artifact #ci !!");
    assert_eq!(p.title, "Deploy");
    assert_eq!(p.subtasks, vec!["build the artifact #ci !!"]);
    assert!(p.tags.is_empty());
    assert_eq!(p.priority, None);
}

#[test]
fn phrase_marker_only_matches_at_a_word_boundary() {
    // "asks:foo" has no whitespace before "s:", so it stays in the title.
    let p = parse("She asks:foo");
    assert_eq!(p.title, "She asks:foo");
    assert!(p.subtasks.is_empty());
}

#[test]
fn empty_or_marker_only_input_yields_empty_title() {
    assert_eq!(parse("").title, "");
    assert_eq!(parse("   ").title, "");
    // Only markers, no title words.
    let p = parse("!! #tag @today");
    assert_eq!(p.title, "");
    assert_eq!(p.priority, Some(Priority::Medium));
    assert_eq!(p.tags, vec!["tag"]);
}

#[test]
fn empty_phrase_segments_are_dropped() {
    let p = parse("x s:, , n:");
    assert!(p.subtasks.is_empty());
    assert!(p.notes.is_empty());
}

// --- highlighting spans -----------------------------------------------------

/// The spans must be contiguous and cover the whole input, so the input cursor
/// (which counts characters) stays aligned when the buffer is recoloured.
#[test]
fn spans_are_contiguous_and_cover_the_whole_input() {
    for input in [
        "",
        "Buy milk",
        "Ship it !! ~5 #work @today n:note here s:a, b",
        "resize ~/photos ~big",
        "café ! #naïve @+2 n:später",
        "!! #tag @today",
        "x n:",
    ] {
        let spans = quick_add_spans(input, today());
        let mut cursor = 0;
        let mut rebuilt = String::new();
        for (start, end, _) in &spans {
            assert_eq!(*start, cursor, "gap/overlap in spans for {input:?}");
            assert!(end > start, "empty span emitted for {input:?}");
            rebuilt.push_str(&input[*start..*end]);
            cursor = *end;
        }
        assert_eq!(cursor, input.len(), "spans stop short for {input:?}");
        assert_eq!(rebuilt, input, "spans don't reproduce {input:?}");
    }
}

#[test]
fn spans_classify_each_marker_kind() {
    let input = "Ship it !! ~5 #work @today n:note s:a, b";
    let kinds: Vec<(&str, QuickAddSpan)> = quick_add_spans(input, today())
        .into_iter()
        .map(|(s, e, k)| (&input[s..e], k))
        .collect();

    assert!(kinds.contains(&("!!", QuickAddSpan::Priority(Priority::Medium))));
    assert!(kinds.contains(&("~5", QuickAddSpan::Points)));
    assert!(kinds.contains(&("#work", QuickAddSpan::Tag)));
    assert!(kinds.contains(&("@today", QuickAddSpan::Due)));
    assert!(kinds.contains(&("n:", QuickAddSpan::NoteMarker)));
    assert!(kinds.contains(&("note ", QuickAddSpan::NoteText)));
    assert!(kinds.contains(&("s:", QuickAddSpan::SubtaskMarker)));
    assert!(kinds.contains(&("a, b", QuickAddSpan::SubtaskText)));
    // "Ship", "it" and the spaces between them are plain title text.
    assert!(kinds.contains(&("Ship", QuickAddSpan::Plain)));
}

#[test]
fn invalid_date_token_is_not_highlighted() {
    let input = "Feed the @frog";
    let due_spans = quick_add_spans(input, today())
        .into_iter()
        .filter(|(_, _, k)| *k == QuickAddSpan::Due)
        .count();
    assert_eq!(due_spans, 0);
}
