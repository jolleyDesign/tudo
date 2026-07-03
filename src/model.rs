//! Core data types for tudo: lists, tasks, subtasks and priorities.
//!
//! These are the on-disk shapes (serde) and carry small bits of pure logic
//! (toggling, priority cycling, due-date parsing) so the rest of the app and
//! the tests can exercise behaviour without a terminal.

use std::cmp::Ordering;

use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Task priority. Serialized in lowercase ("low" / "medium" / "high").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    /// Short label for status/detail display.
    pub fn label(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "med",
            Priority::High => "high",
        }
    }

    /// Single-character marker shown next to a task title.
    pub fn marker(self) -> &'static str {
        match self {
            Priority::Low => "\u{2193}",    // ↓
            Priority::Medium => "\u{2022}", // •
            Priority::High => "\u{2191}",   // ↑
        }
    }

    /// Map a run of `!` marks to a level: 1 → Low, 2 → Medium, 3+ → High.
    /// Used by the quick-add parser (see [`parse_quick_add`]); callers pass a
    /// count of at least one.
    fn from_bang_count(n: usize) -> Priority {
        match n {
            1 => Priority::Low,
            2 => Priority::Medium,
            _ => Priority::High,
        }
    }
}

/// A checkable child of a task. Only one level deep by design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
}

impl Subtask {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            done: false,
        }
    }
}

/// A single todo item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
    #[serde(default)]
    pub priority: Option<Priority>,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            done: false,
            priority: None,
            due: None,
            tags: Vec::new(),
            notes: String::new(),
            subtasks: Vec::new(),
            created: Utc::now(),
            completed_at: None,
        }
    }

    /// Flip done state, stamping/clearing the completion time.
    pub fn toggle_done(&mut self) {
        self.done = !self.done;
        self.completed_at = if self.done { Some(Utc::now()) } else { None };
    }

    /// Advance priority: none -> low -> medium -> high -> none.
    pub fn cycle_priority(&mut self) {
        self.priority = match self.priority {
            None => Some(Priority::Low),
            Some(Priority::Low) => Some(Priority::Medium),
            Some(Priority::Medium) => Some(Priority::High),
            Some(Priority::High) => None,
        };
    }

    /// True when the task has a past due date and is not yet done.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.done && self.due.is_some_and(|d| d < today)
    }

    /// True when the task is due today and not yet done.
    pub fn is_due_today(&self, today: NaiveDate) -> bool {
        !self.done && self.due == Some(today)
    }

    /// Sort rank for the task's priority: High(0) < Medium(1) < Low(2) < None(3),
    /// so ascending order puts the most important tasks first.
    fn priority_rank(&self) -> u8 {
        match self.priority {
            Some(Priority::High) => 0,
            Some(Priority::Medium) => 1,
            Some(Priority::Low) => 2,
            None => 3,
        }
    }

    /// (completed, total) subtasks, or None when there are no subtasks.
    pub fn subtask_progress(&self) -> Option<(usize, usize)> {
        if self.subtasks.is_empty() {
            None
        } else {
            let done = self.subtasks.iter().filter(|s| s.done).count();
            Some((done, self.subtasks.len()))
        }
    }
}

/// How the visible task list is ordered. `Manual` is the user's own order (set
/// by the reorder keys) and is the default; the rest sort by a task field.
///
/// Session-only, like the status/search [`Filter`](crate::app::Filter): it isn't
/// persisted, so each run starts back on `Manual`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// The user's manual order (reordering keys stay enabled).
    #[default]
    Manual,
    /// Highest priority first (high → medium → low → none).
    Priority,
    /// Soonest due date first; tasks with no due date come last.
    Due,
    /// Most recently created first.
    Created,
    /// Alphabetical by title (case-insensitive).
    Alphabetical,
}

impl Sort {
    /// All sorts, in the order they appear in the picker.
    pub fn all() -> [Sort; 5] {
        [
            Sort::Manual,
            Sort::Priority,
            Sort::Due,
            Sort::Created,
            Sort::Alphabetical,
        ]
    }

    /// Short label for the footer indicator and status line.
    pub fn label(self) -> &'static str {
        match self {
            Sort::Manual => "manual",
            Sort::Priority => "priority",
            Sort::Due => "due date",
            Sort::Created => "created",
            Sort::Alphabetical => "A–Z",
        }
    }

    /// One-line explanation shown beside each option in the picker.
    pub fn description(self) -> &'static str {
        match self {
            Sort::Manual => "your own order · reordering enabled",
            Sort::Priority => "highest priority first",
            Sort::Due => "soonest due date first",
            Sort::Created => "newest first",
            Sort::Alphabetical => "by title, A to Z",
        }
    }

    /// Order two tasks for this sort. `Manual` reports every pair equal, so a
    /// *stable* sort with it leaves the stored order untouched — and any ties
    /// under the other sorts fall back to that same stored order.
    pub fn compare(self, a: &Task, b: &Task) -> Ordering {
        match self {
            Sort::Manual => Ordering::Equal,
            Sort::Priority => a.priority_rank().cmp(&b.priority_rank()),
            // `(is_none, due)` keys put dated tasks (false) before undated ones
            // (true), and order two dated tasks by their date ascending.
            Sort::Due => (a.due.is_none(), a.due).cmp(&(b.due.is_none(), b.due)),
            Sort::Created => b.created.cmp(&a.created),
            Sort::Alphabetical => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        }
    }
}

