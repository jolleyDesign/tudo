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
    Block, BorderType, Clear, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Wrap,
};

use crate::app::{App, Focus, InputField, Mode, NoteView, NotesFocus, Tab, View};
use crate::keybind::Action;
use crate::model::{Note, NoteSort, QuickAddSpan, Sort, Task, quick_add_spans};
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

    // The rendered read view needs &mut App (to clamp scroll and record link
    // rects), so it's handled before the immutable-borrow match below.
    if matches!(app.mode, Mode::NoteView(_)) {
        render_note_view(f, app);
        return;
    }
    // The move picker likewise needs &mut App to clamp its scroll offset.
    if matches!(app.mode, Mode::MovePicker(_)) {
        render_move_picker(f, app);
        return;
    }

    match &app.mode {
        Mode::Input(_) => render_input(f, app),
        Mode::Confirm(_) => render_confirm(f, app),
        Mode::ThemePicker(_) => render_theme_picker(f, app),
        Mode::CopyMenu(_) => render_copy_menu(f, app),
        Mode::SortPicker(_) => render_sort_picker(f, app),
        Mode::NoteSortPicker(_) => render_note_sort_picker(f, app),
        Mode::NoteMovePicker(_) => render_note_move_picker(f, app),
        Mode::NoteCopyMenu(_) => render_note_copy_menu(f, app),
        Mode::NoteLinkPicker(_) => render_note_link_picker(f, app),
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

    render_header(f, header, app);
    match app.active_tab {
        Tab::Tasks => render_tasks_body(f, body, app),
        Tab::Notes => render_notes_body(f, body, app),
    }
    render_footer(f, footer, app);
}

fn render_tasks_body(f: &mut Frame, body: Rect, app: &mut App) {
    let [sidebar, right] =
        Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(body);

    let detail = matches!(app.mode, Mode::Detail);
    let (task_pct, detail_pct) = if detail { (50, 50) } else { (66, 34) };
    let [tasks_area, detail_area] = Layout::vertical([
        Constraint::Percentage(task_pct),
        Constraint::Percentage(detail_pct),
    ])
    .areas(right);

    render_sidebar(f, sidebar, app);
    render_tasks(f, tasks_area, app);
    render_detail(f, detail_area, app);
}

fn render_notes_body(f: &mut Frame, body: Rect, app: &mut App) {
    // Sidebar of notebooks, note list, and a live preview of the selection.
    // Opening a note (Enter) drops into the full-screen body editor, which is
    // drawn as an overlay on top of this by `render_note_body_editor`.
    let [sidebar, right] =
        Layout::horizontal([Constraint::Length(26), Constraint::Min(20)]).areas(body);
    let [list_area, preview_area] =
        Layout::vertical([Constraint::Percentage(66), Constraint::Percentage(34)]).areas(right);

    render_notes_sidebar(f, sidebar, app);
    render_note_list(f, list_area, app);
    render_note_preview(f, preview_area, app);
}

fn render_header(f: &mut Frame, area: Rect, app: &mut App) {
    f.render_widget(
        Block::default().style(Style::default().bg(theme::surface())),
        area,
    );

    let right = Line::from(header_stats_spans(app)).alignment(Alignment::Right);
    let right_w = right.width() as u16 + 1;
    let [left, rightside] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(right_w)]).areas(area);

    // Title, then the tab strip. We track the running column so each tab chip's
    // rect can be recorded for click-to-switch.
    let mut left_spans = vec![
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
        Span::raw("   "),
    ];
    let prefix_w: u16 = left_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();
    let mut x = left.x + prefix_w;
    let mut tab_rects = Vec::new();
    for tab in Tab::all() {
        let active = tab == app.active_tab;
        let n = match tab {
            Tab::Tasks => 1,
            Tab::Notes => 2,
        };
        let style = if active {
            Style::default()
                .fg(theme::bg())
                .bg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::muted())
        };
        let label = format!(" {n} {} ", tab.label());
        let w = label.chars().count() as u16;
        tab_rects.push((
            tab,
            Rect {
                x,
                y: area.y,
                width: w,
                height: 1,
            },
        ));
        x += w + 1; // + the separator space below
        left_spans.push(Span::styled(label, style));
        left_spans.push(Span::raw(" "));
    }
    app.clickables.tab_rects = tab_rects;

    f.render_widget(Paragraph::new(Line::from(left_spans)), left);
    f.render_widget(Paragraph::new(right), rightside);
}

/// Right-hand header summary for the active tab.
fn header_stats_spans(app: &App) -> Vec<Span<'static>> {
    let today = App::today();
    let mut spans = vec![
        Span::styled(
            today.format("%a %d %b").to_string(),
            Style::default().fg(theme::muted()),
        ),
        Span::raw("    "),
    ];
    match app.active_tab {
        Tab::Tasks => {
            let (active, done, overdue) = app.header_stats(today);
            spans.push(Span::styled(
                format!("{active} active"),
                Style::default().fg(theme::fg()),
            ));
            spans.push(Span::styled(" · ", Style::default().fg(theme::muted())));
            spans.push(Span::styled(
                format!("{done} done"),
                Style::default().fg(theme::green()),
            ));
            if overdue > 0 {
                spans.push(Span::styled(" · ", Style::default().fg(theme::muted())));
                spans.push(Span::styled(
                    format!("{} {overdue} overdue", theme::FLAG),
                    Style::default()
                        .fg(theme::red())
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }
        Tab::Notes => {
            spans.push(Span::styled(
                format!("{} notebooks", app.notebooks.len()),
                Style::default().fg(theme::fg()),
            ));
            spans.push(Span::styled(" · ", Style::default().fg(theme::muted())));
            spans.push(Span::styled(
                format!("{} notes", app.total_note_count()),
                Style::default().fg(theme::fg()),
            ));
        }
    }
    spans.push(Span::raw(" "));
    spans
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
    let sidebar_focused = matches!(app.mode, Mode::Normal) && app.focus == Focus::Lists;
    let selected = app.list_state.selected();
    let n_lists = app.lists.len();
    let tags = app.all_tags();

    // Lists pane items (always present when there are any lists).
    let list_items: Vec<ListItem> = app
        .lists
        .iter()
        .map(|l| {
            let open = l.open_count();
            let total = l.tasks.len();
            // The Archived list sits at the bottom, rendered dimmer than the rest.
            let name_fg = if l.is_archive() {
                theme::muted()
            } else {
                theme::fg()
            };
            ListItem::new(Line::from(vec![
                Span::styled(l.name.clone(), Style::default().fg(name_fg)),
                Span::styled(
                    format!("  {open}/{total}"),
                    Style::default().fg(theme::muted()),
                ),
            ]))
        })
        .collect();

    // No tags: one full-height Lists pane (the original layout).
    if tags.is_empty() {
        if list_items.is_empty() {
            let block = pane_block("Lists", sidebar_focused);
            let inner = block.inner(area);
            f.render_widget(block, area);
            f.render_widget(empty_hint("No lists yet.\nPress A to add one."), inner);
            app.clickables.lists_inner = None;
            app.clickables.tags_inner = None;
            return;
        }
        let mut state = ListState::default().with_selected(selected);
        *state.offset_mut() = prev_offset(app.clickables.lists_inner);
        let (inner, offset) =
            render_sidebar_pane(f, area, "Lists", list_items, sidebar_focused, &mut state);
        app.clickables.lists_inner = Some((inner, offset));
        app.clickables.tags_inner = None;
        return;
    }

    // With tags present, split the column: Lists on top (priority — it keeps the
    // remainder, always at least two thirds), Tags pinned to the bottom, capped
    // at a third of the height and only as tall as the tags need. Overflowing
    // tags scroll inside their own pane, so they can never take over the sidebar.
    let cap = area.height / 3;
    let tags_needed = (tags.len() as u16).saturating_add(2); // + top/bottom border
    let tags_h = tags_needed.min(cap).max(3);
    let [lists_area, tags_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(tags_h)]).areas(area);

    // Which pane holds the cursor decides which one shows the active (accent)
    // border and highlight; the other stays dim even while the sidebar is focused.
    let sel_in_lists = selected.is_some_and(|i| i < n_lists);

    let mut lists_state = ListState::default().with_selected(selected.filter(|&i| i < n_lists));
    *lists_state.offset_mut() = prev_offset(app.clickables.lists_inner);
    let (lists_inner, lists_off) = render_sidebar_pane(
        f,
        lists_area,
        "Lists",
        list_items,
        sidebar_focused && sel_in_lists,
        &mut lists_state,
    );

    let tag_items: Vec<ListItem> = tags
        .iter()
        .map(|name| {
            let (open, total) = app.tag_counts(name);
            ListItem::new(Line::from(vec![
                Span::styled(format!("#{name}"), Style::default().fg(theme::purple())),
                Span::styled(
                    format!("  {open}/{total}"),
                    Style::default().fg(theme::muted()),
                ),
            ]))
        })
        .collect();
    let mut tags_state =
        ListState::default().with_selected(selected.and_then(|i| i.checked_sub(n_lists)));
    *tags_state.offset_mut() = prev_offset(app.clickables.tags_inner);
    let (tags_inner, tags_off) = render_sidebar_pane(
        f,
        tags_area,
        "Tags",
        tag_items,
        sidebar_focused && !sel_in_lists,
        &mut tags_state,
    );

    app.clickables.lists_inner = Some((lists_inner, lists_off));
    app.clickables.tags_inner = Some((tags_inner, tags_off));
}

/// The scroll offset a pane was last drawn with, so it holds its place between
/// frames (ratatui still nudges it to keep the selection visible).
fn prev_offset(slot: Option<(Rect, usize)>) -> usize {
    slot.map(|(_, off)| off).unwrap_or(0)
}

/// Render one titled sidebar pane (a scrollable list) into `area`, returning its
/// inner rect and final scroll offset for mouse hit-testing. Draws a scrollbar
/// when the content overflows.
fn render_sidebar_pane(
    f: &mut Frame,
    area: Rect,
    title: &str,
    items: Vec<ListItem<'static>>,
    focused: bool,
    state: &mut ListState,
) -> (Rect, usize) {
    let block = pane_block(title, focused);
    let inner = block.inner(area);
    let total = items.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selection(focused))
        .highlight_symbol("\u{258e} ");
    f.render_stateful_widget(list, area, state);
    if total > inner.height as usize {
        let pos = state.selected().unwrap_or_else(|| state.offset());
        render_scrollbar(f, area, total, pos);
    }
    (inner, state.offset())
}

