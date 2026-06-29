//! All rendering. `render` is the single entry point; it draws the themed
//! header / three panes / footer and then any active overlay (input / confirm /
//! help / first-run). Clickable rects are recorded into `app.clickables` for the
//! mouse handler.

use chrono::{Local, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

use crate::app::{App, Focus, Mode};
use crate::model::Task;
use crate::theme;

pub fn render(f: &mut Frame, app: &mut App) {
    // Themed backdrop for the whole screen.
    f.render_widget(
        Block::default().style(Style::default().bg(theme::bg()).fg(theme::fg())),
        f.area(),
    );

    if matches!(app.mode, Mode::FirstRun(_)) {
        render_first_run(f, app);
        return;
    }

    render_base(f, app);

    match &app.mode {
        Mode::Input(_) => render_input(f, app),
        Mode::Confirm(_) => render_confirm(f, app),
        Mode::ThemePicker(_) => render_theme_picker(f, app),
        Mode::Settings => render_settings(f, app),
        Mode::Help => render_help(f, app),
        _ => {}
    }
}

fn render_base(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let [sidebar, right] =
        Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(body);

    let detail = matches!(app.mode, Mode::Detail);
    let (task_pct, detail_pct) = if detail { (50, 50) } else { (66, 34) };
    let [tasks_area, detail_area] = Layout::vertical([
        Constraint::Percentage(task_pct),
        Constraint::Percentage(detail_pct),
    ])
    .areas(right);

    render_header(f, header, app);
    render_sidebar(f, sidebar, app);
    render_tasks(f, tasks_area, app);
    render_detail(f, detail_area, app);
    render_footer(f, footer, app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Block::default().style(Style::default().bg(theme::surface())),
        area,
    );
    let [left, rightside] =
        Layout::horizontal([Constraint::Length(12), Constraint::Min(10)]).areas(area);

    let title = Line::from(vec![
        Span::styled(
            format!(" {} ", theme::DIAMOND),
            Style::default().fg(theme::accent()),
        ),
        Span::styled(
            "tudo",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(title), left);

    let today = App::today();
    let (active, done, overdue) = match app.current_list() {
        Some(l) => {
            let a = l.tasks.iter().filter(|t| !t.done).count();
            let d = l.tasks.len() - a;
            let o = l.tasks.iter().filter(|t| t.is_overdue(today)).count();
            (a, d, o)
        }
        None => (0, 0, 0),
    };

    let mut spans = vec![
        Span::styled(
            today.format("%a %d %b").to_string(),
            Style::default().fg(theme::muted()),
        ),
        Span::raw("    "),
        Span::styled(format!("{active} active"), Style::default().fg(theme::fg())),
        Span::styled(" · ", Style::default().fg(theme::muted())),
        Span::styled(format!("{done} done"), Style::default().fg(theme::green())),
    ];
    if overdue > 0 {
        spans.push(Span::styled(" · ", Style::default().fg(theme::muted())));
        spans.push(Span::styled(
            format!("{} {overdue} overdue", theme::FLAG),
            Style::default()
                .fg(theme::red())
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(" "));
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Right),
        rightside,
    );
}

fn pane_block(title: &str, focused: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::pane_border(focused))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(if focused {
                    theme::accent()
                } else {
                    theme::muted()
                })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
}

fn render_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = matches!(app.mode, Mode::Normal) && app.focus == Focus::Lists;
    let block = pane_block("Lists", focused);
    let inner = block.inner(area);

    let items: Vec<ListItem> = app
        .lists
        .iter()
        .map(|l| {
            let open = l.open_count();
            let total = l.tasks.len();
            ListItem::new(Line::from(vec![
                Span::styled(l.name.clone(), Style::default().fg(theme::fg())),
                Span::styled(
                    format!("  {open}/{total}"),
                    Style::default().fg(theme::muted()),
                ),
            ]))
        })
        .collect();

    if items.is_empty() {
        f.render_widget(block, area);
        f.render_widget(empty_hint("No lists yet.\nPress A to add one."), inner);
        app.clickables.lists_inner = None;
        return;
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selection(focused))
        .highlight_symbol("\u{258e} ");
    f.render_stateful_widget(list, area, &mut app.list_state);
    app.clickables.lists_inner = Some((inner, app.list_state.offset()));

    let total = app.lists.len();
    if total > inner.height as usize {
        render_scrollbar(f, area, total, app.list_state.selected().unwrap_or(0));
    }
}

fn render_tasks(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = matches!(app.mode, Mode::Normal) && app.focus == Focus::Tasks;
    let title = match app.current_list() {
        Some(l) => format!("Tasks · {}", l.name),
        None => "Tasks".to_string(),
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);

    let today = App::today();
    let visible = app.visible_task_indices();
    let items: Vec<ListItem> = visible
        .iter()
        .filter_map(|&i| app.current_list().and_then(|l| l.tasks.get(i)))
        .map(|t| ListItem::new(task_line(t, today)))
        .collect();

    if items.is_empty() {
        f.render_widget(block, area);
        let msg = if app.lists.is_empty() {
            "Press A to create your first list."
        } else if app.filter.is_active() {
            "No tasks match the filter (Esc to clear)."
        } else {
            "No tasks yet. Press a to add one."
        };
        f.render_widget(empty_hint(msg), inner);
        app.clickables.tasks_inner = None;
        return;
    }

    let count = items.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selection(focused))
        .highlight_symbol("\u{258e} ");
    f.render_stateful_widget(list, area, &mut app.task_state);
    app.clickables.tasks_inner = Some((inner, app.task_state.offset()));

    if count > inner.height as usize {
        render_scrollbar(f, area, count, app.task_state.selected().unwrap_or(0));
    }
}

