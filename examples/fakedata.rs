//! Generate a *large* fake dataset for exercising a test build: 50 task lists
//! (full of tasks), 50 distinct tags spread across those tasks, 50 notebooks
//! with 50 notes each, plus a loose `Inbox` notebook of 50 more notes.
//!
//! Uses the real `model` + `storage` types, so everything is written in exactly
//! the on-disk shape the app loads (pretty JSON per list, `.md` + YAML
//! frontmatter per note, atomic writes).
//!
//! Writes to a throwaway data dir so your real store is untouched:
//!
//!   cargo run --example fakedata                  # -> ~/tudo-testdata (reset first)
//!   cargo run --example fakedata -- /tmp/tudo-x   # -> given path (not reset)
//!
//! Then launch the test build against it:
//!
//!   TUDO_DIR=~/tudo-testdata cargo run
//!   TUDO_DIR=~/tudo-testdata ./target/release/tudo

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{Days, Duration, Local, NaiveDate, Utc};
use tudo::model::{List, Note, Priority, Subtask, Task};
use tudo::storage;

const LISTS: usize = 50;
const NOTEBOOKS: usize = 50;
const NOTES_PER_NOTEBOOK: usize = 50;
const LOOSE_NOTES: usize = 50;

fn main() {
    // A default arg means "the dedicated testdata dir" — we reset it so re-runs
    // are reproducible. A custom path is left as-is (never blown away).
    let arg = std::env::args().nth(1);
    let dir = arg
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().expect("no home dir").join("tudo-testdata"));

    if arg.is_none() {
        let _ = std::fs::remove_dir_all(&dir);
        println!("Reset {}", dir.display());
    }
    std::fs::create_dir_all(&dir).expect("create data dir");

    let today = Local::now().date_naive();
    let mut rng = Rng::new(0x7d0_5eed_c0ffee);

    // --- lists + tasks (also where the 50 tags come from) --------------------
    let mut tag_cursor = 0usize; // walks TAGS so all 50 get used
    let mut used_slugs: HashSet<String> = HashSet::new();
    let mut total_tasks = 0usize;
    let mut used_tags: HashSet<String> = HashSet::new();

    for (i, name) in LIST_NAMES.iter().take(LISTS).enumerate() {
        let mut list = List::new(*name);
        list.slug = unique_slug(&mut used_slugs, name);
        list.order = Some(i as u32);

        let n_tasks = 4 + rng.below(9); // 4..=12
        for t in 0..n_tasks {
            let task = build_task(
                &mut rng,
                today,
                t,
                n_tasks,
                &mut tag_cursor,
                &mut used_tags,
            );
            list.tasks.push(task);
        }
        total_tasks += list.tasks.len();
        storage::save_list(&dir, &list).expect("save list");
    }

    // Guarantee every one of the 50 tags is actually present somewhere: park any
    // stragglers on a dedicated "Tag Zoo" list so the sidebar shows the full set.
    let missing: Vec<&str> = TAGS
        .iter()
        .copied()
        .filter(|t| !used_tags.contains(*t))
        .collect();
    if !missing.is_empty() {
        let mut zoo = List::new("Tag Zoo");
        zoo.slug = unique_slug(&mut used_slugs, "Tag Zoo");
        zoo.order = Some(LISTS as u32);
        for (k, tag) in missing.iter().enumerate() {
            let mut task = Task::new(format!("Sample task carrying #{tag}"));
            task.tags = vec![(*tag).to_string()];
            task.created = Utc::now() - Duration::days(k as i64);
            zoo.tasks.push(task);
            used_tags.insert((*tag).to_string());
        }
        total_tasks += zoo.tasks.len();
        storage::save_list(&dir, &zoo).expect("save tag-zoo list");
    }

    // --- notebooks + notes ---------------------------------------------------
    let mut order_slugs: Vec<String> = Vec::new();

    // The loose "Inbox" notebook of 50 notes (the "50 notes" in the ask).
    let inbox_slug = "Inbox".to_string();
    write_notebook(&dir, &inbox_slug, LOOSE_NOTES, &mut rng);
    order_slugs.push(inbox_slug);

    // 50 themed notebooks, 50 notes each.
    let mut nb_slugs: HashSet<String> = HashSet::new();
    for name in NOTEBOOK_NAMES.iter().take(NOTEBOOKS) {
        let slug = unique_notebook_slug(&mut nb_slugs, name);
        write_notebook(&dir, &slug, NOTES_PER_NOTEBOOK, &mut rng);
        order_slugs.push(slug);
    }

    // Persist a manual notebook order (Inbox first, then the themed ones).
    storage::save_notebook_order(&dir, &order_slugs).expect("save notebook order");

    let total_notes = LOOSE_NOTES + NOTEBOOKS * NOTES_PER_NOTEBOOK;
    println!(
        "Seeded into {}:\n  {} lists ({} tasks, {} distinct tags)\n  {} notebooks ({} notes: {} loose in Inbox + {}x{})",
        dir.display(),
        LISTS + if missing.is_empty() { 0 } else { 1 },
        total_tasks,
        used_tags.len(),
        NOTEBOOKS + 1,
        total_notes,
        LOOSE_NOTES,
        NOTEBOOKS,
        NOTES_PER_NOTEBOOK,
    );
    println!("\nLaunch the test build against it:");
    println!("  TUDO_DIR={} cargo run", dir.display());
    println!("  TUDO_DIR={} ./target/release/tudo", dir.display());
}