fn render_tasks(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = matches!(app.mode, Mode::Normal) && app.focus == Focus::Tasks;
    // The tag view spans lists, so its title says so and each row is tagged with
    // its home list.
    let (title, tag_view) = match app.current_view() {
        Some(View::List(li)) => (
            format!(
                "Tasks · {}",
                app.lists.get(li).map(|l| l.name.as_str()).unwrap_or("")
            ),
            false,
        ),
        Some(View::Tag(tag)) => (format!("Tag · #{tag} (all lists)"), true),
        None => ("Tasks".to_string(), false),
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);

    let today = App::today();
    let refs = app.visible_task_refs();
    let items: Vec<ListItem> = refs
        .iter()
        .filter_map(|r| {
            let list = app.lists.get(r.list)?;
            let t = list.tasks.get(r.task)?;
            let mut line = task_line(t, today);
            if tag_view {
                line.spans.push(Span::styled(
                    format!("  · {}", list.name),
                    Style::default().fg(theme::muted()),
                ));
            }
            Some(ListItem::new(line))
        })
        .collect();

    if items.is_empty() {
        f.render_widget(block, area);
        let msg = if app.lists.is_empty() {
            "Press A to create your first list."
        } else if tag_view {
            "No tasks with this tag match the filter."
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

    // story points
    if let Some(points) = t.points {
        spans.push(Span::styled(
            format!("  {points}pt"),
            Style::default().fg(theme::accent()),
        ));
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
    if task.done
        && let Some(at) = task.completed_at
    {
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
    if let Some(points) = task.points {
        meta.push(Span::styled(
            "   points ",
            Style::default().fg(theme::muted()),
        ));
        meta.push(Span::styled(
            points.to_string(),
            Style::default().fg(theme::accent()),
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

// --- Notes tab ---------------------------------------------------------------

/// Marker shown before a pinned note.
const PIN: &str = "\u{2605}"; // ★

fn render_notes_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = matches!(app.mode, Mode::Normal) && app.notes_focus == NotesFocus::Notebooks;
    let selected = app.notebook_state.selected();
    let n_notebooks = app.notebooks.len();
    let tags = app.all_note_tags();

    let notebook_items: Vec<ListItem> = app
        .notebooks
        .iter()
        .map(|nb| {
            // The Archived notebook sits at the bottom, dimmer than the rest.
            let name_fg = if nb.is_archive() {
                theme::muted()
            } else {
                theme::fg()
            };
            ListItem::new(Line::from(vec![
                Span::styled(nb.name.clone(), Style::default().fg(name_fg)),
                Span::styled(
                    format!("  {}", nb.notes.len()),
                    Style::default().fg(theme::muted()),
                ),
            ]))
        })
        .collect();

    // No tags: one full-height Notebooks pane.
    if tags.is_empty() {
        if notebook_items.is_empty() {
            let block = pane_block("Notebooks", focused);
            let inner = block.inner(area);
            f.render_widget(block, area);
            f.render_widget(empty_hint("No notebooks yet.\nPress A to add one."), inner);
            app.clickables.notebooks_inner = None;
            app.clickables.note_tags_inner = None;
            return;
        }
        let mut state = ListState::default().with_selected(selected);
        *state.offset_mut() = prev_offset(app.clickables.notebooks_inner);
        let (inner, offset) =
            render_sidebar_pane(f, area, "Notebooks", notebook_items, focused, &mut state);
        app.clickables.notebooks_inner = Some((inner, offset));
        app.clickables.note_tags_inner = None;
        return;
    }

    // With tags present, split the column just like the Tasks sidebar.
    let cap = area.height / 3;
    let tags_needed = (tags.len() as u16).saturating_add(2);
    let tags_h = tags_needed.min(cap).max(3);
    let [nb_area, tags_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(tags_h)]).areas(area);

    let sel_in_nb = selected.is_some_and(|i| i < n_notebooks);

    let mut nb_state = ListState::default().with_selected(selected.filter(|&i| i < n_notebooks));
    *nb_state.offset_mut() = prev_offset(app.clickables.notebooks_inner);
    let (nb_inner, nb_off) = render_sidebar_pane(
        f,
        nb_area,
        "Notebooks",
        notebook_items,
        focused && sel_in_nb,
        &mut nb_state,
    );

    let tag_items: Vec<ListItem> = tags
        .iter()
        .map(|name| {
            let count = app.note_tag_count(name);
            ListItem::new(Line::from(vec![
                Span::styled(format!("#{name}"), Style::default().fg(theme::purple())),
                Span::styled(format!("  {count}"), Style::default().fg(theme::muted())),
            ]))
        })
        .collect();
    let mut tags_state =
        ListState::default().with_selected(selected.and_then(|i| i.checked_sub(n_notebooks)));
    *tags_state.offset_mut() = prev_offset(app.clickables.note_tags_inner);
    let (tags_inner, tags_off) = render_sidebar_pane(
        f,
        tags_area,
        "Tags",
        tag_items,
        focused && !sel_in_nb,
        &mut tags_state,
    );

    app.clickables.notebooks_inner = Some((nb_inner, nb_off));
    app.clickables.note_tags_inner = Some((tags_inner, tags_off));
}

fn render_note_list(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = matches!(app.mode, Mode::Normal) && app.notes_focus == NotesFocus::Notes;
    let (title, tag_view) = match app.current_note_view() {
        Some(NoteView::Notebook(ni)) => (
            format!(
                "Notes · {}",
                app.notebooks.get(ni).map(|n| n.name.as_str()).unwrap_or("")
            ),
            false,
        ),
        Some(NoteView::Tag(tag)) => (format!("Tag · #{tag} (all notebooks)"), true),
        None => ("Notes".to_string(), false),
    };
    let block = pane_block(&title, focused);
    let inner = block.inner(area);

    let refs = app.visible_note_refs();
    let items: Vec<ListItem> = refs
        .iter()
        .filter_map(|r| {
            let nb = app.notebooks.get(r.notebook)?;
            let note = nb.notes.get(r.note)?;
            let mut line = note_line(note);
            if tag_view {
                line.spans.push(Span::styled(
                    format!("  · {}", nb.name),
                    Style::default().fg(theme::muted()),
                ));
            }
            Some(ListItem::new(line))
        })
        .collect();

    if items.is_empty() {
        f.render_widget(block, area);
        let msg = if app.notebooks.is_empty() {
            "Press A to create your first notebook."
        } else if tag_view {
            "No notes with this tag match the search."
        } else if app.note_filter_active() {
            "No notes match the search (Esc to clear)."
        } else {
            "No notes yet. Press a to add one."
        };
        f.render_widget(empty_hint(msg), inner);
        app.clickables.note_list_inner = None;
        return;
    }

    let count = items.len();
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selection(focused))
        .highlight_symbol("\u{258e} ");
    f.render_stateful_widget(list, area, &mut app.note_state);
    app.clickables.note_list_inner = Some((inner, app.note_state.offset()));

    if count > inner.height as usize {
        render_scrollbar(f, area, count, app.note_state.selected().unwrap_or(0));
    }
}

/// Single-line representation of a note for the note list.
fn note_line(note: &Note) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    if note.pinned {
        spans.push(Span::styled(
            format!("{PIN} "),
            Style::default().fg(theme::amber()),
        ));
    } else {
        spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(
        note.title.clone(),
        Style::default().fg(theme::fg()),
    ));
    for tag in &note.tags {
        spans.push(Span::styled(
            format!("  #{tag}"),
            Style::default().fg(theme::purple()),
        ));
    }
    spans.push(Span::styled(
        format!("  {}", note.modified.with_timezone(&Local).format("%d %b")),
        Style::default().fg(theme::muted()),
    ));
    spans.push(Span::styled(
        format!("  {}w", note.word_count()),
        Style::default().fg(theme::muted()),
    ));
    Line::from(spans)
}

fn render_note_preview(f: &mut Frame, area: Rect, app: &App) {
    let block = pane_block("Preview", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(note) = app.current_note() else {
        f.render_widget(
            empty_hint("Select a note to preview.\nPress Enter to open it full-screen."),
            inner,
        );
        return;
    };
    f.render_widget(
        Paragraph::new(Text::from(note_detail_lines(note))).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Title + metadata + body lines shown in the preview pane.
fn note_detail_lines(note: &Note) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        note.title.clone(),
        Style::default()
            .fg(theme::fg())
            .add_modifier(Modifier::BOLD),
    )));

    let mut meta = vec![
        Span::styled("modified ", Style::default().fg(theme::muted())),
        Span::styled(
            note.modified
                .with_timezone(&Local)
                .format("%d %b %Y")
                .to_string(),
            Style::default().fg(theme::fg()),
        ),
        Span::styled("   words ", Style::default().fg(theme::muted())),
        Span::styled(
            note.word_count().to_string(),
            Style::default().fg(theme::fg()),
        ),
    ];
    if note.pinned {
        meta.push(Span::styled(
            format!("   {PIN} pinned"),
            Style::default().fg(theme::amber()),
        ));
    }
    lines.push(Line::from(meta));

    if !note.tags.is_empty() {
        let mut tag_spans = vec![Span::styled("tags ", Style::default().fg(theme::muted()))];
        for tag in &note.tags {
            tag_spans.push(Span::styled(
                format!(" #{tag} "),
                Style::default().fg(theme::purple()),
            ));
        }
        lines.push(Line::from(tag_spans));
    }

    lines.push(Line::raw(""));
    if note.body.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty — press Enter to write, E for $EDITOR)",
            Style::default().fg(theme::muted()),
        )));
    } else {
        for line in note.body.lines() {
            lines.push(markdown_line(line));
        }
    }
    lines
}

