//! Generate realistic demo data for screenshots / video recording.
//!
//! Writes a set of lists into a throwaway data dir (default `~/tudo-demo`, or a
//! path given as the first argument), so your real store is untouched.
//!
//!   cargo run --example seed                 # -> ~/tudo-demo
//!   cargo run --example seed -- /tmp/demo    # -> /tmp/demo
//!
//! Then record with:  TUDO_DIR=~/tudo-demo cargo run

use std::path::PathBuf;

use chrono::{Days, Local, NaiveDate};
use tudo::model::{List, Note, Priority, Subtask, Task};
use tudo::storage;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("no home dir").join("tudo-demo"));

    std::fs::create_dir_all(&dir).expect("create demo dir");
    let today = Local::now().date_naive();

    let mut lists = build_lists(today);
    // Pin the sidebar order (Work first) so the demo opens on the richest list.
    for (i, list) in lists.iter_mut().enumerate() {
        list.order = Some(i as u32);
    }
    for list in &lists {
        storage::save_list(&dir, list).expect("save list");
    }

    // Notebooks + notes (the markdown side of the app). Saved in the same
    // on-disk shape the app loads: one folder per notebook, one `.md` per note.
    let notebooks = build_notebooks();
    let mut order = Vec::new();
    for (name, notes) in &notebooks {
        let slug = storage::sanitize_notebook_name(name);
        for note in notes {
            storage::save_note(&dir, &slug, note).expect("save note");
        }
        order.push(slug);
    }
    storage::save_notebook_order(&dir, &order).expect("save notebook order");

    let tasks: usize = lists.iter().map(|l| l.tasks.len()).sum();
    let notes: usize = notebooks.iter().map(|(_, n)| n.len()).sum();
    println!(
        "Seeded {} lists ({tasks} tasks), {} notebooks ({notes} notes) into {}",
        lists.len(),
        notebooks.len(),
        dir.display()
    );
    println!("Record with:  TUDO_DIR={} cargo run", dir.display());
}