// --- task building ----------------------------------------------------------

fn build_task(
    rng: &mut Rng,
    today: NaiveDate,
    idx: usize,
    total: usize,
    tag_cursor: &mut usize,
    used_tags: &mut HashSet<String>,
) -> Task {
    let verb = TASK_VERBS[rng.below(TASK_VERBS.len())];
    let obj = TASK_OBJECTS[rng.below(TASK_OBJECTS.len())];
    let mut task = Task::new(format!("{verb} {obj}"));

    // Created over the last few months, newest tasks near the top of the list.
    task.created = Utc::now() - Duration::days((idx * 3 + rng.below(20)) as i64);

    // Priority on ~70% of tasks.
    task.priority = match rng.below(10) {
        0..=2 => None,
        3..=4 => Some(Priority::Low),
        5..=7 => Some(Priority::Medium),
        _ => Some(Priority::High),
    };

    // Story points on ~55%, from a fibonacci-ish deck.
    if rng.bool(55) {
        task.points = Some(POINTS[rng.below(POINTS.len())]);
    }

    // Due date on ~55%: a mix of overdue, today, and upcoming.
    if rng.bool(55) {
        task.due = Some(match rng.below(10) {
            0..=2 => today - Days::new(1 + rng.below(14) as u64), // overdue
            3 => today,                                           // due today
            _ => today + Days::new(1 + rng.below(45) as u64),     // upcoming
        });
    }

    // 1..=3 tags, pulled in order so all 50 in TAGS get exercised.
    let n_tags = 1 + rng.below(3);
    for _ in 0..n_tags {
        let tag = TAGS[*tag_cursor % TAGS.len()];
        *tag_cursor += 1;
        if !task.tags.iter().any(|t| t == tag) {
            task.tags.push(tag.to_string());
            used_tags.insert(tag.to_string());
        }
    }

    // A note on ~40%.
    if rng.bool(40) {
        task.notes = TASK_NOTES[rng.below(TASK_NOTES.len())].to_string();
    }

    // Subtasks on ~35%.
    if rng.bool(35) {
        let n = 2 + rng.below(4); // 2..=5
        for _ in 0..n {
            let mut s = Subtask::new(SUBTASKS[rng.below(SUBTASKS.len())]);
            s.done = rng.bool(40);
            task.subtasks.push(s);
        }
    }

    // Mark roughly the back third of a list done (keeps open items up top).
    if idx * 3 >= total * 2 && rng.bool(70) {
        task.done = true;
        task.completed_at = Some(Utc::now() - Duration::days(rng.below(10) as i64));
    }

    task
}

// --- notebook / note building -----------------------------------------------

/// Write `count` notes into notebook `slug`, linking each to the next for
/// backlinks. Titles are unique within the notebook.
fn write_notebook(dir: &std::path::Path, slug: &str, count: usize, rng: &mut Rng) {
    // Pre-pick titles so a note can wiki-link to its neighbour by title.
    let mut titles: Vec<String> = Vec::with_capacity(count);
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..count {
        let base = format!(
            "{} {}",
            NOTE_ADJ[rng.below(NOTE_ADJ.len())],
            NOTE_TOPIC[rng.below(NOTE_TOPIC.len())]
        );
        let mut title = base.clone();
        let mut n = 2;
        while !seen.insert(title.to_lowercase()) {
            title = format!("{base} {n}");
            n += 1;
        }
        let _ = i;
        titles.push(title);
    }

    for i in 0..count {
        let title = &titles[i];
        let link = if count > 1 {
            Some(titles[(i + 1) % count].as_str())
        } else {
            None
        };
        let mut note = Note::new(title.clone());
        note.slug = format!("{}-{:03}", storage::slugify(title), i + 1);
        note.order = Some(i as u32);
        note.created = Utc::now() - Duration::days((count - i) as i64) - Duration::hours(rng.below(24) as i64);
        note.pinned = i < 2 || rng.bool(6); // first couple pinned + a sprinkle
        note.tags = pick_note_tags(rng);
        note.body = note_body(rng, title, link);
        storage::save_note(dir, slug, &note).expect("save note");
    }
}