/// Style one line of markdown so the *markup* recedes: leading heading / list /
/// quote / fence markers and inline `` ` `` / `**` delimiters render dimmed,
/// while the text they wrap stays bright (headings in accent-bold). The
/// concatenated span text equals `line` exactly, so this is safe to use live in
/// the editor — every character is emitted once, so the cursor stays aligned.
fn markdown_line(line: &str) -> Line<'static> {
    let muted = Style::default().fg(theme::muted());
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let mut spans: Vec<Span> = Vec::new();
    if !indent.is_empty() {
        spans.push(Span::raw(indent.to_string()));
    }

    // Fenced code fence: the whole line is markup.
    if rest.starts_with("```") || rest.starts_with("~~~") {
        spans.push(Span::styled(rest.to_string(), muted));
        return Line::from(spans);
    }

    // ATX heading: 1-6 '#' then a space.
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && rest[hashes..].starts_with(' ') {
        let marker_len = hashes + leading_spaces(&rest[hashes..]);
        let (marker, content) = rest.split_at(marker_len);
        spans.push(Span::styled(marker.to_string(), muted));
        spans.push(Span::styled(content.to_string(), heading_style()));
        return Line::from(spans);
    }

    // Blockquote: '>' then optional space.
    if let Some(after) = rest.strip_prefix('>') {
        let (marker, content) = rest.split_at(1 + leading_spaces(after));
        spans.push(Span::styled(marker.to_string(), muted));
        spans.extend(inline_spans(content));
        return Line::from(spans);
    }

    // Unordered list: '-', '*', '+' then a space.
    if matches!(rest.chars().next(), Some('-' | '*' | '+')) && rest[1..].starts_with(' ') {
        let (marker, content) = rest.split_at(1 + leading_spaces(&rest[1..]));
        spans.push(Span::styled(marker.to_string(), muted));
        spans.extend(inline_spans(content));
        return Line::from(spans);
    }

    // Ordered list: digits then '.' or ')' then a space.
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let after = &rest[digits..];
        if matches!(after.chars().next(), Some('.' | ')')) && after[1..].starts_with(' ') {
            let (marker, content) = rest.split_at(digits + 1 + leading_spaces(&after[1..]));
            spans.push(Span::styled(marker.to_string(), muted));
            spans.extend(inline_spans(content));
            return Line::from(spans);
        }
    }

    spans.extend(inline_spans(rest));
    Line::from(spans)
}