/// Reserved slug (and file stem) for the special "Archived" list. Underscores
/// can never appear in a user slug (see `storage::slugify`), so this can't
/// collide with a list the user creates.
pub const ARCHIVE_SLUG: &str = "__archived__";

/// Display name given to the Archived list when it is first created.
pub const ARCHIVE_NAME: &str = "Archived";

/// A named list/project. `slug` is the file stem on disk and is not serialized
/// (it comes from the filename when loading).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct List {
    pub name: String,
    #[serde(default)]
    pub tasks: Vec<Task>,
    /// Explicit position in the sidebar. `None` means "unordered" and sorts
    /// alphabetically after any explicitly-ordered lists; a fresh install (and
    /// any list never touched by reordering) leaves this out of the JSON, so
    /// list files stay unchanged until the user actually reorders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[serde(skip)]
    pub slug: String,
}

impl List {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            order: None,
            slug: String::new(),
        }
    }

    /// Count of not-done tasks.
    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    /// True for the reserved Archived list (identified by its on-disk slug).
    pub fn is_archive(&self) -> bool {
        self.slug == ARCHIVE_SLUG
    }
}

/// Parse user due-date input. Empty clears (`Ok(None)`). Accepts `YYYY-MM-DD`,
/// `today`, `tomorrow`/`tmr`, and `+N` (N days from today).
pub fn parse_due(input: &str, today: NaiveDate) -> anyhow::Result<Option<NaiveDate>> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return Ok(None);
    }
    let date = match s.as_str() {
        "today" => today,
        "tomorrow" | "tmr" => today + Days::new(1),
        rest if rest.starts_with('+') => {
            let n: u64 = rest[1..].trim().parse()?;
            today + Days::new(n)
        }
        rest => NaiveDate::parse_from_str(rest, "%Y-%m-%d")?,
    };
    Ok(Some(date))
}

