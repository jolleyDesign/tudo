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
    /// Story points: a unitless estimate of size/effort. `None` means unestimated.
    #[serde(default)]
    pub points: Option<u16>,
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
            points: None,
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
    /// Fewest story points first; tasks with no estimate come last.
    Points,
    /// Soonest due date first; tasks with no due date come last.
    Due,
    /// Most recently created first.
    Created,
    /// Alphabetical by title (case-insensitive).
    Alphabetical,
}

impl Sort {
    /// All sorts, in the order they appear in the picker.
    pub fn all() -> [Sort; 6] {
        [
            Sort::Manual,
            Sort::Priority,
            Sort::Points,
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
            Sort::Points => "points",
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
            Sort::Points => "fewest story points first",
            Sort::Due => "soonest due date first",
            Sort::Created => "newest first",
            Sort::Alphabetical => "by title, A to Z",
        }
    }

    /// Order two tasks for this sort in its *ascending* form (see [`SortDir`]).
    /// `Manual` reports every pair equal, so a *stable* sort with it leaves the
    /// stored order untouched — and any ties under the other sorts fall back to
    /// that same stored order.
    pub fn compare(self, a: &Task, b: &Task) -> Ordering {
        match self {
            Sort::Manual => Ordering::Equal,
            Sort::Priority => a.priority_rank().cmp(&b.priority_rank()),
            // `(is_none, points)` keys put estimated tasks (false) before
            // unestimated ones (true), then order two estimates ascending.
            Sort::Points => (a.points.is_none(), a.points).cmp(&(b.points.is_none(), b.points)),
            // `(is_none, due)` keys put dated tasks (false) before undated ones
            // (true), and order two dated tasks by their date ascending.
            Sort::Due => (a.due.is_none(), a.due).cmp(&(b.due.is_none(), b.due)),
            Sort::Created => b.created.cmp(&a.created),
            Sort::Alphabetical => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        }
    }
}

/// Direction a [`Sort`] is applied in. `Ascending` is each sort's natural order
/// as defined by [`Sort::compare`] (highest priority first, soonest due first,
/// fewest points first, …); `Descending` mirrors it. Like [`Sort`] itself this
/// is session-only and resets to the default each run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDir {
    #[default]
    Ascending,
    Descending,
}

impl SortDir {
    /// Flip ascending ↔ descending.
    pub fn toggle(self) -> SortDir {
        match self {
            SortDir::Ascending => SortDir::Descending,
            SortDir::Descending => SortDir::Ascending,
        }
    }

    /// Arrow shown next to the sort in the footer / picker.
    pub fn arrow(self) -> &'static str {
        match self {
            SortDir::Ascending => "\u{2191}",  // ↑
            SortDir::Descending => "\u{2193}", // ↓
        }
    }

    /// Word form for the picker's direction line.
    pub fn label(self) -> &'static str {
        match self {
            SortDir::Ascending => "ascending",
            SortDir::Descending => "descending",
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
    pub points: Option<u16>,
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
    Points(u16),
    Tag,
    Due(NaiveDate),
    Plain,
}