/// Build the single-line representation of a task for the task list.
fn task_line(t: &Task, today: NaiveDate) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();

    // priority dot (aligned slot)
    match t.priority {
        Some(p) => spans.push(Span::styled(
            format!("{} ", theme::DOT),
            Style::default().fg(theme::priority_color(p)),
        )),
        None => spans.push(Span::raw("  ")),
    }

    // checkbox
    spans.push(checkbox_span(t.done));
    spans.push(Span::raw(" "));

    // title
    let title_style = if t.done {
        Style::default()
            .fg(theme::muted())
            .add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default().fg(theme::fg())
    };
    spans.push(Span::styled(t.title.clone(), title_style));

    // subtask progress mini-bar
    if let Some((done, total)) = t.subtask_progress() {
        spans.push(Span::raw("  "));
        spans.extend(progress_spans(done, total, 4));
    }

    // tags
    for tag in &t.tags {
        spans.push(Span::styled(
            format!("  #{tag}"),
            Style::default().fg(theme::purple()),
        ));
    }

    // due date (human-friendly)
    if let Some(due) = t.due {
        spans.push(Span::raw("  "));
        spans.push(due_span(due, today, t.done));
    }

    Line::from(spans)
}

fn render_detail(f: &mut Frame, area: Rect, app: &App) {
    let active = matches!(app.mode, Mode::Detail);
    let block = pane_block("Details", active);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(task) = app.current_task() else {
        f.render_widget(
            empty_hint("Select a task to see details.\nPress Enter to open the detail view."),
            inner,
        );
        return;
    };

    let today = App::today();
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        checkbox_span(task.done),
        Span::raw(" "),
        Span::styled(
            task.title.clone(),
            Style::default()
                .fg(theme::fg())
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // meta line
    let mut meta: Vec<Span> = vec![
        Span::styled("status ", Style::default().fg(theme::muted())),
        Span::styled(
            if task.done { "done" } else { "open" }.to_string(),
            Style::default().fg(if task.done {
                theme::green()
            } else {
                theme::fg()
            }),
        ),
    ];
    if task.done && let Some(at) = task.completed_at {
        meta.push(Span::styled(
            "   completed ",
            Style::default().fg(theme::muted()),
        ));
        meta.push(Span::styled(
            at.with_timezone(&Local)
                .format("%d %b %Y, %H:%M")
                .to_string(),
            Style::default().fg(theme::green()),
        ));
    }
    if let Some(p) = task.priority {
        meta.push(Span::styled(
            "   priority ",
            Style::default().fg(theme::muted()),
        ));
        meta.push(Span::styled(
            format!("{} {}", theme::DOT, p.label()),
            Style::default().fg(theme::priority_color(p)),
        ));
    }
    if let Some(due) = task.due {
        meta.push(Span::styled("   due ", Style::default().fg(theme::muted())));
        meta.push(due_span(due, today, task.done));
    }
    lines.push(Line::from(meta));

    if !task.tags.is_empty() {
        let mut tag_spans = vec![Span::styled("tags ", Style::default().fg(theme::muted()))];
        for tag in &task.tags {
            tag_spans.push(Span::styled(
                format!(" #{tag} "),
                Style::default().fg(theme::purple()),
            ));
        }
        lines.push(Line::from(tag_spans));
    }

    lines.push(Line::raw(""));
    lines.push(section_header("Notes"));
    if task.notes.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "(none — press n to add)",
            Style::default().fg(theme::muted()),
        )));
    } else {
        for line in task.notes.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(theme::fg()),
            )));
        }
    }

    lines.push(Line::raw(""));
    let mut sub_header = section_header("Subtasks").spans;
    if let Some((done, total)) = task.subtask_progress() {
        sub_header.push(Span::raw("  "));
        sub_header.extend(progress_spans(done, total, 8));
        sub_header.push(Span::styled(
            format!("  {done}/{total}"),
            Style::default().fg(theme::muted()),
        ));
    }
    lines.push(Line::from(sub_header));

    if task.subtasks.is_empty() {
        lines.push(Line::from(Span::styled(
            "(none — press s to add)",
            Style::default().fg(theme::muted()),
        )));
    } else {
        let sel = if active {
            app.subtask_state.selected()
        } else {
            None
        };
        for (i, s) in task.subtasks.iter().enumerate() {
            let selected = sel == Some(i);
            let marker = if selected { "\u{258e} " } else { "  " };
            let mut title_style = if s.done {
                Style::default()
                    .fg(theme::muted())
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(theme::fg())
            };
            if selected {
                title_style = title_style.add_modifier(Modifier::BOLD);
            }
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme::accent())),
                checkbox_span(s.done),
                Span::raw(" "),
                Span::styled(s.title.clone(), title_style),
            ]));
        }
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Block::default().style(Style::default().bg(theme::surface())),
        area,
    );

    let chips: &[(&str, &str)] = if matches!(app.mode, Mode::Detail) {
        &[
            ("space", "done"),
            ("e", "dit"),
            ("d", "elete"),
            ("n", "otes"),
            ("Esc", "back"),
            ("?", "help"),
        ]
    } else {
        &[
            ("a", "dd"),
            ("A", "+list"),
            ("space", "done"),
            ("/", "search"),
            ("T", "heme"),
            ("S", "ettings"),
            ("?", "help"),
            ("q", "uit"),
        ]
    };

    let mut right = String::new();
    if app.filter.is_active() {
        right.push_str(&format!("filter:{}", app.filter.status.0.label()));
        if !app.filter.query.is_empty() {
            right.push_str(&format!(" /{}", app.filter.query));
        }
        right.push(' ');
    }
    if !app.status.is_empty() {
        right.push_str(&app.status);
        right.push(' ');
    }

    let right_w = right.chars().count() as u16 + 1;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(right_w)]).areas(area);

    f.render_widget(Paragraph::new(chip_line(chips)), left_area);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            right,
            Style::default().fg(theme::amber()),
        )))
        .alignment(Alignment::Right),
        right_area,
    );
}