fn pick_note_tags(rng: &mut Rng) -> Vec<String> {
    let n = rng.below(3); // 0..=2
    let mut out: Vec<String> = Vec::new();
    for _ in 0..n {
        let tag = TAGS[rng.below(TAGS.len())];
        if !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    }
    out
}

fn note_body(rng: &mut Rng, title: &str, link: Option<&str>) -> String {
    let mut b = String::new();
    b.push_str(&format!("# {title}\n\n"));
    b.push_str(NOTE_INTROS[rng.below(NOTE_INTROS.len())]);
    b.push_str(&format!(" Tagged #{}.\n\n", TAGS[rng.below(TAGS.len())]));

    b.push_str("## Key points\n\n");
    let bullets = 2 + rng.below(3);
    for _ in 0..bullets {
        b.push_str(&format!("- {}\n", NOTE_BULLETS[rng.below(NOTE_BULLETS.len())]));
    }
    b.push('\n');

    if rng.bool(55) {
        b.push_str("## Todo\n\n");
        let n = 2 + rng.below(3);
        for _ in 0..n {
            let mark = if rng.bool(35) { "x" } else { " " };
            b.push_str(&format!("- [{mark}] {}\n", NOTE_TODOS[rng.below(NOTE_TODOS.len())]));
        }
        b.push('\n');
    }

    if rng.bool(30) {
        b.push_str("```rust\nfn main() {\n    println!(\"hello from tudo\");\n}\n```\n\n");
    }

    b.push_str("## Notes\n\n");
    b.push_str(NOTE_PARAS[rng.below(NOTE_PARAS.len())]);
    b.push_str("\n\n");

    if let Some(link) = link {
        b.push_str(&format!("See also [[{link}]].\n"));
    }
    b
}

// --- slug helpers ------------------------------------------------------------

fn unique_slug(used: &mut HashSet<String>, name: &str) -> String {
    let base = storage::slugify(name);
    let mut slug = base.clone();
    let mut n = 2;
    while !used.insert(slug.clone()) {
        slug = format!("{base}-{n}");
        n += 1;
    }
    slug
}

fn unique_notebook_slug(used: &mut HashSet<String>, name: &str) -> String {
    let base = storage::sanitize_notebook_name(name);
    let mut slug = base.clone();
    let mut n = 2;
    while !used.insert(slug.clone()) {
        slug = format!("{base} {n}");
        n += 1;
    }
    slug
}

// --- tiny deterministic RNG (no rand crate) ---------------------------------

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9e37_79b9_7f4a_7c15)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    /// 0..n
    fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 33) as usize) % n.max(1)
    }
    /// true `pct`% of the time
    fn bool(&mut self, pct: u64) -> bool {
        (self.next() >> 33) % 100 < pct
    }
}

// --- word banks --------------------------------------------------------------

const TAGS: [&str; 50] = [
    "work", "personal", "urgent", "rust", "ui", "backend", "frontend", "design", "bug", "feature",
    "docs", "tests", "review", "refactor", "research", "meeting", "email", "finance", "health",
    "fitness", "travel", "home", "shopping", "reading", "learning", "ideas", "admin", "legal", "hr",
    "marketing", "sales", "support", "devops", "security", "database", "api", "mobile", "web",
    "planning", "roadmap", "release", "hotfix", "chore", "spike", "blocked", "waiting", "someday",
    "followup", "errand", "family",
];

const LIST_NAMES: [&str; 50] = [
    "Work", "Personal", "Groceries", "Reading List", "Home Improvement", "Fitness Plan",
    "Trip to Tokyo", "Side Project", "Q3 Roadmap", "Bug Triage", "Client: Acme", "Client: Globex",
    "Learning Rust", "Book Club", "Meal Prep", "Garden", "Car Maintenance", "Finances",
    "Tax Season", "Wedding Planning", "Move Apartment", "Job Search", "Open Source", "Blog Posts",
    "Podcast Ideas", "Conference Talk", "Team Standup", "Sprint 12", "Sprint 13", "Backlog",
    "Inbox", "Waiting On", "Someday Maybe", "Health", "Errands", "Gift Ideas", "Recipes",
    "Watchlist", "Music to Learn", "Photography", "Volunteering", "Community", "Kids School",
    "Pet Care", "Renovation", "Investments", "Learning Spanish", "Marathon Training",
    "Startup Ideas", "Weekly Review",
];