/// Byte length of the run of leading spaces in `s`.
fn leading_spaces(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

/// Dim inline `` `code` `` and `**bold**` delimiters, keeping the wrapped text
/// bright. Every character of `text` is emitted exactly once.
fn inline_spans(text: &str) -> Vec<Span<'static>> {
    let muted = Style::default().fg(theme::muted());
    let base = Style::default().fg(theme::fg());
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Wiki link: [[note title]] — brackets dimmed, title in accent.
        if c == '['
            && i + 1 < chars.len()
            && chars[i + 1] == '['
            && let Some(close) = double_bracket_close(&chars, i + 2)
            && close > i + 2
        {
            flush(&mut plain, base, &mut spans);
            spans.push(Span::styled("[[".to_string(), muted));
            spans.push(Span::styled(
                chars[i + 2..close].iter().collect::<String>(),
                Style::default().fg(theme::accent()),
            ));
            spans.push(Span::styled("]]".to_string(), muted));
            i = close + 2;
            continue;
        }
        // Inline code: `code`
        if c == '`'
            && let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '`')
            && close > i + 1
        {
            flush(&mut plain, base, &mut spans);
            spans.push(Span::styled("`".to_string(), muted));
            spans.push(Span::styled(
                chars[i + 1..close].iter().collect::<String>(),
                Style::default().fg(theme::green()),
            ));
            spans.push(Span::styled("`".to_string(), muted));
            i = close + 1;
            continue;
        }
        // Strikethrough: ~~text~~
        if c == '~'
            && i + 1 < chars.len()
            && chars[i + 1] == '~'
            && let Some(close) = double_char(&chars, i + 2, '~')
            && close > i + 2
        {
            flush(&mut plain, base, &mut spans);
            spans.push(Span::styled("~~".to_string(), muted));
            spans.push(Span::styled(
                chars[i + 2..close].iter().collect::<String>(),
                base.add_modifier(Modifier::CROSSED_OUT),
            ));
            spans.push(Span::styled("~~".to_string(), muted));
            i = close + 2;
            continue;
        }
        // Bold: **text**
        if c == '*'
            && i + 1 < chars.len()
            && chars[i + 1] == '*'
            && let Some(close) = double_star(&chars, i + 2)
        {
            flush(&mut plain, base, &mut spans);
            spans.push(Span::styled("**".to_string(), muted));
            spans.push(Span::styled(
                chars[i + 2..close].iter().collect::<String>(),
                base.add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled("**".to_string(), muted));
            i = close + 2;
            continue;
        }
        // Italic: *text* or _text_
        if (c == '*' || c == '_')
            && opens_italic(&chars, i, c)
            && let Some(close) = single_char(&chars, i + 1, c)
            && close > i + 1
        {
            flush(&mut plain, base, &mut spans);
            spans.push(Span::styled(c.to_string(), muted));
            spans.push(Span::styled(
                chars[i + 1..close].iter().collect::<String>(),
                base.add_modifier(Modifier::ITALIC),
            ));
            spans.push(Span::styled(c.to_string(), muted));
            i = close + 1;
            continue;
        }
        plain.push(c);
        i += 1;
    }
    flush(&mut plain, base, &mut spans);
    spans
}

/// Push accumulated plain text (if any) as a span and clear the buffer.
fn flush(plain: &mut String, style: Style, spans: &mut Vec<Span<'static>>) {
    if !plain.is_empty() {
        spans.push(Span::styled(std::mem::take(plain), style));
    }
}

/// Index of the next `**` at or after `start` that has non-empty content before
/// it, or `None`.
fn double_star(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start;
    while j + 1 < chars.len() {
        if chars[j] == '*' && chars[j + 1] == '*' {
            return if j > start { Some(j) } else { None };
        }
        j += 1;
    }
    None
}