// --- overlays ---------------------------------------------------------------

fn render_input(f: &mut Frame, app: &App) {
    let Mode::Input(input) = &app.mode else {
        return;
    };
    let height = if input.multiline { 10 } else { 3 };
    let area = centered_rect(f.area(), 70, height);
    overlay_clear(f, area);
    let block = overlay_block(&input.prompt, theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(
        Paragraph::new(Span::styled(
            input.buffer.clone(),
            Style::default().fg(theme::fg()),
        ))
        .wrap(Wrap { trim: false }),
        inner,
    );

    // Place the real terminal cursor.
    if input.multiline {
        let mut x = inner.x;
        let mut y = inner.y;
        for ch in input.buffer.chars() {
            if ch == '\n' {
                y += 1;
                x = inner.x;
            } else {
                x += 1;
                if x >= inner.x + inner.width {
                    x = inner.x;
                    y += 1;
                }
            }
        }
        f.set_cursor_position((x.min(inner.x + inner.width.saturating_sub(1)), y));
    } else {
        let x = inner.x + input.cursor as u16;
        f.set_cursor_position((x.min(inner.x + inner.width.saturating_sub(1)), inner.y));
    }
}

fn render_confirm(f: &mut Frame, app: &App) {
    let Mode::Confirm(c) = &app.mode else {
        return;
    };
    let area = centered_rect(f.area(), 60, 5);
    overlay_clear(f, area);
    let block = overlay_block("Confirm", theme::red());
    let text = Text::from(vec![
        Line::raw(""),
        Line::from(Span::styled(
            c.prompt.clone(),
            Style::default().fg(theme::fg()),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "y / Enter = yes      any other key = no",
            Style::default().fg(theme::muted()),
        )),
    ]);
    f.render_widget(
        Paragraph::new(text)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_help(f: &mut Frame, _app: &App) {
    let area = centered_rect(f.area(), 64, 24);
    overlay_clear(f, area);
    let block = overlay_block("Keybindings", theme::accent());
    let rows = [
        ("Tab / h l / arrows", "switch focus between panes"),
        ("j k / up down", "move selection"),
        ("space", "toggle task (or subtask) done"),
        ("Enter", "open task detail / drill into list"),
        ("a / A", "add task / add list"),
        ("e", "edit selected title"),
        ("d", "delete selected (with confirm)"),
        ("p", "cycle priority"),
        ("D", "set / clear due date"),
        ("t", "edit tags"),
        ("n", "edit notes"),
        ("s", "add subtask"),
        ("/", "search (title, tags, notes)"),
        ("f", "cycle status filter (all/active/done)"),
        ("T", "open the theme picker"),
        ("S", "settings (paths, data location)"),
        ("Esc", "back / clear filter"),
        ("?", "toggle this help"),
        ("q / Ctrl-C", "quit"),
    ];
    let mut lines: Vec<Line> = Vec::new();
    for (k, v) in rows {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {k:<20}"),
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(v.to_string(), Style::default().fg(theme::fg())),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  press any key to close",
        Style::default().fg(theme::muted()),
    )));
    f.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn render_theme_picker(f: &mut Frame, app: &App) {
    let Mode::ThemePicker(state) = &app.mode else {
        return;
    };
    let all = theme::ThemeKind::all();
    let area = centered_rect(f.area(), 56, all.len() as u16 + 5);
    overlay_clear(f, area);
    let block = overlay_block("Theme", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (i, kind) in all.iter().enumerate() {
        let th = kind.theme();
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        let mut spans = vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{:<24}", kind.name()), name_style),
        ];
        // Per-theme swatch so each row previews its own palette at a glance.
        for color in [th.accent, th.green, th.amber, th.red, th.purple] {
            spans.push(Span::styled(theme::DOT, Style::default().fg(color)));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} preview \u{00b7} Enter apply \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_settings(f: &mut Frame, app: &App) {
    let area = centered_rect(f.area(), 76, 17);
    overlay_clear(f, area);
    let block = overlay_block("Settings", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let label = |s: &str| Span::styled(format!("{s:<14}"), Style::default().fg(theme::muted()));
    let value = |s: String| Span::styled(s, Style::default().fg(theme::fg()));

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            label("Data directory"),
            value(app.data_dir.display().to_string()),
        ]),
        Line::from(vec![label("Config file"), value(app.config_path_display())]),
        Line::from(vec![
            label("Format"),
            value("JSON — one file per list, plus config.json".to_string()),
        ]),
        Line::from(vec![label("Theme"), value(app.theme.name().to_string())]),
        Line::from(vec![
            label("Contents"),
            value(format!(
                "{} lists · {} tasks",
                app.lists.len(),
                app.task_count()
            )),
        ]),
    ];

    // Active environment overrides, if any.
    let overrides: Vec<String> = [
        ("TUDO_DIR", std::env::var("TUDO_DIR").ok()),
        ("TUDO_CONFIG", std::env::var("TUDO_CONFIG").ok()),
        ("TUDO_THEME", std::env::var("TUDO_THEME").ok()),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| format!("{k}={v}")))
    .collect();
    if !overrides.is_empty() {
        lines.push(Line::from(vec![
            label("Overrides"),
            value(overrides.join("  ")),
        ]));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "d",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  change data directory (moves your lists)",
            Style::default().fg(theme::fg()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "t",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  open the theme picker", Style::default().fg(theme::fg())),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Esc / S to close",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_first_run(f: &mut Frame, app: &App) {
    let Mode::FirstRun(fr) = &app.mode else {
        return;
    };
    let area = centered_rect(f.area(), 72, 18);
    overlay_clear(f, area);
    let block = overlay_block("Welcome to tudo", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Where should your todo lists be stored?",
        Style::default()
            .fg(theme::fg())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "j/k to move · Enter to choose",
        Style::default().fg(theme::muted()),
    )));
    lines.push(Line::raw(""));
    for (i, (label, path)) in fr.options.iter().enumerate() {
        let selected = !fr.editing_custom && fr.selected == i;
        lines.push(option_line(
            selected,
            label,
            Some(&path.display().to_string()),
        ));
    }
    let custom_selected = !fr.editing_custom && fr.selected == fr.options.len();
    lines.push(option_line(custom_selected, "Custom path…", None));

    if fr.editing_custom {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("path: ", Style::default().fg(theme::muted())),
            Span::styled(fr.custom.clone(), Style::default().fg(theme::fg())),
            Span::styled("\u{2588}", Style::default().fg(theme::accent())),
        ]));
        lines.push(Line::from(Span::styled(
            "Enter = confirm · Esc = back",
            Style::default().fg(theme::muted()),
        )));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "q / Esc = quit",
            Style::default().fg(theme::muted()),
        )));
    }

    if !app.status.is_empty() {
        lines.push(Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(theme::red()),
        )));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