fn build_lists(today: NaiveDate) -> Vec<List> {
    let d = |n: i64| Some(today + Days::new(n as u64));
    let ago = |n: u64| Some(today - Days::new(n));

    vec![
        list(
            "Work",
            "work",
            vec![
                task(
                    "Ship v1.0 release",
                    Some(Priority::High),
                    d(2),
                    &["release", "rust"],
                )
                .points(8)
                .note("Cut the 1.0 tag once CI is green and the changelog is final.")
                .subs(&[
                    ("Finalise the changelog", true),
                    ("Tag and build binaries", false),
                    ("Announce on the forum", false),
                ]),
                task(
                    "Fix mouse-scroll jitter on resize",
                    Some(Priority::High),
                    ago(1),
                    &["bug"],
                )
                .points(3)
                .note("Repro: resize the window while scrolled to the bottom of a long list."),
                task(
                    "Write integration tests for storage",
                    Some(Priority::Medium),
                    None,
                    &["tests"],
                )
                .points(5)
                .subs(&[
                    ("JSON round-trip", true),
                    ("Atomic save", true),
                    ("Config resolution", false),
                ]),
                task(
                    "Review PR #42 — theme picker",
                    Some(Priority::Medium),
                    d(0),
                    &["review"],
                )
                .points(2),
                task(
                    "Refactor event handling",
                    Some(Priority::Low),
                    None,
                    &["tech-debt"],
                )
                .points(13)
                .note("Split nav_key into per-mode handlers; it's getting long."),
                task(
                    "Renew the domain + TLS cert",
                    Some(Priority::Medium),
                    d(20),
                    &["admin"],
                )
                .note("Auto-renew is on, but double-check the card on file hasn't expired."),
                done(task("Update the README screenshots", None, None, &["docs"])),
                done(task(
                    "Reply to the user feedback thread",
                    None,
                    ago(2),
                    &["community"],
                )),
            ],
        ),
        list(
            "Personal",
            "personal",
            vec![
                task("Renew passport", Some(Priority::High), d(10), &["admin"])
                    .note("Bring the old passport and two photos. Office opens at 9."),
                task(
                    "Dentist appointment",
                    Some(Priority::Medium),
                    ago(3),
                    &["health"],
                ),
                task("Call mum", None, d(0), &["family"]),
                done(task(
                    "Pay the electricity bill",
                    Some(Priority::High),
                    ago(4),
                    &["bills"],
                )),
                task(
                    "Plan the weekend hike",
                    Some(Priority::Low),
                    d(4),
                    &["outdoors"],
                )
                .subs(&[
                    ("Check the weather", false),
                    ("Pack snacks", false),
                    ("Charge the power bank", false),
                ]),
            ],
        ),
        list(
            "Groceries",
            "groceries",
            vec![
                done(task("Milk", None, None, &[])),
                done(task("Eggs", None, None, &[])),
                task("Coffee beans", None, None, &[]),
                task("Olive oil", None, None, &[]),
                task("Spinach", None, None, &[]),
                done(task("Dark chocolate", None, None, &[])),
                task("Sourdough", None, None, &[]),
            ],
        ),
        list(
            "Trip to Tokyo",
            "trip-to-tokyo",
            vec![
                done(task(
                    "Book flights",
                    Some(Priority::High),
                    ago(5),
                    &["booking"],
                )),
                task(
                    "Reserve a ryokan in Kyoto",
                    Some(Priority::High),
                    d(4),
                    &["booking"],
                )
                .subs(&[("Compare options", true), ("Book two nights", false)]),
                task(
                    "Get the JR Pass",
                    Some(Priority::Medium),
                    d(6),
                    &["transit"],
                ),
                task(
                    "teamLab reservation",
                    Some(Priority::Medium),
                    d(0),
                    &["activity"],
                )
                .note("Tickets drop at 12:00 JST — set an alarm."),
                task("Pack", Some(Priority::Low), d(12), &[]).subs(&[
                    ("Passport", false),
                    ("Power adapters", false),
                    ("Camera + charger", false),
                ]),
                task(
                    "Learn a few basic phrases",
                    Some(Priority::Low),
                    None,
                    &["language"],
                ),
            ],
        ),
        list(
            "Reading",
            "reading",
            vec![
                done(task("The Pragmatic Programmer", None, None, &["tech"]))
                    .note("Re-read the chapter on orthogonality."),
                task(
                    "Crafting Interpreters",
                    Some(Priority::Medium),
                    None,
                    &["rust", "learning"],
                )
                .note("Up to the bytecode VM."),
                task(
                    "Designing Data-Intensive Applications",
                    Some(Priority::Low),
                    d(30),
                    &["reference"],
                ),
                task("Project Hail Mary", None, None, &["fiction"]),
            ],
        ),
    ]
}

// --- notebooks + notes -------------------------------------------------------