/// Classify one head token. `@` tokens that aren't a valid date, and `~` tokens
/// that aren't a valid number, fall through to [`HeadToken::Plain`] (they stay
/// in the title, so `~/paths` and stray `@`s are safe).
fn classify_head_token(tok: &str, today: NaiveDate) -> HeadToken {
    if !tok.is_empty() && tok.bytes().all(|b| b == b'!') {
        HeadToken::Priority(Priority::from_bang_count(tok.len()))
    } else if tok.starts_with('#') {
        HeadToken::Tag
    } else if let Some(rest) = tok.strip_prefix('~')
        && let Ok(points) = rest.parse::<u16>()
    {
        HeadToken::Points(points)
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
/// - `~N` — story points (`~` + a non-negative integer; last one wins)
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
            HeadToken::Points(n) => parsed.points = Some(n),
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

// --- notes ------------------------------------------------------------------

/// A single markdown note. Stored on disk as one `.md` file with a small YAML
/// frontmatter block carrying its metadata; everything after the frontmatter is
/// the plain-markdown body. `slug` is the file stem and is not part of the
/// frontmatter (it comes from the filename on load), and `modified` is taken
/// from the filesystem so it stays correct even when the file is edited by
/// another tool.
#[derive(Debug, Clone)]
pub struct Note {
    /// Display title (the `title:` field, falling back to the first heading or
    /// the de-slugged filename).
    pub title: String,
    /// Markdown body (everything after the frontmatter).
    pub body: String,
    pub tags: Vec<String>,
    /// Pinned notes float to the top of their notebook regardless of the sort.
    pub pinned: bool,
    /// Manual position within the notebook. `None` sorts after ordered notes.
    pub order: Option<u32>,
    pub created: DateTime<Utc>,
    /// Last-modified time, taken from the filesystem on load.
    pub modified: DateTime<Utc>,
    /// File stem (no `.md`). Not serialized — set from the filename on load.
    pub slug: String,
}

impl Note {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            title: title.into(),
            body: String::new(),
            tags: Vec::new(),
            pinned: false,
            order: None,
            created: now,
            modified: now,
            slug: String::new(),
        }
    }

    /// Word count of the body, for the note-list metadata column.
    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }

    /// Wiki-link targets (`[[note title]]`) referenced in the body, in order,
    /// deduped. Used for following links and building backlinks.
    pub fn link_targets(&self) -> Vec<String> {
        note_link_targets(&self.body)
    }

    /// Substring match across title, body and tags (case-insensitive).
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        self.title.to_lowercase().contains(&q)
            || self.body.to_lowercase().contains(&q)
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    /// Serialize to the on-disk `.md` form: frontmatter block then body. `slug`
    /// and `modified` are intentionally left out (they live in the filename and
    /// the filesystem, respectively).
    pub fn to_file_string(&self) -> String {
        let mut out = String::from("---\n");
        out.push_str(&format!("title: {}\n", self.title));
        if !self.tags.is_empty() {
            out.push_str(&format!("tags: [{}]\n", self.tags.join(", ")));
        }
        if self.pinned {
            out.push_str("pinned: true\n");
        }
        if let Some(order) = self.order {
            out.push_str(&format!("order: {order}\n"));
        }
        out.push_str(&format!("created: {}\n", self.created.to_rfc3339()));
        out.push_str("---\n");
        // Body goes directly after the closing fence so it round-trips exactly
        // (a blank separator line would come back as a leading newline).
        if !self.body.is_empty() {
            out.push_str(&self.body);
            if !self.body.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }

    /// Parse a note from a file's `contents`, its `slug` (file stem) and its
    /// filesystem `modified` time. Lenient: unknown frontmatter keys are ignored,
    /// a missing/broken frontmatter block treats the whole file as body, and a
    /// missing title falls back to the first heading or the de-slugged filename.
    pub fn from_file(contents: &str, slug: String, modified: DateTime<Utc>) -> Note {
        let mut note = Note {
            title: String::new(),
            body: String::new(),
            tags: Vec::new(),
            pinned: false,
            order: None,
            created: modified,
            modified,
            slug: slug.clone(),
        };
        match split_frontmatter(contents) {
            Some((fm, body)) => {
                for line in fm.lines() {
                    let Some((key, value)) = line.split_once(':') else {
                        continue;
                    };
                    let value = value.trim();
                    match key.trim() {
                        "title" => note.title = unquote(value).to_string(),
                        "tags" => {
                            let inner = value.trim_start_matches('[').trim_end_matches(']');
                            note.tags = parse_tags(inner);
                        }
                        "pinned" => note.pinned = value == "true",
                        "order" => note.order = value.parse().ok(),
                        "created" => {
                            if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
                                note.created = dt.with_timezone(&Utc);
                            }
                        }
                        _ => {}
                    }
                }
                note.body = body;
            }
            None => note.body = contents.to_string(),
        }
        if note.title.trim().is_empty() {
            note.title = title_from_body_or_slug(&note.body, &slug);
        }
        note
    }
}

/// Split leading YAML frontmatter (delimited by `---` lines) from the body.
/// Returns `(frontmatter, body)` with line endings normalized to `\n`, or
/// `None` when the content doesn't open with a `---` fence that later closes.
fn split_frontmatter(contents: &str) -> Option<(String, String)> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let mut lines = contents.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut fm = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim_end() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            fm.push_str(line);
            fm.push('\n');
        }
    }
    // No closing fence: not real frontmatter — let the caller keep it as body.
    if !in_body {
        return None;
    }
    if body.ends_with('\n') {
        body.pop();
    }
    Some((fm, body))
}

/// Strip a single pair of matching surrounding quotes, if present.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Derive a title when the frontmatter has none: the first non-empty body line
/// (with any leading `#` markers stripped), else the filename with dashes turned
/// back into spaces.
fn title_from_body_or_slug(body: &str, slug: &str) -> String {
    if let Some(first) = body.lines().find(|l| !l.trim().is_empty()) {
        let heading = first.trim().trim_start_matches('#').trim();
        if !heading.is_empty() {
            return heading.to_string();
        }
    }
    if slug.is_empty() {
        "Untitled".to_string()
    } else {
        slug.replace('-', " ")
    }
}