// --- small builders ---------------------------------------------------------

fn checkbox_span(done: bool) -> Span<'static> {
    if done {
        Span::styled(theme::CHECK_DONE, Style::default().fg(theme::green()))
    } else {
        Span::styled(theme::CHECK_OPEN, Style::default().fg(theme::muted()))
    }
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::accent())
            .add_modifier(Modifier::BOLD),
    ))
}

fn empty_hint(msg: &str) -> Paragraph<'static> {
    Paragraph::new(msg.to_string())
        .style(Style::default().fg(theme::muted()))
        .wrap(Wrap { trim: true })
}

/// A row in the first-run picker.
fn option_line(selected: bool, label: &str, path: Option<&str>) -> Line<'static> {
    let marker = if selected { "\u{258e} " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(theme::accent())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::fg())
    };
    let mut spans = vec![
        Span::styled(marker, Style::default().fg(theme::accent())),
        Span::styled(label.to_string(), label_style),
    ];
    if let Some(p) = path {
        spans.push(Span::styled(
            format!("   {p}"),
            Style::default().fg(theme::muted()),
        ));
    }
    Line::from(spans)
}

/// Build a footer line of `[key]rest` chips.
fn chip_line(chips: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (key, rest) in chips {
        spans.push(Span::styled("[", Style::default().fg(theme::muted())));
        spans.push(Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("]", Style::default().fg(theme::muted())));
        spans.push(Span::styled(
            rest.to_string(),
            Style::default().fg(theme::fg()),
        ));
        spans.push(Span::raw("  "));
    }
    Line::from(spans)
}