/// Index of the next doubled `ch` (e.g. `~~`) at or after `start`, or `None`.
fn double_char(chars: &[char], start: usize, ch: char) -> Option<usize> {
    let mut j = start;
    while j + 1 < chars.len() {
        if chars[j] == ch && chars[j + 1] == ch {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Index of the next single `ch` at or after `start`, or `None`. Used for
/// `*italic*` / `_italic_` spans.
fn single_char(chars: &[char], start: usize, ch: char) -> Option<usize> {
    (start..chars.len()).find(|&j| chars[j] == ch)
}

/// True when an italic marker at `i` opens a plausible span: it isn't part of a
/// doubled `**`/`__`, isn't mid-word (so `snake_case` and `2*3` are left alone),
/// and isn't immediately followed by whitespace.
fn opens_italic(chars: &[char], i: usize, ch: char) -> bool {
    let prev_ok = i == 0 || !chars[i - 1].is_alphanumeric();
    let next_ok = chars
        .get(i + 1)
        .is_some_and(|&n| n != ch && !n.is_whitespace());
    prev_ok && next_ok
}

/// Heading style. Terminals can't scale font size, so every level renders the
/// same (accent + bold) rather than an arbitrary weight/colour hierarchy.
fn heading_style() -> Style {
    Style::default()
        .fg(theme::accent())
        .add_modifier(Modifier::BOLD)
}

/// Index of the next `]]` at or after `start`, or `None`.
fn double_bracket_close(chars: &[char], start: usize) -> Option<usize> {
    let mut j = start;
    while j + 1 < chars.len() {
        if chars[j] == ']' && chars[j + 1] == ']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

// --- rendered markdown (read view) ------------------------------------------

/// A clickable link in the rendered read view: its wrapped-row index, starting
/// column and length within the text area, and the note title it targets.
struct MdLink {
    row: usize,
    col: u16,
    len: u16,
    target: String,
}

/// One rendered character with its style and, for `[[wiki]]` links, its target.
#[derive(Clone)]
struct MdCell {
    ch: char,
    style: Style,
    link: Option<String>,
}

/// Render a markdown body into styled, word-wrapped rows (markers hidden) plus
/// the clickable link positions. This is the "rendered" counterpart to the raw
/// dimmed editor.
fn render_markdown(body: &str, width: u16) -> (Vec<Line<'static>>, Vec<MdLink>) {
    let width = width.max(1) as usize;
    let logical = md_logical_lines(body, width);

    let mut lines: Vec<Line> = Vec::new();
    let mut links: Vec<MdLink> = Vec::new();
    for cells in &logical {
        for row in wrap_cells(cells, width) {
            let row_idx = lines.len();
            let mut spans: Vec<Span> = Vec::new();
            let mut col = 0u16;
            let mut i = 0;
            while i < row.len() {
                let style = row[i].style;
                let link = row[i].link.clone();
                let start = i;
                while i < row.len() && row[i].style == style && row[i].link == link {
                    i += 1;
                }
                let text: String = row[start..i].iter().map(|c| c.ch).collect();
                let len = (i - start) as u16;
                if let Some(target) = &link {
                    links.push(MdLink {
                        row: row_idx,
                        col,
                        len,
                        target: target.clone(),
                    });
                }
                spans.push(Span::styled(text, style));
                col += len;
            }
            lines.push(Line::from(spans));
        }
    }
    (lines, links)
}

/// Turn a body into logical rendered lines (one per source line, unwrapped),
/// with markers hidden and block/inline styling applied.
fn md_logical_lines(body: &str, width: usize) -> Vec<Vec<MdCell>> {
    let base = Style::default().fg(theme::fg());
    let muted = Style::default().fg(theme::muted());

    let mut out: Vec<Vec<MdCell>> = Vec::new();
    let mut in_code = false;
    for raw in body.lines() {
        let trimmed = raw.trim_start();
        // Fenced code blocks: fences are hidden, inner lines render as code.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(styled_cells(raw, Style::default().fg(theme::green())));
            continue;
        }
        // Horizontal rule.
        if is_hr(trimmed) {
            out.push(styled_cells(&"\u{2500}".repeat(width), muted));
            continue;
        }
        // Heading.
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let content = trimmed[hashes..].trim_start();
            out.push(inline_cells(content, heading_style()));
            continue;
        }
        // Blockquote.
        if let Some(rest) = trimmed.strip_prefix('>') {
            let mut cells = styled_cells("\u{2502} ", muted);
            cells.extend(inline_cells(rest.trim_start(), muted));
            out.push(cells);
            continue;
        }
        // Unordered list.
        if matches!(trimmed.chars().next(), Some('-' | '*' | '+')) && trimmed[1..].starts_with(' ')
        {
            let mut cells = styled_cells("\u{2022} ", list_marker_style());
            cells.extend(inline_cells(trimmed[1..].trim_start(), base));
            out.push(cells);
            continue;
        }
        // Ordered list.
        let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0
            && matches!(trimmed[digits..].chars().next(), Some('.' | ')'))
            && trimmed[digits + 1..].starts_with(' ')
        {
            let marker = format!("{} ", &trimmed[..digits + 1]);
            let mut cells = styled_cells(&marker, list_marker_style());
            cells.extend(inline_cells(trimmed[digits + 1..].trim_start(), base));
            out.push(cells);
            continue;
        }
        out.push(inline_cells(raw, base));
    }
    out
}

fn list_marker_style() -> Style {
    Style::default().fg(theme::accent())
}

/// True for a markdown horizontal rule (3+ of the same `-`, `*` or `_`).
fn is_hr(s: &str) -> bool {
    let s = s.trim();
    (s.len() >= 3)
        && (s.chars().all(|c| c == '-')
            || s.chars().all(|c| c == '*')
            || s.chars().all(|c| c == '_'))
}

/// Cells for `text` all in one style, no link.
fn styled_cells(text: &str, style: Style) -> Vec<MdCell> {
    text.chars()
        .map(|ch| MdCell {
            ch,
            style,
            link: None,
        })
        .collect()
}

/// Inline markdown to cells: `[[wiki]]` (accent + underline, carrying its
/// target), `` `code` `` (code colour), `**bold**` (bold), markers hidden.
fn inline_cells(text: &str, base: Style) -> Vec<MdCell> {
    let chars: Vec<char> = text.chars().collect();
    let mut cells: Vec<MdCell> = Vec::new();
    let link_style = Style::default()
        .fg(theme::accent())
        .add_modifier(Modifier::UNDERLINED);
    let code_style = Style::default().fg(theme::green());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '['
            && i + 1 < chars.len()
            && chars[i + 1] == '['
            && let Some(close) = double_bracket_close(&chars, i + 2)
            && close > i + 2
        {
            let target: String = chars[i + 2..close]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            for ch in chars[i + 2..close].iter() {
                cells.push(MdCell {
                    ch: *ch,
                    style: link_style,
                    link: Some(target.clone()),
                });
            }
            i = close + 2;
            continue;
        }
        if c == '`'
            && let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '`')
            && close > i + 1
        {
            for ch in chars[i + 1..close].iter() {
                cells.push(MdCell {
                    ch: *ch,
                    style: code_style,
                    link: None,
                });
            }
            i = close + 1;
            continue;
        }
        if c == '~'
            && i + 1 < chars.len()
            && chars[i + 1] == '~'
            && let Some(close) = double_char(&chars, i + 2, '~')
            && close > i + 2
        {
            push_run(
                &mut cells,
                &chars[i + 2..close],
                base.add_modifier(Modifier::CROSSED_OUT),
            );
            i = close + 2;
            continue;
        }
        if c == '*'
            && i + 1 < chars.len()
            && chars[i + 1] == '*'
            && let Some(close) = double_star(&chars, i + 2)
        {
            push_run(
                &mut cells,
                &chars[i + 2..close],
                base.add_modifier(Modifier::BOLD),
            );
            i = close + 2;
            continue;
        }
        if (c == '*' || c == '_')
            && opens_italic(&chars, i, c)
            && let Some(close) = single_char(&chars, i + 1, c)
            && close > i + 1
        {
            push_run(
                &mut cells,
                &chars[i + 1..close],
                base.add_modifier(Modifier::ITALIC),
            );
            i = close + 1;
            continue;
        }
        cells.push(MdCell {
            ch: c,
            style: base,
            link: None,
        });
        i += 1;
    }
    cells
}

/// Push a run of characters as (linkless) cells sharing one style.
fn push_run(cells: &mut Vec<MdCell>, chars: &[char], style: Style) {
    for &ch in chars {
        cells.push(MdCell {
            ch,
            style,
            link: None,
        });
    }
}

/// Word-wrap a logical line of cells to `width`, hard-splitting any single word
/// longer than the width. An empty input yields one empty row (a blank line).
fn wrap_cells(cells: &[MdCell], width: usize) -> Vec<Vec<MdCell>> {
    if cells.is_empty() {
        return vec![Vec::new()];
    }
    let mut rows: Vec<Vec<MdCell>> = Vec::new();
    let mut cur: Vec<MdCell> = Vec::new();
    let mut i = 0;
    while i < cells.len() {
        if cells[i].ch == ' ' {
            if cur.len() < width {
                cur.push(cells[i].clone());
            }
            i += 1;
            continue;
        }
        let start = i;
        while i < cells.len() && cells[i].ch != ' ' {
            i += 1;
        }
        let word = &cells[start..i];
        if word.len() > width {
            for c in word {
                if cur.len() >= width {
                    rows.push(std::mem::take(&mut cur));
                }
                cur.push(c.clone());
            }
        } else {
            if cur.len() + word.len() > width {
                while cur.last().map(|c| c.ch == ' ').unwrap_or(false) {
                    cur.pop();
                }
                rows.push(std::mem::take(&mut cur));
            }
            cur.extend(word.iter().cloned());
        }
    }
    rows.push(cur);
    rows
}

fn render_note_view(f: &mut Frame, app: &mut App) {
    let screen = f.area();
    let body = Rect {
        x: screen.x,
        y: screen.y.saturating_add(1),
        width: screen.width,
        height: screen.height.saturating_sub(2),
    };
    overlay_clear(f, body);

    let Some((title, content)) = app
        .current_note()
        .map(|n| (n.title.clone(), n.body.clone()))
    else {
        app.clickables.note_view_links.clear();
        return;
    };
    let block = pane_block(&format!("Note · {title}"), true);
    let inner = block.inner(body);
    f.render_widget(block, body);
    let [text_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let width = text_area.width.max(1);
    let height = text_area.height.max(1);

    let (mut rows, mut links) = render_markdown(&content, width);
    if content.trim().is_empty() {
        rows = vec![Line::from(Span::styled(
            "(empty — Tab to edit, E for $EDITOR)",
            Style::default().fg(theme::muted()),
        ))];
    }

    // Append a backlinks section (each backlink is itself a clickable link).
    let here = app.current_note_ref();
    let backlinks = app.note_backlinks(&title, here);
    if !backlinks.is_empty() {
        rows.push(Line::raw(""));
        rows.push(Line::from(Span::styled(
            "Linked from",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        )));
        for b in &backlinks {
            let bt = app.notebooks[b.notebook].notes[b.note].title.clone();
            links.push(MdLink {
                row: rows.len(),
                col: 2,
                len: bt.chars().count() as u16,
                target: bt.clone(),
            });
            rows.push(Line::from(vec![
                Span::styled("\u{2190} ", Style::default().fg(theme::muted())),
                Span::styled(
                    bt,
                    Style::default()
                        .fg(theme::accent())
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
        }
    }

    // Clamp the scroll now that we know the wrapped height.
    let total = rows.len() as u16;
    let max_scroll = total.saturating_sub(height);
    let scroll = if let Mode::NoteView(state) = &mut app.mode {
        state.scroll = state.scroll.min(max_scroll);
        state.scroll
    } else {
        0
    };

    let visible: Vec<Line> = rows
        .into_iter()
        .skip(scroll as usize)
        .take(height as usize)
        .collect();
    f.render_widget(Paragraph::new(Text::from(visible)), text_area);

    // Record clickable rects for visible, resolvable links.
    app.clickables.note_view_links.clear();
    for l in &links {
        if l.row >= scroll as usize
            && l.row < scroll as usize + height as usize
            && let Some(r) = app.resolve_note_link(&l.target)
        {
            app.clickables.note_view_links.push((
                Rect {
                    x: text_area.x + l.col,
                    y: text_area.y + (l.row as u16 - scroll),
                    width: l.len,
                    height: 1,
                },
                r,
            ));
        }
    }

    let more = if total > height {
        "  ·  j/k scroll"
    } else {
        ""
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("Tab edit · f follow link · Esc back{more}"),
            Style::default().fg(theme::muted()),
        ))),
        hint_area,
    );
}

fn render_note_link_picker(f: &mut Frame, app: &App) {
    let Mode::NoteLinkPicker(state) = &app.mode else {
        return;
    };
    let area = centered_rect(f.area(), 60, state.entries.len() as u16 + 6);
    overlay_clear(f, area);
    let block = overlay_block("Follow link", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Jump to:",
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, entry) in state.entries.iter().enumerate() {
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(entry.label.clone(), name_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} Enter jump \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(
        Block::default().style(Style::default().bg(theme::surface())),
        area,
    );

    // Keep the footer minimal: just the escape hatches. The full keybinding
    // list lives in the `?` help overlay. Keys come from the live keymap so
    // custom bindings show correctly.
    let help_key = primary_key(app, Action::Help);
    let quit_key = primary_key(app, Action::Quit);
    let back_key = primary_key(app, Action::Back);
    let chips: Vec<(&str, &str)> = if matches!(app.mode, Mode::Detail) {
        vec![(back_key.as_str(), " back"), (help_key.as_str(), " help")]
    } else {
        vec![(help_key.as_str(), " help"), (quit_key.as_str(), " quit")]
    };

    let mut right = String::new();
    // Surface the active filter/sort, from whichever tab is showing. Kept quiet
    // in the common case (no filter, manual sort).
    match app.active_tab {
        Tab::Tasks => {
            if app.filter.is_active() {
                right.push_str(&format!("filter:{}", app.filter.status.0.label()));
                if !app.filter.query.is_empty() {
                    right.push_str(&format!(" /{}", app.filter.query));
                }
                right.push(' ');
            }
            if app.sort != Sort::Manual {
                right.push_str(&format!(
                    "sort:{} {} ",
                    app.sort.label(),
                    app.sort_dir.arrow()
                ));
            }
        }
        Tab::Notes => {
            if app.note_filter_active() {
                right.push_str(&format!("search:/{} ", app.note_query));
            }
            if app.note_sort != NoteSort::Manual {
                right.push_str(&format!(
                    "sort:{} {} ",
                    app.note_sort.label(),
                    app.note_sort_dir.arrow()
                ));
            }
        }
    }
    if !app.status.is_empty() {
        right.push_str(&app.status);
        right.push(' ');
    }

    let right_w = right.chars().count() as u16 + 1;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(right_w)]).areas(area);

    f.render_widget(Paragraph::new(chip_line(&chips)), left_area);
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
    // The note body gets the whole screen to write in, not a small popup.
    if input.field == InputField::NoteBody {
        render_note_body_editor(f, app, input);
        return;
    }
    let height = if input.multiline { 10 } else { 3 };
    let area = centered_rect(f.area(), 70, height);
    overlay_clear(f, area);
    let block = overlay_block(&input.prompt, theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    // The quick-add task input and the new-note input highlight their markers
    // live; every other field is plain text. Highlighting only recolors
    // characters, so the cursor math below (counting characters) stays correct.
    if input.field == InputField::NewTask {
        f.render_widget(
            Paragraph::new(highlight_quick_add(&input.buffer)).wrap(Wrap { trim: false }),
            inner,
        );
    } else if input.field == InputField::NewNote {
        f.render_widget(
            Paragraph::new(highlight_note_title(&input.buffer)).wrap(Wrap { trim: false }),
            inner,
        );
    } else {
        f.render_widget(
            Paragraph::new(input.buffer.as_str())
                .style(Style::default().fg(theme::fg()))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }

    // Place the real terminal cursor.
    if input.multiline {
        let mut x = inner.x;
        let mut y = inner.y;
        for ch in input.buffer.chars().take(input.cursor) {
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

/// Full-screen editor for a note body: fills the whole body region (between the
/// header and footer), titled with the note, with a save/cancel hint at the
/// bottom. Drawn as an overlay over the note panes.
fn render_note_body_editor(f: &mut Frame, app: &App, input: &crate::app::InputState) {
    let screen = f.area();
    let body = Rect {
        x: screen.x,
        y: screen.y.saturating_add(1),
        width: screen.width,
        height: screen.height.saturating_sub(2),
    };
    overlay_clear(f, body);

    let title = app
        .current_note()
        .map(|n| format!("Note · {}", n.title))
        .unwrap_or_else(|| "Note".to_string());
    let block = pane_block(&title, true);
    let block_inner = block.inner(body);
    f.render_widget(block, body);

    // Reserve the last inner row for the key hint; text fills the rest.
    let [text_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(block_inner);
    let width = text_area.width.max(1);
    let height = text_area.height.max(1);

    // Cursor's (row, col) using a hard char-wrap at the pane width — the same
    // model drives both the on-screen cursor and the vertical scroll, so they
    // stay in lockstep.
    let (mut crow, mut ccol) = (0u16, 0u16);
    for ch in input.buffer.chars().take(input.cursor) {
        if ch == '\n' {
            crow += 1;
            ccol = 0;
        } else {
            ccol += 1;
            if ccol >= width {
                ccol = 0;
                crow += 1;
            }
        }
    }
    // Scroll so the cursor row stays on screen.
    let scroll_y = crow.saturating_sub(height - 1);

    // `split('\n')` (not `lines()`) so a trailing newline shows as a blank last
    // line and the cursor can sit on it.
    let body_lines: Vec<Line> = input.buffer.split('\n').map(markdown_line).collect();
    f.render_widget(
        Paragraph::new(Text::from(body_lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll_y, 0)),
        text_area,
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Enter = newline  ·  Tab = read view  ·  Esc or Ctrl+S = save & close",
            Style::default().fg(theme::muted()),
        ))),
        hint_area,
    );

    f.set_cursor_position((
        (text_area.x + ccol).min(text_area.x + width - 1),
        text_area.y + (crow - scroll_y),
    ));
}

/// Colour `#tag` tokens in the new-note input purple (the same colour tags use
/// elsewhere), leaving the rest of the title plain. Walks whitespace/word runs so
/// every character is preserved and the input cursor stays aligned.
fn highlight_note_title(buffer: &str) -> Line<'static> {
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;
    while i < buffer.len() {
        let ws = buffer[i..].starts_with(char::is_whitespace);
        let end = buffer[i..]
            .char_indices()
            .find(|(_, c)| c.is_whitespace() != ws)
            .map(|(b, _)| i + b)
            .unwrap_or(buffer.len());
        let slice = &buffer[i..end];
        let style = if !ws && slice.starts_with('#') {
            Style::default().fg(theme::purple())
        } else {
            Style::default().fg(theme::fg())
        };
        spans.push(Span::styled(slice.to_string(), style));
        i = end;
    }
    Line::from(spans)
}

/// Colour the quick-add markers in the task input, reusing the same palette the
/// task list uses (priority colour, purple tags, teal due date) so what lights
/// up while typing matches how the task will look. Classification comes from
/// [`quick_add_spans`], so the highlight can't drift from what gets parsed.
fn highlight_quick_add(buffer: &str) -> Line<'static> {
    let spans = quick_add_spans(buffer, App::today())
        .into_iter()
        .map(|(start, end, kind)| {
            let style = match kind {
                QuickAddSpan::Plain => Style::default().fg(theme::fg()),
                QuickAddSpan::Priority(p) => Style::default()
                    .fg(theme::priority_color(p))
                    .add_modifier(Modifier::BOLD),
                QuickAddSpan::Points => Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD),
                QuickAddSpan::Tag => Style::default().fg(theme::purple()),
                QuickAddSpan::Due => Style::default().fg(theme::teal()),
                QuickAddSpan::NoteMarker => Style::default()
                    .fg(theme::amber())
                    .add_modifier(Modifier::BOLD),
                QuickAddSpan::SubtaskMarker => Style::default()
                    .fg(theme::green())
                    .add_modifier(Modifier::BOLD),
                QuickAddSpan::NoteText | QuickAddSpan::SubtaskText => {
                    Style::default().fg(theme::muted())
                }
            };
            Span::styled(buffer[start..end].to_string(), style)
        })
        .collect::<Vec<_>>();
    Line::from(spans)
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

/// Space-joined labels for every chord bound to the given actions, e.g.
/// `[MoveDown, MoveUp]` -> "j ↓ k ↑". Reads the live keymap so the help stays
/// truthful after the user customizes their bindings. Duplicate labels are
/// dropped (e.g. BackTab and Shift+Tab both render as "⇧Tab").
fn keys_for(app: &App, actions: &[Action]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for &action in actions {
        for chord in app.keymap.chords_for(action) {
            let label = chord.display();
            if !parts.contains(&label) {
                parts.push(label);
            }
        }
    }
    parts.join(" ")
}

/// The first (primary) bound key for an action, for compact one-key hints.
fn primary_key(app: &App, action: Action) -> String {
    app.keymap
        .primary(action)
        .map(|c| c.display())
        .unwrap_or_default()
}

fn render_help(f: &mut Frame, app: &App) {
    let area = centered_rect_cols(f.area(), 68, 24);
    overlay_clear(f, area);
    let block = overlay_block(
        &format!("Keybindings · {}", app.active_tab.label()),
        theme::accent(),
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Grouped bindings in two columns above a one-line footer, so the whole
    // cheat-sheet stays short enough to fit a typical terminal.
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)]).areas(body);

    let header = |t: &str| {
        Line::from(Span::styled(
            t.to_string(),
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ))
    };
    // A `key … description` row; the key is padded to `kw` so descriptions align.
    let row = |keys: &str, desc: &str, kw: usize| {
        Line::from(vec![
            Span::styled(
                format!("{keys:<kw$}"),
                Style::default()
                    .fg(theme::fg())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc.to_string(), Style::default().fg(theme::muted())),
        ])
    };

    const LW: usize = 12;
    const RW: usize = 5;
    let tab_row = row(
        &keys_for(app, &[Action::SwitchToTasks, Action::SwitchToNotes]),
        "Tasks / Notes tab",
        LW,
    );
    let (left_lines, right_lines) = match app.active_tab {
        Tab::Tasks => (
            vec![
                header("Navigate"),
                row(&keys_for(app, &[Action::ToggleFocus]), "toggle panes", LW),
                row(
                    &keys_for(
                        app,
                        &[Action::FocusLists, Action::FocusTasks, Action::FocusTags],
                    ),
                    "lists / tasks / tags",
                    LW,
                ),
                row(
                    &keys_for(app, &[Action::MoveDown, Action::MoveUp]),
                    "move cursor",
                    LW,
                ),
                row(&keys_for(app, &[Action::Activate]), "open / drill in", LW),
                row(&keys_for(app, &[Action::Back]), "back / clear", LW),
                tab_row.clone(),
                Line::raw(""),
                header("Organize"),
                row(&keys_for(app, &[Action::ToggleDone]), "toggle done", LW),
                row(
                    &keys_for(app, &[Action::ReorderDown, Action::ReorderUp]),
                    "reorder up / down",
                    LW,
                ),
                row(
                    &keys_for(app, &[Action::SendTop, Action::SendBottom]),
                    "to top / bottom",
                    LW,
                ),
                row(&keys_for(app, &[Action::MoveTask]), "move / unarchive", LW),
                Line::raw(""),
                header("Find"),
                row(&keys_for(app, &[Action::Search]), "search", LW),
                row(&keys_for(app, &[Action::CycleFilter]), "cycle filter", LW),
                row(&keys_for(app, &[Action::SortPicker]), "sort tasks", LW),
                row(&keys_for(app, &[Action::Copy]), "copy task", LW),
            ],
            vec![
                header("Create & edit"),
                row(
                    &keys_for(app, &[Action::AddTask, Action::AddList]),
                    "add task / list",
                    RW,
                ),
                row(&keys_for(app, &[Action::Edit]), "edit / rename", RW),
                row(&keys_for(app, &[Action::AddSubtask]), "add subtask", RW),
                Line::raw(""),
                header("Details"),
                row(&keys_for(app, &[Action::CyclePriority]), "priority", RW),
                row(&keys_for(app, &[Action::SetPoints]), "story points", RW),
                row(&keys_for(app, &[Action::SetDue]), "due date", RW),
                row(&keys_for(app, &[Action::SetTags]), "tags", RW),
                row(&keys_for(app, &[Action::SetNotes]), "notes", RW),
                Line::raw(""),
                header("Remove"),
                row(&keys_for(app, &[Action::Archive]), "archive task", RW),
                row(&keys_for(app, &[Action::Delete]), "delete (confirm)", RW),
                Line::raw(""),
                header("App"),
                row(&keys_for(app, &[Action::ThemePicker]), "theme", RW),
                row(&keys_for(app, &[Action::Settings]), "settings", RW),
                row(
                    &keys_for(app, &[Action::Help, Action::Quit]),
                    "help / quit",
                    RW,
                ),
            ],
        ),
        Tab::Notes => (
            vec![
                header("Navigate"),
                row(&keys_for(app, &[Action::ToggleFocus]), "toggle panes", LW),
                row(
                    &keys_for(
                        app,
                        &[Action::FocusLists, Action::FocusTasks, Action::FocusTags],
                    ),
                    "notebooks / notes / tags",
                    LW,
                ),
                row(
                    &keys_for(app, &[Action::MoveDown, Action::MoveUp]),
                    "move cursor",
                    LW,
                ),
                row(
                    &keys_for(app, &[Action::Activate]),
                    "open note / drill in",
                    LW,
                ),
                row(&keys_for(app, &[Action::Back]), "back / clear", LW),
                tab_row.clone(),
                Line::raw(""),
                header("Organize"),
                row(
                    &keys_for(app, &[Action::ReorderDown, Action::ReorderUp]),
                    "reorder up / down",
                    LW,
                ),
                row(
                    &keys_for(app, &[Action::SendTop, Action::SendBottom]),
                    "to top / bottom",
                    LW,
                ),
                row(&keys_for(app, &[Action::TogglePin]), "pin / unpin", LW),
                row(&keys_for(app, &[Action::MoveTask]), "move / unarchive", LW),
                Line::raw(""),
                header("Find"),
                row(&keys_for(app, &[Action::Search]), "search", LW),
                row(&keys_for(app, &[Action::SortPicker]), "sort notes", LW),
                row(&keys_for(app, &[Action::Copy]), "copy note", LW),
            ],
            vec![
                header("Create & edit"),
                row(
                    &keys_for(app, &[Action::AddTask, Action::AddList]),
                    "add note / notebook",
                    RW,
                ),
                row(&keys_for(app, &[Action::Edit]), "edit title / rename", RW),
                row(&keys_for(app, &[Action::SetNotes]), "edit body", RW),
                row(
                    &keys_for(app, &[Action::ExternalEdit]),
                    "open in $EDITOR",
                    RW,
                ),
                row(&keys_for(app, &[Action::SetTags]), "tags", RW),
                Line::raw(""),
                header("Remove"),
                row(&keys_for(app, &[Action::Archive]), "archive note", RW),
                row(&keys_for(app, &[Action::Delete]), "delete (confirm)", RW),
                Line::raw(""),
                header("App"),
                row(&keys_for(app, &[Action::ThemePicker]), "theme", RW),
                row(&keys_for(app, &[Action::Settings]), "settings", RW),
                row(
                    &keys_for(app, &[Action::Help, Action::Quit]),
                    "help / quit",
                    RW,
                ),
            ],
        ),
    };

    f.render_widget(Paragraph::new(Text::from(left_lines)), left);
    f.render_widget(Paragraph::new(Text::from(right_lines)), right);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "press any key to close",
            Style::default().fg(theme::muted()),
        )))
        .alignment(Alignment::Center),
        footer,
    );
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

