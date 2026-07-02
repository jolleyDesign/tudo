//! Application state and all the actions that mutate it.
//!
//! `App` owns everything. Its methods are deliberately terminal-free so they can
//! be unit-tested directly; `ui.rs` renders from `&mut App` and `event.rs` maps
//! input to these methods.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

/// How long a transient status-line message stays on screen before clearing.
pub const STATUS_TTL: Duration = Duration::from_secs(4);

use crate::config::{self, Config};
use crate::model::{self, List, Subtask, Task};
use crate::storage;
use crate::theme::{self, ThemeKind};

/// Which pane currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Lists,
    Tasks,
}

/// Which text field an [`InputState`] is collecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    NewList,
    RenameList,
    NewTask,
    EditTask,
    Tags,
    Due,
    Notes,
    NewSubtask,
    EditSubtask,
    Search,
    DataDir,
}

/// State for the modal text-entry overlay.
#[derive(Debug, Clone)]
pub struct InputState {
    pub field: InputField,
    pub prompt: String,
    pub buffer: String,
    /// Cursor position as a character index into `buffer`.
    pub cursor: usize,
    pub multiline: bool,
    /// Return to Detail mode (vs Normal) when finished.
    pub return_detail: bool,
}

impl InputState {
    fn new(
        field: InputField,
        prompt: &str,
        buffer: String,
        multiline: bool,
        return_detail: bool,
    ) -> Self {
        let cursor = buffer.chars().count();
        Self {
            field,
            prompt: prompt.to_string(),
            buffer,
            cursor,
            multiline,
            return_detail,
        }
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.buffer.len())
    }

    pub fn insert(&mut self, c: char) {
        let b = self.byte_at(self.cursor);
        self.buffer.insert(b, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let start = self.byte_at(self.cursor - 1);
            let end = self.byte_at(self.cursor);
            self.buffer.replace_range(start..end, "");
            self.cursor -= 1;
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        let n = self.buffer.chars().count();
        if self.cursor < n {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.buffer.chars().count();
    }
}

/// What a pending [`Mode::Confirm`] will do if accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum ConfirmAction {
    DeleteTask,
    DeleteList,
    DeleteSubtask,
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub prompt: String,
    pub action: ConfirmAction,
    pub return_detail: bool,
}

/// State for the first-run data-directory picker.
#[derive(Debug, Clone)]
pub struct FirstRunState {
    pub options: Vec<(String, PathBuf)>,
    /// Selected index; `options.len()` means the "custom path" entry.
    pub selected: usize,
    pub custom: String,
    pub editing_custom: bool,
}

impl FirstRunState {
    fn new() -> Self {
        Self {
            options: config::first_run_options(),
            selected: 0,
            custom: String::new(),
            editing_custom: false,
        }
    }

    /// Total selectable entries (options + the custom entry).
    pub fn entry_count(&self) -> usize {
        self.options.len() + 1
    }
}

/// State for the theme picker overlay.
#[derive(Debug, Clone)]
pub struct ThemePickerState {
    /// Index into [`ThemeKind::all`] of the highlighted theme.
    pub selected: usize,
    /// Theme active when the picker opened, restored on cancel.
    pub original: ThemeKind,
}

/// State for the "move task to another list" picker overlay.
#[derive(Debug, Clone)]
pub struct MovePickerState {
    /// Indices into [`App::lists`] that are valid targets (every list but the source).
    pub targets: Vec<usize>,
    /// Cursor into `targets`.
    pub selected: usize,
    /// Return to Detail mode (vs Normal) on cancel.
    pub return_detail: bool,
}

/// What part of the selected task a copy action puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyWhat {
    /// The whole task serialized as pretty JSON (id, dates, priority, tags…).
    Json,
    /// Just the task title.
    Title,
    /// Just the task's free-text notes / description.
    Notes,
}

impl CopyWhat {
    /// All options in the order they appear in the copy menu.
    pub fn all() -> [CopyWhat; 3] {
        [CopyWhat::Json, CopyWhat::Title, CopyWhat::Notes]
    }

    /// Menu label.
    pub fn label(self) -> &'static str {
        match self {
            CopyWhat::Json => "Full task (JSON)",
            CopyWhat::Title => "Title",
            CopyWhat::Notes => "Description (notes)",
        }
    }

    /// Short noun for the "copied <noun>" status line.
    fn noun(self) -> &'static str {
        match self {
            CopyWhat::Json => "task JSON",
            CopyWhat::Title => "title",
            CopyWhat::Notes => "description",
        }
    }
}

/// State for the "copy selected task" menu overlay.
#[derive(Debug, Clone)]
pub struct CopyMenuState {
    /// Cursor into [`CopyWhat::all`].
    pub selected: usize,
    /// Return to Detail mode (vs Normal) when finished.
    pub return_detail: bool,
}

/// The active interaction mode.
#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Detail,
    Input(InputState),
    Confirm(ConfirmState),
    FirstRun(FirstRunState),
    ThemePicker(ThemePickerState),
    MovePicker(MovePickerState),
    CopyMenu(CopyMenuState),
    Settings,
    Help,
}