/// Split a comma/whitespace separated string into normalized, deduped tags
/// (leading `#` stripped, lowercased, order preserved).
pub fn parse_tags(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in input.split([',', ' ', '\t', '\n']) {
        let tag = raw.trim().trim_start_matches('#').to_lowercase();
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// Fields extracted from a quick-add task string by [`parse_quick_add`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedAdd {
    pub title: String,
    pub priority: Option<Priority>,
    pub due: Option<NaiveDate>,
    pub tags: Vec<String>,
    pub notes: String,
    pub subtasks: Vec<String>,
}

/// How a single whitespace-delimited head token is interpreted. Shared by the
/// parser ([`parse_quick_add`]) and the highlighter ([`quick_add_spans`]) so the
/// two can never disagree about what a token means.
enum HeadToken {
    Priority(Priority),
    Tag,
    Due(NaiveDate),
    Plain,
}

/// Classify one head token. `@` tokens that aren't a valid date fall through to
/// [`HeadToken::Plain`] (they stay in the title).
fn classify_head_token(tok: &str, today: NaiveDate) -> HeadToken {
    if !tok.is_empty() && tok.bytes().all(|b| b == b'!') {
        HeadToken::Priority(Priority::from_bang_count(tok.len()))
    } else if tok.starts_with('#') {
        HeadToken::Tag
    } else if let Some(rest) = tok.strip_prefix('@')
        && !rest.is_empty()
        && let Ok(Some(date)) = parse_due(rest, today)
    {
        HeadToken::Due(date)
    } else {
        HeadToken::Plain
    }
}

/// Parse a quick-add task string into its structured fields.
///
/// The string is a *head* of whitespace-delimited word markers followed by a
/// *tail* of greedy phrase markers; the tail begins at the first `n:`/`s:`
/// token, after which everything is phrase content.
///
/// Word markers (head, any order):
/// - `!` / `!!` / `!!!` — priority low / medium / high (last one wins; 4+ marks
///   clamp to high)
/// - `#tag` — a tag, repeatable (parsed by [`parse_tags`])
/// - `@date` — due date (`@` + anything [`parse_due`] accepts)
///
/// Phrase markers (tail):
/// - `n:…` — notes (repeats are joined with newlines)
/// - `s:a, b` — subtasks (comma-separated; repeats accumulate)
///
/// Markers are only recognised at a word boundary, so "We won!" and "C# work"
/// keep their punctuation. A token starting with `@` that isn't a valid date is
/// left in the title. `today` is only used to resolve relative dates.
pub fn parse_quick_add(input: &str, today: NaiveDate) -> ParsedAdd {
    let (head, tail) = match find_phrase_marker(input, 0) {
        Some(pos) => (&input[..pos], &input[pos..]),
        None => (input, ""),
    };

    let mut parsed = ParsedAdd::default();
    let mut title_words: Vec<&str> = Vec::new();
    let mut tag_tokens: Vec<&str> = Vec::new();

    for tok in head.split_whitespace() {
        match classify_head_token(tok, today) {
            HeadToken::Priority(p) => parsed.priority = Some(p),
            HeadToken::Tag => tag_tokens.push(tok),
            HeadToken::Due(date) => parsed.due = Some(date),
            HeadToken::Plain => title_words.push(tok),
        }
    }

    parsed.title = title_words.join(" ");
    if !tag_tokens.is_empty() {
        parsed.tags = parse_tags(&tag_tokens.join(" "));
    }

    // Tail: walk each `n:`/`s:` segment in order, up to the next marker.
    let mut notes: Vec<&str> = Vec::new();
    let mut pos = 0;
    while pos < tail.len() {
        let kind = tail.as_bytes()[pos]; // marker char: b'n' or b's'
        let content_start = pos + 2; // skip "x:"
        let content_end = find_phrase_marker(tail, content_start).unwrap_or(tail.len());
        let content = tail[content_start..content_end].trim();
        match kind {
            b'n' if !content.is_empty() => notes.push(content),
            b's' => parsed.subtasks.extend(
                content
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            ),
            _ => {}
        }
        pos = content_end;
    }
    parsed.notes = notes.join("\n");

    parsed
}

/// Byte index of the first `n:` / `s:` phrase marker at or after `from`, aligned
/// to a word boundary (start of string or preceded by whitespace). The markers
/// are ASCII, so byte scanning always lands on a char boundary.
fn find_phrase_marker(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i + 1 < bytes.len() {
        let at_boundary = i == 0 || bytes[i - 1].is_ascii_whitespace();
        if at_boundary && (bytes[i] == b'n' || bytes[i] == b's') && bytes[i + 1] == b':' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// A classified slice of a quick-add input string, for live syntax highlighting.
/// See [`quick_add_spans`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddSpan {
    /// Plain text (the title, whitespace, or an unrecognised token).
    Plain,
    /// A `!` / `!!` / `!!!` priority run (carries the level it resolves to).
    Priority(Priority),
    /// A `#tag` token.
    Tag,
    /// A valid `@date` token.
    Due,
    /// A `n:` marker.
    NoteMarker,
    /// The text of a note segment.
    NoteText,
    /// A `s:` marker.
    SubtaskMarker,
    /// The text of a subtask segment.
    SubtaskText,
}

/// Break a quick-add string into classified byte-range spans for highlighting.
///
/// The spans are contiguous and cover the whole input, so concatenating the
/// slices reproduces it exactly — the caller can style each range without ever
/// shifting a character (keeping the input cursor aligned). Classification uses
/// the same rules as [`parse_quick_add`], so what lights up is exactly what will
/// be applied to the task. Zero-length spans are omitted.
pub fn quick_add_spans(input: &str, today: NaiveDate) -> Vec<(usize, usize, QuickAddSpan)> {
    let mut out: Vec<(usize, usize, QuickAddSpan)> = Vec::new();
    let split = find_phrase_marker(input, 0).unwrap_or(input.len());
    let head = &input[..split];

    // Head: walk word/whitespace runs, keeping every character.
    let mut i = 0;
    while i < head.len() {
        let ws = head[i..].starts_with(char::is_whitespace);
        let end = head[i..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace() != ws)
            .map(|(b, _)| i + b)
            .unwrap_or(head.len());
        let kind = if ws {
            QuickAddSpan::Plain
        } else {
            match classify_head_token(&head[i..end], today) {
                HeadToken::Priority(p) => QuickAddSpan::Priority(p),
                HeadToken::Tag => QuickAddSpan::Tag,
                HeadToken::Due(_) => QuickAddSpan::Due,
                HeadToken::Plain => QuickAddSpan::Plain,
            }
        };
        out.push((i, end, kind));
        i = end;
    }

    // Tail: each segment is a two-char marker followed by its text.
    let mut pos = split;
    while pos < input.len() {
        let is_note = input.as_bytes()[pos] == b'n';
        let content_start = pos + 2;
        out.push((
            pos,
            content_start,
            if is_note {
                QuickAddSpan::NoteMarker
            } else {
                QuickAddSpan::SubtaskMarker
            },
        ));
        let content_end = find_phrase_marker(input, content_start).unwrap_or(input.len());
        if content_end > content_start {
            out.push((
                content_start,
                content_end,
                if is_note {
                    QuickAddSpan::NoteText
                } else {
                    QuickAddSpan::SubtaskText
                },
            ));
        }
        pos = content_end;
    }

    out
}