fn render_move_picker(f: &mut Frame, app: &mut App) {
    if !matches!(app.mode, Mode::MovePicker(_)) {
        return;
    }
    let moving = app
        .current_task()
        .map(|t| t.title.clone())
        .unwrap_or_default();
    let filtered = app.move_picker_filtered();

    // Chrome above/below the scrolling rows: 2 borders + subtitle + filter +
    // blank + blank + footer = 7 lines. Cap the visible rows to what fits (and
    // keep the popup from swallowing the whole screen).
    const MAX_ROWS: u16 = 14;
    let avail = f.area().height.saturating_sub(7).max(1);
    let visible_rows = (filtered.len().max(1) as u16).min(avail).min(MAX_ROWS);

    // Clamp the cursor and follow it with the scroll offset.
    let (selected, scroll, filter) = if let Mode::MovePicker(state) = &mut app.mode {
        if filtered.is_empty() {
            state.selected = 0;
        } else if state.selected >= filtered.len() {
            state.selected = filtered.len() - 1;
        }
        let max_scroll = (filtered.len() as u16).saturating_sub(visible_rows);
        let sel = state.selected as u16;
        if sel < state.scroll {
            state.scroll = sel;
        } else if sel >= state.scroll + visible_rows {
            state.scroll = sel + 1 - visible_rows;
        }
        state.scroll = state.scroll.min(max_scroll);
        (state.selected, state.scroll, state.filter.clone())
    } else {
        return;
    };

    let area = centered_rect(f.area(), 56, visible_rows + 7);
    overlay_clear(f, area);
    let block = overlay_block("Move to list", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        format!("Move \u{201c}{moving}\u{201d} to:"),
        Style::default().fg(theme::muted()),
    ))];
    // Filter row with a block cursor; a muted hint stands in when it's empty.
    if filter.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(theme::muted())),
            Span::styled("\u{2588}", Style::default().fg(theme::accent())),
            Span::styled(
                " type to filter",
                Style::default().fg(theme::sel_dim()),
            ),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(theme::muted())),
            Span::styled(filter, Style::default().fg(theme::fg())),
            Span::styled("\u{2588}", Style::default().fg(theme::accent())),
        ]));
    }
    lines.push(Line::raw(""));

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no lists match",
            Style::default().fg(theme::muted()),
        )));
        for _ in 1..visible_rows {
            lines.push(Line::raw(""));
        }
    } else {
        let start = scroll as usize;
        let end = (start + visible_rows as usize).min(filtered.len());
        for (offset, &li) in filtered[start..end].iter().enumerate() {
            let list = &app.lists[li];
            let selected_row = start + offset == selected;
            let marker = if selected_row { "\u{258e} " } else { "  " };
            let name_style = if selected_row {
                Style::default()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::fg())
            };
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme::accent())),
                Span::styled(format!("{:<22}", list.name), name_style),
                Span::styled(
                    format!("{}/{}", list.open_count(), list.tasks.len()),
                    Style::default().fg(theme::muted()),
                ),
            ]));
        }
        // Pad short pages so the footer stays anchored to the bottom.
        for _ in (end - start)..visible_rows as usize {
            lines.push(Line::raw(""));
        }
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} Enter move \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Scrollbar alongside the rows region when the filtered set overflows.
    if filtered.len() as u16 > visible_rows {
        let rows_area = Rect {
            x: inner.x,
            y: inner.y + 3,
            width: inner.width,
            height: visible_rows,
        };
        render_scrollbar(f, rows_area, filtered.len(), scroll as usize);
    }
}