/// Status filter cycled with `f`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Active,
    Completed,
}

impl StatusFilter {
    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "all",
            StatusFilter::Active => "active",
            StatusFilter::Completed => "done",
        }
    }

    fn next(self) -> Self {
        match self {
            StatusFilter::All => StatusFilter::Active,
            StatusFilter::Active => StatusFilter::Completed,
            StatusFilter::Completed => StatusFilter::All,
        }
    }
}

/// Current task filter: a text query plus a status filter.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub query: String,
    pub status: StatusFilterWrap,
}

/// Newtype so `Filter` can derive `Default` (Active by default would surprise).
#[derive(Debug, Clone, Copy)]
pub struct StatusFilterWrap(pub StatusFilter);

impl Default for StatusFilterWrap {
    fn default() -> Self {
        StatusFilterWrap(StatusFilter::All)
    }
}

impl Filter {
    pub fn is_active(&self) -> bool {
        !self.query.is_empty() || self.status.0 != StatusFilter::All
    }

    pub fn matches(&self, t: &Task) -> bool {
        let status_ok = match self.status.0 {
            StatusFilter::All => true,
            StatusFilter::Active => !t.done,
            StatusFilter::Completed => t.done,
        };
        if !status_ok {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }
        let q = self.query.to_lowercase();
        t.title.to_lowercase().contains(&q)
            || t.notes.to_lowercase().contains(&q)
            || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
    }
}

/// Screen rectangles recorded during render for mouse hit-testing.
#[derive(Debug, Clone, Default)]
pub struct Clickables {
    /// Inner area of the lists pane and the scroll offset used to draw it.
    pub lists_inner: Option<(Rect, usize)>,
    /// Inner area of the tasks pane and the scroll offset used to draw it.
    pub tasks_inner: Option<(Rect, usize)>,
}

pub struct App {
    pub data_dir: PathBuf,
    pub lists: Vec<List>,
    pub mode: Mode,
    pub focus: Focus,
    pub list_state: ListState,
    pub task_state: ListState,
    pub subtask_state: ListState,
    pub filter: Filter,
    pub theme: ThemeKind,
    pub status: String,
    /// When the current `status` message should auto-clear (`None` = nothing showing).
    status_expiry: Option<Instant>,
    pub should_quit: bool,
    pub clickables: Clickables,
}

impl App {
    /// Build the app. `data_dir = None` starts the first-run picker. The active
    /// theme defaults to Tokyo Night; call [`App::set_theme`] to change it.
    pub fn new(data_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let (data_dir, lists, mode) = match data_dir {
            Some(dir) => {
                let lists = storage::load_lists(&dir)?;
                (dir, lists, Mode::Normal)
            }
            None => (
                PathBuf::new(),
                Vec::new(),
                Mode::FirstRun(FirstRunState::new()),
            ),
        };

        let theme = ThemeKind::default();
        theme::set(theme.theme());

        let mut app = Self {
            data_dir,
            lists,
            mode,
            focus: Focus::Lists,
            list_state: ListState::default(),
            task_state: ListState::default(),
            subtask_state: ListState::default(),
            filter: Filter::default(),
            theme,
            status: String::new(),
            status_expiry: None,
            should_quit: false,
            clickables: Clickables::default(),
        };

        // Pin any previously-created Archived list to the bottom, then select
        // the first list. (First run has no data dir yet, so skip it.)
        if !matches!(app.mode, Mode::FirstRun(_)) {
            app.sort_lists();
            if !app.lists.is_empty() {
                app.list_state.select(Some(0));
                app.task_state.select(Some(0));
            }
        }

        Ok(app)
    }

    /// Set the active theme without persisting (used at startup).
    pub fn set_theme(&mut self, kind: ThemeKind) {
        self.theme = kind;
        theme::set(kind.theme());
    }

    /// Open the theme picker, remembering the current theme to restore on cancel.
    pub fn open_theme_picker(&mut self) {
        let selected = ThemeKind::all()
            .iter()
            .position(|&k| k == self.theme)
            .unwrap_or(0);
        self.mode = Mode::ThemePicker(ThemePickerState {
            selected,
            original: self.theme,
        });
    }

    /// Move the picker highlight and live-preview that theme (wraps).
    pub fn theme_picker_preview(&mut self, delta: isize) {
        let kind = if let Mode::ThemePicker(state) = &mut self.mode {
            let all = ThemeKind::all();
            let n = all.len() as isize;
            let next = (((state.selected as isize + delta) % n) + n) % n;
            state.selected = next as usize;
            all[next as usize]
        } else {
            return;
        };
        self.set_theme(kind);
    }

    /// Apply the highlighted theme and persist it.
    pub fn theme_picker_confirm(&mut self) {
        if matches!(self.mode, Mode::ThemePicker(_)) {
            self.persist_config();
            self.mode = Mode::Normal;
        }
    }

    /// Open the read-only settings panel.
    pub fn open_settings(&mut self) {
        self.mode = Mode::Settings;
    }

