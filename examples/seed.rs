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
use tudo::model::{List, Priority, Subtask, Task};
use tudo::storage;

fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("no home dir").join("tudo-demo"));

    std::fs::create_dir_all(&dir).expect("create demo dir");
    let today = Local::now().date_naive();

    let lists = build_lists(today);
    for list in &lists {
        storage::save_list(&dir, list).expect("save list");
    }

    let tasks: usize = lists.iter().map(|l| l.tasks.len()).sum();
    println!(
        "Seeded {} lists ({tasks} tasks) into {}",
        lists.len(),
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
                .note("Repro: resize the window while scrolled to the bottom of a long list."),
                task(
                    "Write integration tests for storage",
                    Some(Priority::Medium),
                    None,
                    &["tests"],
                )
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
                ),
                task(
                    "Refactor event handling",
                    Some(Priority::Low),
                    None,
                    &["tech-debt"],
                )
                .note("Split nav_key into per-mode handlers; it's getting long."),
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
    fn subs(self, subs: &[(&str, bool)]) -> Self;
}

impl TaskExt for Task {
    fn note(mut self, notes: &str) -> Self {
        self.notes = notes.to_string();
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