fn render_note_copy_menu(f: &mut Frame, app: &App) {
    let Mode::NoteCopyMenu(state) = &app.mode else {
        return;
    };
    let options = crate::app::NoteCopyWhat::all();
    let area = centered_rect(f.area(), 56, options.len() as u16 + 6);
    overlay_clear(f, area);
    let block = overlay_block("Copy", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let title = app
        .current_note()
        .map(|n| n.title.clone())
        .unwrap_or_default();
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("Copy from \u{201c}{title}\u{201d}:"),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, what) in options.iter().enumerate() {
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{}  ", i + 1), Style::default().fg(theme::muted())),
            Span::styled(what.label().to_string(), name_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} 1-3 quick \u{00b7} Enter copy \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_note_move_picker(f: &mut Frame, app: &App) {
    let Mode::NoteMovePicker(state) = &app.mode else {
        return;
    };
    let area = centered_rect(f.area(), 56, state.targets.len() as u16 + 6);
    overlay_clear(f, area);
    let block = overlay_block("Move to notebook", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let moving = app
        .current_note()
        .map(|n| n.title.clone())
        .unwrap_or_default();
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("Move \u{201c}{moving}\u{201d} to:"),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, &ni) in state.targets.iter().enumerate() {
        let nb = &app.notebooks[ni];
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{:<22}", nb.name), name_style),
            Span::styled(
                format!("{}", nb.notes.len()),
                Style::default().fg(theme::muted()),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} Enter move \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_copy_menu(f: &mut Frame, app: &App) {
    let Mode::CopyMenu(state) = &app.mode else {
        return;
    };
    let options = crate::app::CopyWhat::all();
    let area = centered_rect(f.area(), 56, options.len() as u16 + 6);
    overlay_clear(f, area);
    let block = overlay_block("Copy", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let title = app
        .current_task()
        .map(|t| t.title.clone())
        .unwrap_or_default();
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            format!("Copy from \u{201c}{title}\u{201d}:"),
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, what) in options.iter().enumerate() {
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{}  ", i + 1), Style::default().fg(theme::muted())),
            Span::styled(what.label().to_string(), name_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} 1-3 quick \u{00b7} Enter copy \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_sort_picker(f: &mut Frame, app: &App) {
    let Mode::SortPicker(state) = &app.mode else {
        return;
    };
    let options = crate::model::Sort::all();
    let area = centered_rect(f.area(), 60, options.len() as u16 + 7);
    overlay_clear(f, area);
    let block = overlay_block("Sort tasks", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Order the tasks in this list by:",
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, sort) in options.iter().enumerate() {
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{}  ", i + 1), Style::default().fg(theme::muted())),
            Span::styled(format!("{:<10}", sort.label()), name_style),
            Span::styled(
                format!("  {}", sort.description()),
                Style::default().fg(theme::muted()),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("direction  ", Style::default().fg(theme::muted())),
        Span::styled(
            format!("{} {}", app.sort_dir.label(), app.sort_dir.arrow()),
            Style::default().fg(theme::accent()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} 1-6 quick \u{00b7} r flip \u{00b7} Enter apply \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_note_sort_picker(f: &mut Frame, app: &App) {
    let Mode::NoteSortPicker(state) = &app.mode else {
        return;
    };
    let options = NoteSort::all();
    let area = centered_rect(f.area(), 60, options.len() as u16 + 7);
    overlay_clear(f, area);
    let block = overlay_block("Sort notes", theme::accent());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            "Order the notes by:",
            Style::default().fg(theme::muted()),
        )),
        Line::raw(""),
    ];
    for (i, sort) in options.iter().enumerate() {
        let selected = i == state.selected;
        let marker = if selected { "\u{258e} " } else { "  " };
        let name_style = if selected {
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::fg())
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::accent())),
            Span::styled(format!("{}  ", i + 1), Style::default().fg(theme::muted())),
            Span::styled(format!("{:<10}", sort.label()), name_style),
            Span::styled(
                format!("  {}", sort.description()),
                Style::default().fg(theme::muted()),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("direction  ", Style::default().fg(theme::muted())),
        Span::styled(
            format!(
                "{} {}",
                app.note_sort_dir.label(),
                app.note_sort_dir.arrow()
            ),
            Style::default().fg(theme::accent()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "\u{2191}/\u{2193} choose \u{00b7} 1-4 quick \u{00b7} r flip \u{00b7} Enter apply \u{00b7} Esc cancel",
        Style::default().fg(theme::muted()),
    )));

    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_settings(f: &mut Frame, app: &App) {
    let area = centered_rect(f.area(), 76, 19);
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
            value("JSON lists + notes/ (one .md per note), plus config.json".to_string()),
        ]),
        Line::from(vec![label("Theme"), value(app.theme.name().to_string())]),
        Line::from(vec![
            label("Completed"),
            value(
                if app.completed_to_bottom {
                    "sink to bottom of list"
                } else {
                    "stay in place"
                }
                .to_string(),
            ),
        ]),
        Line::from(vec![
            label("Contents"),
            value(format!(
                "{} lists · {} tasks   ·   {} notebooks · {} notes",
                app.lists.len(),
                app.task_count(),
                app.notebooks.len(),
                app.total_note_count()
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
    lines.push(Line::from(vec![
        Span::styled(
            "c",
            Style::default()
                .fg(theme::accent())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  toggle sinking completed tasks to the bottom",
            Style::default().fg(theme::fg()),
        ),
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

/// A centered rectangle of a fixed column width (capped to the area) and height.
fn centered_rect_cols(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
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