    /// Begin changing the data directory (prefilled with the current path).
    pub fn start_change_data_dir(&mut self) {
        self.mode = Mode::Input(InputState::new(
            InputField::DataDir,
            "Move data to (path; ~ allowed)",
            self.data_dir.display().to_string(),
            false,
            false,
        ));
    }

    /// Move all lists to `raw` (a possibly `~`-prefixed path) and repoint config.
    pub fn relocate_data(&mut self, raw: &str) {
        if raw.trim().is_empty() {
            return;
        }
        let new = config::expand_tilde(raw);
        if new == self.data_dir {
            self.set_status("data is already there".to_string());
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&new) {
            self.set_status(format!("could not create {}: {e}", new.display()));
            return;
        }
        // Write every list into the new dir first; bail (keeping originals) on error.
        let old = self.data_dir.clone();
        let lists = self.lists.clone();
        for list in &lists {
            if let Err(e) = storage::save_list(&new, list) {
                self.set_status(format!("move failed: {e}"));
                return;
            }
        }
        // Then remove the originals.
        for list in &lists {
            let _ = storage::delete_list_file(&old, list);
        }
        self.data_dir = new.clone();
        match storage::load_lists(&new) {
            Ok(l) => self.lists = l,
            Err(e) => self.set_status(format!("load error: {e}")),
        }
        self.sort_lists();
        if self.lists.is_empty() {
            self.list_state.select(None);
            self.task_state.select(None);
        } else {
            self.list_state.select(Some(0));
            self.task_state.select(Some(0));
        }
        self.persist_config();
        self.set_status(format!("data moved to {}", new.display()));
    }

    /// Close the picker and restore the theme that was active when it opened.
    pub fn theme_picker_cancel(&mut self) {
        let original = match &self.mode {
            Mode::ThemePicker(state) => state.original,
            _ => return,
        };
        self.set_theme(original);
        self.mode = Mode::Normal;
    }

    /// Write the current data dir + theme to the config pointer.
    fn persist_config(&mut self) {
        if self.data_dir.as_os_str().is_empty() {
            return; // no data dir chosen yet (first run)
        }
        let config = Config {
            data_dir: self.data_dir.clone(),
            theme: self.theme,
        };
        if let Err(e) = config::save_config(&config) {
            self.set_status(format!("theme set for this session only: {e}"));
        }
    }

    /// Today's date in the local timezone (used for overdue/due-today logic).
    pub fn today() -> chrono::NaiveDate {
        Local::now().date_naive()
    }

    // --- transient status line ----------------------------------------------