/// Reserved folder name for the special "Archived" notebook. Its display name is
/// [`ARCHIVE_NOTEBOOK_NAME`]; the underscore-wrapped folder keeps it visually
/// distinct on disk and lets [`Notebook::is_archive`] identify it.
pub const ARCHIVE_NOTEBOOK_SLUG: &str = "__archived__";

/// Display name for the Archived notebook.
pub const ARCHIVE_NOTEBOOK_NAME: &str = "Archived";

/// A named notebook: a folder of notes. Unlike a task [`List`], a notebook has no
/// metadata file of its own — its `slug` is the folder name and doubles as the
/// display `name`, so the `.md` files inside stay the only things on disk.
#[derive(Debug, Clone, Default)]
pub struct Notebook {
    pub name: String,
    pub notes: Vec<Note>,
    /// Folder name on disk. Set from the directory name on load.
    pub slug: String,
    /// Explicit sidebar position. `None` sorts alphabetically after any
    /// explicitly-ordered notebooks. Persisted in `notes/.order`, not per-note.
    pub order: Option<u32>,
}

impl Notebook {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            notes: Vec::new(),
            slug: String::new(),
            order: None,
        }
    }

    /// True for the reserved Archived notebook (identified by its folder name).
    pub fn is_archive(&self) -> bool {
        self.slug == ARCHIVE_NOTEBOOK_SLUG
    }
}

/// Extract `[[wiki link]]` targets from a markdown body, in order of first
/// appearance, deduped and trimmed. Empty `[[]]` and unclosed `[[` are ignored.
pub fn note_link_targets(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            break;
        };
        let target = after[..end].trim();
        if !target.is_empty() && !out.iter().any(|t| t == target) {
            out.push(target.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

/// Parse a new-note title, pulling `#tag` tokens out of it the way the task
/// quick-add does. Returns the cleaned title and the (deduped, normalized) tags.
/// Only `#tag` is recognised — notes have no priority/points/due/subtask markers,
/// so a title like "Meeting @ 3pm" keeps its punctuation.
pub fn parse_note_title(input: &str) -> (String, Vec<String>) {
    let mut title_words: Vec<&str> = Vec::new();
    let mut tag_tokens: Vec<&str> = Vec::new();
    for tok in input.split_whitespace() {
        if tok.starts_with('#') {
            tag_tokens.push(tok);
        } else {
            title_words.push(tok);
        }
    }
    let tags = if tag_tokens.is_empty() {
        Vec::new()
    } else {
        parse_tags(&tag_tokens.join(" "))
    };
    (title_words.join(" "), tags)
}

/// How the visible notes are ordered. Session-only like [`Sort`], defaulting to
/// `Manual` (the stored order set by the reorder keys). Pinned notes float to
/// the top regardless of the active sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteSort {
    /// The user's manual order (reordering keys stay enabled).
    #[default]
    Manual,
    /// Alphabetical by title (case-insensitive).
    Alphabetical,
    /// Most recently created first.
    Created,
    /// Most recently modified first.
    Modified,
}

impl NoteSort {
    /// All sorts, in the order they appear in the picker.
    pub fn all() -> [NoteSort; 4] {
        [
            NoteSort::Manual,
            NoteSort::Alphabetical,
            NoteSort::Created,
            NoteSort::Modified,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            NoteSort::Manual => "manual",
            NoteSort::Alphabetical => "A–Z",
            NoteSort::Created => "created",
            NoteSort::Modified => "modified",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            NoteSort::Manual => "your own order · reordering enabled",
            NoteSort::Alphabetical => "by title, A to Z",
            NoteSort::Created => "newest first",
            NoteSort::Modified => "most recently edited first",
        }
    }

    /// Order two notes for this sort in its ascending form. `Manual` reports
    /// every pair equal so a stable sort leaves the stored order untouched.
    pub fn compare(self, a: &Note, b: &Note) -> Ordering {
        match self {
            NoteSort::Manual => Ordering::Equal,
            NoteSort::Alphabetical => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            NoteSort::Created => b.created.cmp(&a.created),
            NoteSort::Modified => b.modified.cmp(&a.modified),
        }
    }
}

/// A classified slice of a quick-add input string, for live syntax highlighting.
/// See [`quick_add_spans`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddSpan {
    /// Plain text (the title, whitespace, or an unrecognised token).
    Plain,
    /// A `!` / `!!` / `!!!` priority run (carries the level it resolves to).
    Priority(Priority),
    /// A `~N` story-points token.
    Points,
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
                HeadToken::Points(_) => QuickAddSpan::Points,
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
