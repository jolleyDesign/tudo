//! Render a representative tudo screen to HTML using the real theme colours, so
//! the design can be previewed without a terminal. Emits one `<figure>` per
//! built-in theme to stdout.
//!
//! Run: `cargo run --example preview > out.html`

use chrono::Days;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use tudo::app::{App, Focus, Mode};
use tudo::model::{Priority, Subtask};
use tudo::theme::{self, ThemeKind};

// Map a ratatui Color to CSS. ANSI names use a representative dark-terminal
// palette so the "Terminal" theme previews like a typical terminal.
fn hex(c: Color, default: &str) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => default.to_string(),
        Color::Black => "#1e1e1e".into(),
        Color::Red => "#cd3131".into(),
        Color::Green => "#0dbc79".into(),
        Color::Yellow => "#e5e510".into(),
        Color::Blue => "#2472c8".into(),
        Color::Magenta => "#bc3fbc".into(),
        Color::Cyan => "#11a8cd".into(),
        Color::Gray => "#cccccc".into(),
        Color::DarkGray => "#767676".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn to_html(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::from("<pre class=\"term\">");
    for y in 0..area.height {
        for x in 0..area.width {
            let Some(cell) = buf.cell((x, y)) else {
                continue;
            };
            // Reset (Terminal theme) falls back to a typical dark-terminal pair.
            let fg = hex(cell.fg, "#d4d4d4");
            let bg = hex(cell.bg, "#1e1e1e");
            let bold = cell.modifier.contains(Modifier::BOLD);
            let strike = cell.modifier.contains(Modifier::CROSSED_OUT);
            let mut style = format!("color:{fg};background:{bg};");
            if bold {
                style.push_str("font-weight:700;");
            }
            if strike {
                style.push_str("text-decoration:line-through;");
            }
            out.push_str(&format!(
                "<span style=\"{style}\">{}</span>",
                esc(cell.symbol())
            ));
        }
        out.push('\n');
    }
    out.push_str("</pre>");
    out
}

fn main() {
    let dir = std::env::temp_dir().join("tudo-preview-data");
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new(Some(dir)).unwrap();

    app.add_list("Personal".to_string());
    app.add_list("Work".to_string());

    // Select Work (lists are sorted: Personal, Work).
    let work = app.lists.iter().position(|l| l.name == "Work").unwrap();
    app.list_state.select(Some(work));

    app.add_task("Ship the TUI".to_string());
    app.add_task("Write the README".to_string());
    app.add_task("Polish the colours".to_string());
    app.add_task("Pay the invoice".to_string());
    app.add_task("Buy milk".to_string());

    let today = App::today();
    {
        let tasks = &mut app.lists[work].tasks;
        tasks[0].priority = Some(Priority::High);
        tasks[0].due = Some(today + Days::new(3));
        tasks[0].tags = vec!["rust".into()];
        tasks[0].notes = "the big one — ship it this week".into();
        tasks[0].subtasks = vec![
            {
                let mut s = Subtask::new("data model");
                s.done = true;
                s
            },
            Subtask::new("render loop"),
            Subtask::new("polish"),
        ];

        tasks[1].toggle_done();

        tasks[2].priority = Some(Priority::Medium);
        tasks[2].tags = vec!["ui".into()];

        tasks[3].priority = Some(Priority::High);
        tasks[3].due = Some(today - Days::new(1)); // overdue

        // tasks[4] left plain
    }

    // Show the task pane focused with the first task selected.
    app.focus = Focus::Tasks;
    app.task_state.select(Some(0));
    app.mode = Mode::Normal;

    // Render the same screen once per theme.
    for kind in ThemeKind::all() {
        theme::set(kind.theme());
        let backend = TestBackend::new(92, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| tudo::ui::render(f, &mut app)).unwrap();
        println!(
            "<figure class=\"theme\"><figcaption>{}</figcaption>{}</figure>",
            esc(kind.name()),
            to_html(terminal.backend().buffer())
        );
    }
}