    /// Show a status message that auto-clears after [`STATUS_TTL`].
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_expiry = Some(Instant::now() + STATUS_TTL);
    }

    /// The instant the current message should disappear, if one is showing.
    /// The run loop uses this to wake up just in time to clear it.
    pub fn status_deadline(&self) -> Option<Instant> {
        self.status_expiry
    }

    /// Clear the status message once `now` has reached its deadline.
    pub fn expire_status(&mut self, now: Instant) {
        if self.status_expiry.is_some_and(|deadline| now >= deadline) {
            self.status.clear();
            self.status_expiry = None;
        }
    }

    /// Path to the config pointer file, for display in settings.
    pub fn config_path_display(&self) -> String {
        config::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "(unknown)".to_string())
    }

    /// Total tasks across all lists.
    pub fn task_count(&self) -> usize {
        self.lists.iter().map(|l| l.tasks.len()).sum()
    }

    // --- selection accessors -------------------------------------------------

    pub fn selected_list(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    /// Visible-index of the selected task (index into `visible_task_indices`).
    pub fn selected_visible_task(&self) -> usize {
        self.task_state.selected().unwrap_or(0)
    }

    pub fn current_list(&self) -> Option<&List> {
        self.lists.get(self.selected_list())
    }

    /// Indices into the current list's `tasks` that pass the filter.
    pub fn visible_task_indices(&self) -> Vec<usize> {
        match self.current_list() {
            Some(list) => list
                .tasks
                .iter()
                .enumerate()
                .filter(|(_, t)| self.filter.matches(t))
                .map(|(i, _)| i)
                .collect(),
            None => Vec::new(),
        }
    }

    /// Raw index (into `tasks`) of the currently selected task.
    pub fn current_task_index(&self) -> Option<usize> {
        self.visible_task_indices()
            .get(self.selected_visible_task())
            .copied()
    }

    pub fn current_task(&self) -> Option<&Task> {
        let idx = self.current_task_index()?;
        self.current_list()?.tasks.get(idx)
    }

    pub fn selected_subtask(&self) -> Option<usize> {
        let st = self.subtask_state.selected()?;
        let task = self.current_task()?;
        if st < task.subtasks.len() {
            Some(st)
        } else {
            None
        }
    }

    // --- navigation ----------------------------------------------------------

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Lists => Focus::Tasks,
            Focus::Tasks => Focus::Lists,
        };
    }

    pub fn move_selection(&mut self, delta: isize) {
        match self.mode {
            Mode::Detail => self.move_subtask(delta),
            _ => match self.focus {
                Focus::Lists => self.move_list(delta),
                Focus::Tasks => self.move_task(delta),
            },
        }
    }

    fn move_list(&mut self, delta: isize) {
        let len = self.lists.len();
        if len == 0 {
            return;
        }
        let cur = self.selected_list() as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        self.list_state.select(Some(next));
        self.task_state.select(Some(0));
    }

    fn move_task(&mut self, delta: isize) {
        let len = self.visible_task_indices().len();
        if len == 0 {
            self.task_state.select(None);
            return;
        }
        let cur = self.selected_visible_task() as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        self.task_state.select(Some(next));
    }

    fn move_subtask(&mut self, delta: isize) {
        let len = self.current_task().map(|t| t.subtasks.len()).unwrap_or(0);
        if len == 0 {
            self.subtask_state.select(None);
            return;
        }
        let cur = self.subtask_state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        self.subtask_state.select(Some(next));
    }

    pub fn focus_lists(&mut self) {
        self.focus = Focus::Lists;
    }

    pub fn focus_tasks(&mut self) {
        self.focus = Focus::Tasks;
        if self.task_state.selected().is_none() && !self.visible_task_indices().is_empty() {
            self.task_state.select(Some(0));
        }
    }

    pub fn select_list_index(&mut self, idx: usize) {
        if idx < self.lists.len() {
            self.list_state.select(Some(idx));
            self.task_state.select(Some(0));
            self.focus = Focus::Lists;
        }
    }

    pub fn select_task_visible(&mut self, vidx: usize) {
        if vidx < self.visible_task_indices().len() {
            self.task_state.select(Some(vidx));
            self.focus = Focus::Tasks;
        }
    }

    /// Enter key: drill from a list into its tasks, or open task detail.
    pub fn activate(&mut self) {
        match self.focus {
            Focus::Lists => {
                if !self.lists.is_empty() {
                    self.focus_tasks();
                }
            }
            Focus::Tasks => {
                if self.current_task().is_some() {
                    self.open_detail();
                }
            }
        }
    }

    pub fn open_detail(&mut self) {
        let has_subtasks = self
            .current_task()
            .map(|t| !t.subtasks.is_empty())
            .unwrap_or(false);
        self.subtask_state
            .select(if has_subtasks { Some(0) } else { None });
        self.mode = Mode::Detail;
    }

    pub fn close_overlay(&mut self) {
        self.mode = Mode::Normal;
    }

    // --- internal mutation helpers ------------------------------------------

    /// Save the list at `idx`, recording any error in the status line.
    fn save_list_at(&mut self, idx: usize) {
        let result = match self.lists.get(idx) {
            Some(list) => storage::save_list(&self.data_dir, list),
            None => return,
        };
        if let Err(e) = result {
            self.set_status(format!("save error: {e}"));
        }
    }

    /// Run `f` against the currently selected task and persist; returns false
    /// if there is no current task.
    fn with_current_task<F: FnOnce(&mut Task)>(&mut self, f: F) -> bool {
        let li = self.selected_list();
        let Some(ti) = self.current_task_index() else {
            return false;
        };
        if let Some(task) = self.lists.get_mut(li).and_then(|l| l.tasks.get_mut(ti)) {
            f(task);
            self.save_list_at(li);
            true
        } else {
            false
        }
    }

    // --- task / list actions -------------------------------------------------

    pub fn add_list(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let slug = storage::unique_slug(&self.data_dir, &name);
        let mut list = List::new(name);
        list.slug = slug.clone();
        if let Err(e) = storage::save_list(&self.data_dir, &list) {
            self.set_status(format!("save error: {e}"));
            return;
        }
        self.lists.push(list);
        self.sort_lists();
        // Select the newly added list (found by its unique slug after sorting).
        let pos = self.lists.iter().position(|l| l.slug == slug).unwrap_or(0);
        self.list_state.select(Some(pos));
        self.task_state.select(Some(0));
        self.focus = Focus::Lists;
    }

    /// Rename the currently selected list, keeping its on-disk file (slug) as-is.
    /// Re-sorts by name afterwards and keeps the renamed list selected.
    pub fn rename_current_list(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let li = self.selected_list();
        let slug = match self.lists.get_mut(li) {
            Some(list) => {
                list.name = name;
                list.slug.clone()
            }
            None => return,
        };
        self.save_list_at(li);
        self.sort_lists();
        // Re-select the renamed list (its slug is stable across the rename).
        let pos = self.lists.iter().position(|l| l.slug == slug).unwrap_or(li);
        self.list_state.select(Some(pos));
        self.task_state.select(Some(0));
        self.focus = Focus::Lists;
    }

    pub fn add_task(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let li = self.selected_list();
        if let Some(list) = self.lists.get_mut(li) {
            list.tasks.push(Task::new(title));
            self.save_list_at(li);
            // Select the new task if it is visible under the current filter.
            let visible = self.visible_task_indices();
            if let Some(pos) = visible
                .iter()
                .position(|&i| i + 1 == self.lists[li].tasks.len())
            {
                self.task_state.select(Some(pos));
            }
            self.focus = Focus::Tasks;
        } else {
            self.set_status("create a list first (A)".to_string());
        }
    }

    pub fn edit_current_task_title(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        self.with_current_task(|t| t.title = title);
    }

    pub fn toggle_current_done(&mut self) {
        self.with_current_task(|t| t.toggle_done());
    }

    pub fn cycle_current_priority(&mut self) {
        self.with_current_task(|t| t.cycle_priority());
    }

    pub fn set_current_due(&mut self, input: &str) {
        match model::parse_due(input, Self::today()) {
            Ok(due) => {
                self.with_current_task(|t| t.due = due);
            }
            Err(_) => {
                self.set_status("bad date — use YYYY-MM-DD, today, tomorrow, or +N".to_string());
            }
        }
    }

    pub fn set_current_tags(&mut self, input: &str) {
        let tags = model::parse_tags(input);
        self.with_current_task(|t| t.tags = tags);
    }

    pub fn set_current_notes(&mut self, notes: String) {
        self.with_current_task(|t| t.notes = notes);
    }

    pub fn delete_current_task(&mut self) {
        let li = self.selected_list();
        let Some(ti) = self.current_task_index() else {
            return;
        };
        if let Some(list) = self.lists.get_mut(li)
            && ti < list.tasks.len()
        {
            list.tasks.remove(ti);
            self.save_list_at(li);
            // Clamp visible selection.
            let visible = self.visible_task_indices().len();
            if visible == 0 {
                self.task_state.select(None);
            } else {
                let cur = self.selected_visible_task().min(visible - 1);
                self.task_state.select(Some(cur));
            }
        }
    }

    pub fn delete_current_list(&mut self) {
        let li = self.selected_list();
        if li >= self.lists.len() {
            return;
        }
        let list = self.lists.remove(li);
        if let Err(e) = storage::delete_list_file(&self.data_dir, &list) {
            self.set_status(format!("delete error: {e}"));
        }
        if self.lists.is_empty() {
            self.list_state.select(None);
            self.task_state.select(None);
        } else {
            let cur = li.min(self.lists.len() - 1);
            self.list_state.select(Some(cur));
            self.task_state.select(Some(0));
        }
        self.focus = Focus::Lists;
    }

    // --- archive -------------------------------------------------------------

    /// Index of the Archived list, if it has been created yet.
    fn archive_index(&self) -> Option<usize> {
        self.lists.iter().position(|l| l.is_archive())
    }

    /// Sort user lists by name (case-insensitive) and pin the Archived list to
    /// the bottom. A no-op ordering-wise when no archive exists.
    fn sort_lists(&mut self) {
        self.lists.sort_by(|a, b| match (a.is_archive(), b.is_archive()) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
    }

    /// Create the Archived list (on disk and in memory) if it doesn't exist yet.
    /// Appends it, so existing list indices stay valid; it's created lazily on
    /// the first archive rather than cluttering every new install.
    fn ensure_archive(&mut self) {
        if self.data_dir.as_os_str().is_empty() || self.archive_index().is_some() {
            return;
        }
        let mut list = List::new(model::ARCHIVE_NAME);
        list.slug = model::ARCHIVE_SLUG.to_string();
        if let Err(e) = storage::save_list(&self.data_dir, &list) {
            self.set_status(format!("save error: {e}"));
            return;
        }
        self.lists.push(list);
    }

    /// `d` on a task: move the selected task into the Archived list (creating it
    /// on first use) and persist both. Reversible via `m`, so no confirm. No-op
    /// when there's no current task or it already lives in the archive.
    pub fn archive_current_task(&mut self) {
        let src = self.selected_list();
        if self.lists.get(src).is_some_and(|l| l.is_archive()) {
            self.set_status("already archived — X deletes, m moves it out".to_string());
            return;
        }
        let Some(ti) = self.current_task_index() else {
            return;
        };
        self.ensure_archive();
        let Some(dest) = self.archive_index() else {
            return;
        };
        let task = self.lists[src].tasks.remove(ti);
        let title = task.title.clone();
        self.lists[dest].tasks.push(task);
        self.save_list_at(src);
        self.save_list_at(dest);
        // Clamp the source selection now that a task is gone.
        let visible = self.visible_task_indices().len();
        if visible == 0 {
            self.task_state.select(None);
        } else {
            let cur = self.selected_visible_task().min(visible - 1);
            self.task_state.select(Some(cur));
        }
        // The task left this list, so its detail view no longer applies.
        if self.in_detail() {
            self.mode = Mode::Normal;
        }
        self.set_status(format!("archived \"{title}\""));
    }

    /// The `d` key: archive the selected task, but fall back to permanent delete
    /// (with confirm) for a list or subtask, which have no archive.
    pub fn delete_or_archive(&mut self) {
        let deleting_subtask = self.in_detail() && self.selected_subtask().is_some();
        let deleting_list = !self.in_detail() && self.focus == Focus::Lists;
        if deleting_subtask || deleting_list {
            self.start_delete();
        } else {
            self.archive_current_task();
        }
    }

    // --- reorder tasks within a list -----------------------------------------

    /// Swap the selected task with the visible task `delta` slots away
    /// (-1 = up, +1 = down) and persist. Works in terms of the visible list so
    /// it stays intuitive under a filter; a no-op at the ends, off the task
    /// pane, or in a task's detail view.
    pub fn reorder_task(&mut self, delta: isize) {
        if self.focus != Focus::Tasks || self.in_detail() {
            return;
        }
        let vis = self.visible_task_indices();
        let v = self.selected_visible_task();
        let target = v as isize + delta;
        if target < 0 || target >= vis.len() as isize {
            return;
        }
        let target = target as usize;
        let li = self.selected_list();
        let Some(list) = self.lists.get_mut(li) else {
            return;
        };
        list.tasks.swap(vis[v], vis[target]);
        self.save_list_at(li);
        self.task_state.select(Some(target));
    }

    /// Move the selected task to the first (`to_top`) or last visible position
    /// and persist. A no-op with fewer than two visible tasks, off the task
    /// pane, or in detail view.
    pub fn send_task(&mut self, to_top: bool) {
        if self.focus != Focus::Tasks || self.in_detail() {
            return;
        }
        let vis = self.visible_task_indices();
        if vis.len() < 2 {
            return;
        }
        let from = vis[self.selected_visible_task()];
        // Insertion point (raw index) once `from` is spliced out: the current
        // first visible slot for a hoist, the current last for a sink. Both stay
        // valid after the removal because `from` lies within [first, last].
        let dest = if to_top { vis[0] } else { *vis.last().unwrap() };
        let li = self.selected_list();
        let Some(list) = self.lists.get_mut(li) else {
            return;
        };
        let task = list.tasks.remove(from);
        list.tasks.insert(dest, task);
        self.save_list_at(li);
        self.task_state
            .select(Some(if to_top { 0 } else { vis.len() - 1 }));
    }

    // --- move a task to another list -----------------------------------------

    /// Open the picker to move the selected task elsewhere. No-op when there is
    /// no current task; sets a hint when there is no other list to move to.
    pub fn start_move_task(&mut self) {
        if self.current_task().is_none() {
            return;
        }
        let src = self.selected_list();
        // Archiving has its own key (`d`), so keep the Archived list out of the
        // move picker. Moving *out* of it (to a real list) is how you unarchive.
        let targets: Vec<usize> = (0..self.lists.len())
            .filter(|&i| i != src && !self.lists[i].is_archive())
            .collect();
        if targets.is_empty() {
            self.set_status("no other list to move to".to_string());
            return;
        }
        self.mode = Mode::MovePicker(MovePickerState {
            targets,
            selected: 0,
            return_detail: self.in_detail(),
        });
    }

    /// Move the picker highlight (wraps).
    pub fn move_picker_move(&mut self, delta: isize) {
        if let Mode::MovePicker(state) = &mut self.mode {
            let n = state.targets.len() as isize;
            if n == 0 {
                return;
            }
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// Splice the selected task out of its list and onto the end of the
    /// highlighted one, persisting both. The task leaves this list, so we always
    /// return to Normal (its detail view no longer applies here).
    pub fn move_picker_confirm(&mut self) {
        let target = match &self.mode {
            Mode::MovePicker(state) => state.targets.get(state.selected).copied(),
            _ => return,
        };
        let src = self.selected_list();
        let ti = self.current_task_index();
        let (Some(target), Some(ti)) = (target, ti) else {
            self.mode = Mode::Normal;
            return;
        };
        let task = self.lists[src].tasks.remove(ti);
        let title = task.title.clone();
        let dest = self.lists[target].name.clone();
        self.lists[target].tasks.push(task);
        self.save_list_at(src);
        self.save_list_at(target);
        // Clamp the source selection now that a task is gone.
        let visible = self.visible_task_indices().len();
        if visible == 0 {
            self.task_state.select(None);
        } else {
            let cur = self.selected_visible_task().min(visible - 1);
            self.task_state.select(Some(cur));
        }
        self.set_status(format!("moved \"{title}\" to {dest}"));
        self.mode = Mode::Normal;
    }

    /// Close the picker without moving anything.
    pub fn move_picker_cancel(&mut self) {
        if let Mode::MovePicker(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    // --- copy a task to the clipboard ----------------------------------------

    /// Open the copy menu for the selected task. No-op when there is no current
    /// task (mirrors how the move picker behaves).
    pub fn start_copy(&mut self) {
        if self.current_task().is_none() {
            return;
        }
        self.mode = Mode::CopyMenu(CopyMenuState {
            selected: 0,
            return_detail: self.in_detail(),
        });
    }

    /// Move the copy-menu highlight (wraps).
    pub fn copy_menu_move(&mut self, delta: isize) {
        if let Mode::CopyMenu(state) = &mut self.mode {
            let n = CopyWhat::all().len() as isize;
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// The text that copying `what` would place on the clipboard, built from the
    /// currently selected task. Pure and side-effect free; the actual clipboard
    /// write lives in [`App::copy_selected`].
    pub fn copy_payload(&self, what: CopyWhat) -> Option<String> {
        let task = self.current_task()?;
        match what {
            CopyWhat::Json => serde_json::to_string_pretty(task).ok(),
            CopyWhat::Title => Some(task.title.clone()),
            CopyWhat::Notes => Some(task.notes.clone()),
        }
    }

    /// Copy the highlighted menu item to the clipboard.
    pub fn copy_menu_confirm(&mut self) {
        if let Mode::CopyMenu(state) = &self.mode {
            self.copy_selected(CopyWhat::all()[state.selected]);
        }
    }

    /// Copy `what` from the selected task to the clipboard and close the menu.
    /// Shared by the Enter-confirm path and the 1/2/3 quick keys.
    pub fn copy_selected(&mut self, what: CopyWhat) {
        let return_detail = match &self.mode {
            Mode::CopyMenu(state) => state.return_detail,
            _ => return,
        };
        self.mode = if return_detail {
            Mode::Detail
        } else {
            Mode::Normal
        };
        let Some(text) = self.copy_payload(what) else {
            self.set_status("nothing to copy".to_string());
            return;
        };
        // Don't clobber the clipboard with an empty description.
        if what == CopyWhat::Notes && text.trim().is_empty() {
            self.set_status("task has no description to copy".to_string());
            return;
        }
        match crate::clipboard::copy(&text) {
            Ok(()) => self.set_status(format!("copied {} to clipboard", what.noun())),
            Err(e) => self.set_status(format!("copy failed: {e}")),
        }
    }

    /// Close the copy menu without copying anything.
    pub fn copy_menu_cancel(&mut self) {
        if let Mode::CopyMenu(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    // --- subtask actions -----------------------------------------------------

    pub fn add_subtask(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let added = self.with_current_task(|t| t.subtasks.push(Subtask::new(title)));
        if added {
            let len = self.current_task().map(|t| t.subtasks.len()).unwrap_or(0);
            if len > 0 {
                self.subtask_state.select(Some(len - 1));
            }
        }
    }

    pub fn edit_current_subtask(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let Some(si) = self.selected_subtask() else {
            return;
        };
        self.with_current_task(|t| {
            if let Some(s) = t.subtasks.get_mut(si) {
                s.title = title;
            }
        });
    }

    pub fn toggle_current_subtask(&mut self) {
        let Some(si) = self.selected_subtask() else {
            return;
        };
        self.with_current_task(|t| {
            if let Some(s) = t.subtasks.get_mut(si) {
                s.done = !s.done;
            }
        });
    }

    pub fn delete_current_subtask(&mut self) {
        let Some(si) = self.selected_subtask() else {
            return;
        };
        self.with_current_task(|t| {
            if si < t.subtasks.len() {
                t.subtasks.remove(si);
            }
        });
        let len = self.current_task().map(|t| t.subtasks.len()).unwrap_or(0);
        if len == 0 {
            self.subtask_state.select(None);
        } else {
            let cur = self.subtask_state.selected().unwrap_or(0).min(len - 1);
            self.subtask_state.select(Some(cur));
        }
    }

    // --- filter --------------------------------------------------------------

    pub fn cycle_status_filter(&mut self) {
        self.filter.status.0 = self.filter.status.0.next();
        self.clamp_task_selection();
    }

    pub fn set_search(&mut self, query: String) {
        self.filter.query = query;
        self.clamp_task_selection();
    }

    pub fn clear_filter(&mut self) {
        self.filter = Filter::default();
        self.clamp_task_selection();
    }

    fn clamp_task_selection(&mut self) {
        let len = self.visible_task_indices().len();
        if len == 0 {
            self.task_state.select(None);
        } else {
            let cur = self.selected_visible_task().min(len - 1);
            self.task_state.select(Some(cur));
        }
    }

    // --- starting input/confirm modes ---------------------------------------

    fn in_detail(&self) -> bool {
        matches!(self.mode, Mode::Detail)
    }

    pub fn start_add_list(&mut self) {
        self.mode = Mode::Input(InputState::new(
            InputField::NewList,
            "New list name",
            String::new(),
            false,
            false,
        ));
    }

    pub fn start_add_task(&mut self) {
        // The Archived list doesn't count: you need a real list to add tasks to.
        if !self.lists.iter().any(|l| !l.is_archive()) {
            self.set_status("create a list first (A)".to_string());
            return;
        }
        self.mode = Mode::Input(InputState::new(
            InputField::NewTask,
            "New task",
            String::new(),
            false,
            false,
        ));
    }

    /// Open the input to rename the selected list (prefilled with its name).
    pub fn start_rename_list(&mut self) {
        let Some(list) = self.current_list() else {
            return;
        };
        if list.is_archive() {
            self.set_status("the Archived list can't be renamed".to_string());
            return;
        }
        let name = list.name.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::RenameList,
            "Rename list",
            name,
            false,
            false,
        ));
    }

    pub fn start_edit(&mut self) {
        // With the lists pane focused (and not in a task's detail view), `e`
        // renames the list rather than editing a task title.
        if !self.in_detail() && self.focus == Focus::Lists {
            self.start_rename_list();
            return;
        }
        if self.in_detail()
            && let Some(si) = self.selected_subtask()
        {
            let title = self
                .current_task()
                .and_then(|t| t.subtasks.get(si))
                .map(|s| s.title.clone())
                .unwrap_or_default();
            self.mode = Mode::Input(InputState::new(
                InputField::EditSubtask,
                "Edit subtask",
                title,
                false,
                true,
            ));
            return;
        }
        let Some(task) = self.current_task() else {
            return;
        };
        let title = task.title.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::EditTask,
            "Edit task",
            title,
            false,
            self.in_detail(),
        ));
    }

    pub fn start_set_due(&mut self) {
        let Some(task) = self.current_task() else {
            return;
        };
        let buffer = task.due.map(|d| d.to_string()).unwrap_or_default();
        self.mode = Mode::Input(InputState::new(
            InputField::Due,
            "Due (YYYY-MM-DD, today, tomorrow, +N; empty clears)",
            buffer,
            false,
            self.in_detail(),
        ));
    }

    pub fn start_set_tags(&mut self) {
        let Some(task) = self.current_task() else {
            return;
        };
        let buffer = task.tags.join(", ");
        self.mode = Mode::Input(InputState::new(
            InputField::Tags,
            "Tags (comma or space separated; empty clears)",
            buffer,
            false,
            self.in_detail(),
        ));
    }

    pub fn start_set_notes(&mut self) {
        let Some(task) = self.current_task() else {
            return;
        };
        let buffer = task.notes.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::Notes,
            "Notes (Enter = newline, Ctrl+S save, Esc cancel)",
            buffer,
            true,
            self.in_detail(),
        ));
    }

    pub fn start_add_subtask(&mut self) {
        if self.current_task().is_none() {
            return;
        }
        if !self.in_detail() {
            self.open_detail();
        }
        self.mode = Mode::Input(InputState::new(
            InputField::NewSubtask,
            "New subtask",
            String::new(),
            false,
            true,
        ));
    }

    pub fn start_search(&mut self) {
        self.mode = Mode::Input(InputState::new(
            InputField::Search,
            "Search (title, tags, notes)",
            self.filter.query.clone(),
            false,
            self.in_detail(),
        ));
    }

    pub fn start_delete(&mut self) {
        if self.in_detail()
            && let Some(si) = self.selected_subtask()
        {
            let title = self
                .current_task()
                .and_then(|t| t.subtasks.get(si))
                .map(|s| s.title.clone())
                .unwrap_or_default();
            self.mode = Mode::Confirm(ConfirmState {
                prompt: format!("Delete subtask \"{title}\"?"),
                action: ConfirmAction::DeleteSubtask,
                return_detail: true,
            });
            return;
        }
        match self.focus {
            Focus::Lists => {
                let Some(list) = self.current_list() else {
                    return;
                };
                if list.is_archive() {
                    self.set_status("the Archived list can't be deleted".to_string());
                    return;
                }
                let name = list.name.clone();
                let n = list.tasks.len();
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: format!("Delete list \"{name}\" and its {n} task(s)?"),
                    action: ConfirmAction::DeleteList,
                    return_detail: false,
                });
            }
            Focus::Tasks => {
                let Some(task) = self.current_task() else {
                    return;
                };
                let title = task.title.clone();
                self.mode = Mode::Confirm(ConfirmState {
                    prompt: format!("Permanently delete \"{title}\"?"),
                    action: ConfirmAction::DeleteTask,
                    return_detail: self.in_detail(),
                });
            }
        }
    }

    /// Toggle done for whatever is selected given the current mode.
    pub fn toggle_selected(&mut self) {
        if self.in_detail() && self.selected_subtask().is_some() {
            self.toggle_current_subtask();
        } else {
            self.toggle_current_done();
        }
    }

    // --- first run -----------------------------------------------------------

    pub fn commit_first_run(&mut self, dir: PathBuf) {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.set_status(format!("could not create {}: {e}", dir.display()));
            return;
        }
        let config = Config {
            data_dir: dir.clone(),
            theme: self.theme,
        };
        if let Err(e) = config::save_config(&config) {
            self.set_status(format!("could not save config: {e}"));
            return;
        }
        self.data_dir = dir;
        match storage::load_lists(&self.data_dir) {
            Ok(lists) => self.lists = lists,
            Err(e) => {
                self.set_status(format!("load error: {e}"));
                self.lists = Vec::new();
            }
        }
        if self.lists.is_empty() {
            let mut list = List::new("Tasks");
            list.slug = "tasks".to_string();
            let _ = storage::save_list(&self.data_dir, &list);
            self.lists.push(list);
        }
        self.sort_lists();
        self.list_state.select(Some(0));
        self.task_state.select(Some(0));
        self.focus = Focus::Lists;
        self.mode = Mode::Normal;
    }
}
