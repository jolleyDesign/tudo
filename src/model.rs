//! Core data types for tudo: lists, tasks, subtasks and priorities.
//!
//! These are the on-disk shapes (serde) and carry small bits of pure logic
//! (toggling, priority cycling, due-date parsing) so the rest of the app and
//! the tests can exercise behaviour without a terminal.

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

/// A named list/project. `slug` is the file stem on disk and is not serialized
/// (it comes from the filename when loading).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct List {
    pub name: String,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(skip)]
    pub slug: String,
}

impl List {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
            slug: String::new(),
        }
    }

    /// Count of not-done tasks.
    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
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