const NOTEBOOK_NAMES: [&str; 50] = [
    "Journal", "Meeting Notes", "Project Alpha", "Project Beta", "Research", "Recipes",
    "Travel Log", "Book Notes", "Ideas", "Learning", "Rust Notes", "Design System",
    "Architecture", "Onboarding", "Retrospectives", "One on Ones", "Interviews", "Product Specs",
    "API Docs", "Runbooks", "Postmortems", "Personal Wiki", "Daily Notes", "Weekly Reviews",
    "Reading Highlights", "Quotes", "Dreams", "Gratitude", "Health Log", "Workout Log",
    "Finance Notes", "Home Inventory", "Garden Journal", "Photography Notes", "Music Theory",
    "Language Learning", "Spanish", "History", "Philosophy", "Science", "Startup", "Marketing",
    "Sales Playbook", "Support KB", "Security Notes", "DevOps", "Database Notes", "Frontend",
    "Backend", "Snippets",
];

const POINTS: [u16; 6] = [1, 2, 3, 5, 8, 13];

const TASK_VERBS: [&str; 20] = [
    "Ship", "Fix", "Write", "Review", "Refactor", "Plan", "Draft", "Investigate", "Update",
    "Design", "Test", "Deploy", "Migrate", "Document", "Call", "Email", "Schedule", "Buy",
    "Renew", "Prepare",
];

const TASK_OBJECTS: [&str; 24] = [
    "the release notes", "the login flow", "the onboarding email", "PR #128", "the config parser",
    "the sprint plan", "the pitch deck", "a flaky test", "the dashboard", "the API docs",
    "the landing page", "the staging env", "the database schema", "the runbook", "the client",
    "the vendor", "the standup", "groceries", "the passport", "the demo", "the changelog",
    "the retro", "the budget", "the roadmap",
];

const TASK_NOTES: [&str; 8] = [
    "Blocked on the design review — ping @sam when the mock is ready.",
    "Repro steps are in the ticket; needs a fix before the next release.",
    "Keep this small — timebox to an afternoon.",
    "Follow up after the client call on Thursday.",
    "Depends on the API migration landing first.",
    "Low priority but quick; grab it between meetings.",
    "Double-check the numbers against last quarter.",
    "Draft first, polish later — don't over-engineer.",
];

const SUBTASKS: [&str; 12] = [
    "Outline the approach", "Write the first draft", "Get a review", "Address feedback",
    "Add tests", "Update the docs", "Ship to staging", "Verify in prod", "Send the invite",
    "Book the room", "Prepare slides", "Follow up",
];

const NOTE_ADJ: [&str; 16] = [
    "Weekly", "Quarterly", "Rough", "Detailed", "Quick", "Draft", "Final", "Open", "Kickoff",
    "Retro", "Deep-dive", "Scratch", "Working", "Annotated", "Living", "Shared",
];

const NOTE_TOPIC: [&str; 20] = [
    "meeting recap", "design notes", "research summary", "reading highlights", "planning notes",
    "architecture sketch", "brainstorm", "checklist", "roadmap", "postmortem", "interview notes",
    "spec draft", "how-to", "cheatsheet", "journal entry", "review", "proposal", "outline",
    "reference", "log",
];

const NOTE_INTROS: [&str; 6] = [
    "A short write-up capturing the current thinking and open questions.",
    "Rough notes — expect gaps and half-formed ideas below.",
    "Summary of where things stand and what happens next.",
    "Living document; update as the situation evolves.",
    "Captured during the session — cleaned up afterwards.",
    "Context, decisions, and follow-ups from the discussion.",
];

const NOTE_BULLETS: [&str; 12] = [
    "The main constraint is time, not scope.",
    "Prefer the boring, well-understood option.",
    "Revisit this once the migration lands.",
    "Owner: still unassigned — needs a name.",
    "Risk: the third-party API is rate-limited.",
    "Decision: go with the simpler design for now.",
    "Metric to watch: p95 latency under load.",
    "Dependency on the platform team's work.",
    "Cut scope before cutting quality.",
    "Document the rollback plan before shipping.",
    "Cheapest experiment that would tell us something.",
    "Left as a follow-up for next iteration.",
];

const NOTE_TODOS: [&str; 10] = [
    "Send the summary to the team", "Confirm the numbers", "Book a follow-up", "Draft the RFC",
    "Sketch the diagram", "Ping the stakeholders", "Add to the roadmap", "File the ticket",
    "Update the wiki", "Schedule the review",
];

const NOTE_PARAS: [&str; 6] = [
    "The tricky part is the migration path — we can't take downtime, so it has to be backwards compatible for at least one release. Feature-flag it and roll out gradually.",
    "Most of the value is in the first cut. Get something usable in front of people, then iterate based on what they actually do rather than what they say.",
    "There are three viable options and none is obviously best. Leaning toward the middle one because it's reversible and cheap to try.",
    "Wrote this down mostly so future-me remembers *why* — the what is easy to reconstruct, the reasoning isn't.",
    "Keeping a running log here so the context doesn't live only in chat threads that scroll away.",
    "The interesting question isn't whether it works, but whether it still works at 10x the data. Worth a quick load test.",
];