/// `done`/`total` rendered as a `▓▓▓░` bar of `width` cells.
fn progress_spans(done: usize, total: usize, width: usize) -> Vec<Span<'static>> {
    // round(done/total * width); checked_div yields None when total == 0
    let filled = ((done * width) + total / 2)
        .checked_div(total)
        .unwrap_or(0)
        .min(width);
    let mut spans = Vec::new();
    if filled > 0 {
        spans.push(Span::styled(
            theme::BAR_FULL.repeat(filled),
            Style::default().fg(theme::green()),
        ));
    }
    if width > filled {
        spans.push(Span::styled(
            theme::BAR_EMPTY.repeat(width - filled),
            Style::default().fg(theme::muted()),
        ));
    }
    spans
}

/// Human-friendly due date, coloured by urgency.
fn due_span(due: NaiveDate, today: NaiveDate, done: bool) -> Span<'static> {
    let days = (due - today).num_days();
    let text = if days < 0 {
        format!("{}d overdue", -days)
    } else if days == 0 {
        "today".to_string()
    } else if days == 1 {
        "tomorrow".to_string()
    } else if days <= 7 {
        format!("in {days}d")
    } else {
        due.format("%d %b").to_string()
    };
    let color = if done {
        theme::muted()
    } else if days < 0 {
        theme::red()
    } else if days <= 1 {
        theme::amber()
    } else {
        theme::teal()
    };
    Span::styled(text, Style::default().fg(color))
}

// --- helpers ----------------------------------------------------------------

fn render_scrollbar(f: &mut Frame, area: Rect, total: usize, position: usize) {
    let track = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let mut state = ScrollbarState::new(total).position(position);
    let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_style(Style::default().fg(theme::accent()))
        .track_style(Style::default().fg(theme::sel_dim()));
    f.render_stateful_widget(sb, track, &mut state);
}

fn overlay_clear(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(Style::default().bg(theme::bg()).fg(theme::fg())),
        area,
    );
}

fn overlay_block(title: &str, accent: ratatui::style::Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(theme::bg()))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
}

/// A rectangle of the given width percentage and fixed height, centered.
fn centered_rect(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = (area.width.saturating_mul(percent_x) / 100).min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}