/// Curated notebooks for the demo. Returns `(notebook name, notes)` in the
/// sidebar order we want (Project first). Notes wiki-link to each other so the
/// follow-link picker and "Linked from" backlinks have something to show.
fn build_notebooks() -> Vec<(&'static str, Vec<Note>)> {
    vec![
        (
            "Project",
            vec![
                note(
                    "Release plan",
                    &["release", "rust"],
                    true,
                    0,
                    "\
Cut **v1.1.0** once CI is green and the changelog reads well. Tag from `main`, \
build the binaries, then announce.

## Checklist

- [x] Finalise the changelog
- [x] Bump the version in `Cargo.toml`
- [ ] Tag and build the release binaries
- [ ] Re-record the README demo
- [ ] Announce on the forum

## Notes

> Lead the release notes with the two headline features: the Notes tab and the \
global tag view.

Design decisions live in [[Architecture]]; the write-up is in [[Retro - v1.0]].",
                ),
                note(
                    "Architecture",
                    &["rust", "backend"],
                    false,
                    1,
                    "\
The core stays terminal-free so it can be tested without a real TTY.

## Layers

- `model` - plain data types (tasks, notes, lists)
- `storage` - atomic JSON + markdown I/O
- `app` - state and actions
- `ui` - ratatui rendering

Every keypress routes through one action enum, which is what keeps the keymap \
fully configurable.

Back to the plan in [[Release plan]].",
                ),
                note(
                    "Retro - v1.0",
                    &["review"],
                    false,
                    2,
                    "\
What went well, and what to change next time.

## Went well

- Shipped the TUI end to end
- Headless render tests caught real regressions early

## Change next time

- Start the README demo sooner
- Smaller, more frequent releases

Follows up on [[Release plan]].",
                ),
            ],
        ),
        (
            "Trip to Tokyo",
            vec![
                note(
                    "Itinerary",
                    &["travel"],
                    true,
                    0,
                    "\
Two weeks, mostly Tokyo and Kyoto, with a slow first day to beat the jet lag.

## Rough plan

- Days 1-5: Tokyo
- Days 6-9: [[Kyoto]]
- Days 10-14: back to Tokyo

## Todo

- [x] Book the flights
- [ ] Reserve the ryokan
- [ ] Buy the JR Pass",
                ),
                note(
                    "Kyoto",
                    &["travel"],
                    false,
                    1,
                    "\
Temples in the morning, food in the evening.

- Fushimi Inari at sunrise, before the crowds
- Arashiyama bamboo grove
- Nishiki market for lunch

Back to the plan in [[Itinerary]].",
                ),
                note(
                    "Useful phrases",
                    &["language"],
                    false,
                    2,
                    "\
A few basics to learn on the flight.

- konnichiwa - hello
- arigatou - thank you
- sumimasen - excuse me / sorry
- onegaishimasu - please",
                ),
            ],
        ),
        (
            "Journal",
            vec![
                note(
                    "This week",
                    &[],
                    true,
                    0,
                    "\
Shipping the notes feature and re-recording the demo.

## Focus

- Polish the Notes tab
- Re-record the README demo
- Reply to the feedback thread

> Plain markdown, plain files - grep and git still work.",
                ),
                note(
                    "Reading log",
                    &["reading"],
                    false,
                    1,
                    "\
## Now

- *Crafting Interpreters* - up to the bytecode VM
- *Project Hail Mary* - most evenings

## Next

- *Designing Data-Intensive Applications*",
                ),
            ],
        ),
    ]
}

fn note(title: &str, tags: &[&str], pinned: bool, order: u32, body: &str) -> Note {
    let mut n = Note::new(title);
    n.slug = storage::slugify(title);
    n.tags = tags.iter().map(|s| s.to_string()).collect();
    n.pinned = pinned;
    n.order = Some(order);
    n.body = body.to_string();
    n
}

// --- tiny builders ----------------------------------------------------------

fn list(name: &str, slug: &str, tasks: Vec<Task>) -> List {
    let mut l = List::new(name);
    l.slug = slug.to_string();
    l.tasks = tasks;
    l
}

fn task(title: &str, priority: Option<Priority>, due: Option<NaiveDate>, tags: &[&str]) -> Task {
    let mut t = Task::new(title);
    t.priority = priority;
    t.due = due;
    t.tags = tags.iter().map(|s| s.to_string()).collect();
    t
}

fn done(mut t: Task) -> Task {
    t.toggle_done();
    t
}

trait TaskExt {
    fn note(self, notes: &str) -> Self;
    fn points(self, points: u16) -> Self;
    fn subs(self, subs: &[(&str, bool)]) -> Self;
}

impl TaskExt for Task {
    fn note(mut self, notes: &str) -> Self {
        self.notes = notes.to_string();
        self
    }
    fn points(mut self, points: u16) -> Self {
        self.points = Some(points);
        self
    }
    fn subs(mut self, subs: &[(&str, bool)]) -> Self {
        self.subtasks = subs
            .iter()
            .map(|(title, done)| {
                let mut s = Subtask::new(*title);
                s.done = *done;
                s
            })
            .collect();
        self
    }
}
