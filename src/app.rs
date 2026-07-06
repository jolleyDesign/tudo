//! Application state and all the actions that mutate it.
//!
//! `App` owns everything. Its methods are deliberately terminal-free so they can
//! be unit-tested directly; `ui.rs` renders from `&mut App` and `event.rs` maps
//! input to these methods.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{Local, Utc};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

/// How long a transient status-line message stays on screen before clearing.
pub const STATUS_TTL: Duration = Duration::from_secs(4);

use crate::config::{self, Config};
use crate::keybind::Keymap;
use crate::model::{self, List, Note, NoteSort, Notebook, Sort, SortDir, Subtask, Task};
use crate::storage;
use crate::theme::{self, ThemeKind};

/// Which pane currently has keyboard focus. The sidebar (lists *and* tags) is a
/// single focus; which of its two panes is active is derived from the selected
/// sidebar row (see [`App::selected_row`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Lists,
    Tasks,
}

/// The top-level workspace shown in the header tab strip. Session-only, like the
/// sort/filter — every run starts on `Tasks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tasks,
    Notes,
}

impl Tab {
    /// The tabs in the order they appear (and are numbered) in the header.
    pub fn all() -> [Tab; 2] {
        [Tab::Tasks, Tab::Notes]
    }

    pub fn label(self) -> &'static str {
        match self {
            Tab::Tasks => "Tasks",
            Tab::Notes => "Notes",
        }
    }
}

/// Which pane has focus on the Notes tab (parallel to [`Focus`] on the Tasks
/// tab). The notebooks pane and the note-tags pane share one sidebar cursor, so
/// `Notebooks` covers both — which is active is derived from the selected row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesFocus {
    Notebooks,
    Notes,
}

/// A reference to a note by its home notebook and position within it, so
/// mutations land on the right note even in the cross-notebook tag view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteRef {
    /// Index into [`App::notebooks`].
    pub notebook: usize,
    /// Index into that notebook's `notes`.
    pub note: usize,
}

/// One entry in the Notes sidebar's flat selection order: every notebook first
/// (so `Notebook(i)` sits at index `i`), then one `Tag` per distinct note tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteSidebarRow {
    Notebook(usize),
    Tag(String),
}

/// What the note list is showing, derived from the selected Notes-sidebar row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteView {
    Notebook(usize),
    Tag(String),
}

/// A stable identity for a selected Notes-sidebar row, so the selection can be
/// re-found after an edit reshuffles the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NoteEntryKey {
    Notebook(String),
    Tag(String),
}

/// A reference to a task by its home list and position within that list. Used
/// whenever the visible task set can span lists — the cross-list tag view — so
/// that mutations still land on the right list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRef {
    /// Index into [`App::lists`].
    pub list: usize,
    /// Index into that list's `tasks`.
    pub task: usize,
}

/// One entry in the sidebar's flat selection order: every list first (so
/// `List(i)` always sits at index `i`), then one `Tag` per distinct tag in use.
/// The two are rendered in separate panes (see `ui::render_sidebar`), but share
/// this single cursor so `j`/`k` flow from the last list into the tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRow {
    List(usize),
    Tag(String),
}

/// What the tasks pane is showing, derived from the selected sidebar row: a
/// single list, or every task carrying a tag across all lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    List(usize),
    Tag(String),
}

/// A stable identity for a selected sidebar row (lists by slug, tags by name),
/// so the selection can be re-found after an edit reshuffles the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryKey {
    List(String),
    Tag(String),
}

/// Which text field an [`InputState`] is collecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    NewList,
    RenameList,
    NewTask,
    EditTask,
    Tags,
    Points,
    Due,
    Notes,
    NewSubtask,
    EditSubtask,
    Search,
    DataDir,
    // Notes tab.
    NewNotebook,
    RenameNotebook,
    NewNote,
    EditNote,
    NoteBody,
    NoteTags,
    NoteSearch,
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

    /// Forward-delete the character under the cursor (the `Delete` key).
    pub fn delete(&mut self) {
        let n = self.buffer.chars().count();
        if self.cursor < n {
            let start = self.byte_at(self.cursor);
            let end = self.byte_at(self.cursor + 1);
            self.buffer.replace_range(start..end, "");
        }
    }

    /// Move the cursor up one logical line, keeping the column where possible.
    /// Only meaningful for multiline inputs.
    pub fn up(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let cur_start = line_start_of(&chars, self.cursor);
        if cur_start == 0 {
            self.cursor = 0;
            return;
        }
        let col = self.cursor - cur_start;
        let prev_start = line_start_of(&chars, cur_start - 1);
        let prev_len = (cur_start - 1) - prev_start;
        self.cursor = prev_start + col.min(prev_len);
    }

    /// Move the cursor down one logical line, keeping the column where possible.
    pub fn down(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let n = chars.len();
        let cur_start = line_start_of(&chars, self.cursor);
        let col = self.cursor - cur_start;
        let cur_end = (cur_start..n).find(|&i| chars[i] == '\n').unwrap_or(n);
        if cur_end == n {
            self.cursor = n;
            return;
        }
        let next_start = cur_end + 1;
        let next_end = (next_start..n).find(|&i| chars[i] == '\n').unwrap_or(n);
        self.cursor = next_start + col.min(next_end - next_start);
    }
}

/// Character index of the start of the logical line containing `cursor`.
fn line_start_of(chars: &[char], cursor: usize) -> usize {
    let mut start = 0;
    for (i, &c) in chars.iter().enumerate().take(cursor) {
        if c == '\n' {
            start = i + 1;
        }
    }
    start
}

/// What a pending [`Mode::Confirm`] will do if accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum ConfirmAction {
    DeleteTask,
    DeleteList,
    DeleteSubtask,
    DeleteNote,
    DeleteNotebook,
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
    /// Type-to-filter query (case-insensitive substring match on list name).
    pub filter: String,
    /// Cursor into the *filtered* target list.
    pub selected: usize,
    /// First visible filtered row (vertical scroll offset).
    pub scroll: u16,
    /// Return to Detail mode (vs Normal) on cancel.
    pub return_detail: bool,
}

/// State for the full-screen rendered note read view.
#[derive(Debug, Clone, Default)]
pub struct NoteViewState {
    /// Vertical scroll offset in rendered (wrapped) rows.
    pub scroll: u16,
}

/// One entry in the note link picker: a display label and where it jumps.
#[derive(Debug, Clone)]
pub struct NoteLinkEntry {
    pub label: String,
    pub target: NoteRef,
}

/// State for the "follow a link" picker (outgoing links + backlinks).
#[derive(Debug, Clone)]
pub struct NoteLinkPickerState {
    pub entries: Vec<NoteLinkEntry>,
    pub selected: usize,
    /// Return to the read view (vs the note list) on cancel.
    pub return_view: bool,
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

/// What part of the selected note a copy action puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteCopyWhat {
    /// The markdown body (no frontmatter).
    Body,
    /// Just the note title.
    Title,
    /// The whole note file, frontmatter included.
    Markdown,
}

impl NoteCopyWhat {
    pub fn all() -> [NoteCopyWhat; 3] {
        [
            NoteCopyWhat::Body,
            NoteCopyWhat::Title,
            NoteCopyWhat::Markdown,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            NoteCopyWhat::Body => "Body (markdown)",
            NoteCopyWhat::Title => "Title",
            NoteCopyWhat::Markdown => "Full note (with frontmatter)",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            NoteCopyWhat::Body => "body",
            NoteCopyWhat::Title => "title",
            NoteCopyWhat::Markdown => "note markdown",
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

/// State for the "sort tasks" picker overlay.
#[derive(Debug, Clone)]
pub struct SortPickerState {
    /// Cursor into [`Sort::all`].
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
    SortPicker(SortPickerState),
    NoteSortPicker(SortPickerState),
    NoteMovePicker(MovePickerState),
    NoteCopyMenu(CopyMenuState),
    /// Full-screen rendered read view of the selected note.
    NoteView(NoteViewState),
    /// Picker to follow a `[[link]]` (outgoing or backlink).
    NoteLinkPicker(NoteLinkPickerState),
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
    /// Inner area of the tags pane and the scroll offset used to draw it. `None`
    /// when no tags are in use (the sidebar is then a single Lists pane).
    pub tags_inner: Option<(Rect, usize)>,
    /// Inner area of the tasks pane and the scroll offset used to draw it.
    pub tasks_inner: Option<(Rect, usize)>,
    /// Inner area of the notebooks pane and its scroll offset (Notes tab).
    pub notebooks_inner: Option<(Rect, usize)>,
    /// Inner area of the note-tags pane and its scroll offset (Notes tab).
    pub note_tags_inner: Option<(Rect, usize)>,
    /// Inner area of the note-list pane and its scroll offset (Notes tab).
    pub note_list_inner: Option<(Rect, usize)>,
    /// Header tab-strip chips and the tab each selects, for click-to-switch.
    pub tab_rects: Vec<(Tab, Rect)>,
    /// Clickable `[[link]]` rects in the rendered read view and their targets.
    pub note_view_links: Vec<(Rect, NoteRef)>,
}

pub struct App {
    pub data_dir: PathBuf,
    pub lists: Vec<List>,
    pub mode: Mode,
    /// The active top-level tab (Tasks / Notes). Session-only.
    pub active_tab: Tab,
    pub focus: Focus,
    pub list_state: ListState,
    pub task_state: ListState,
    pub subtask_state: ListState,
    pub filter: Filter,
    /// How the visible task list is ordered (session-only; see [`Sort`]).
    pub sort: Sort,
    /// Direction the active `sort` is applied in (session-only; see [`SortDir`]).
    pub sort_dir: SortDir,
    /// When set, completed tasks are shown at the bottom of the list regardless
    /// of the active [`Sort`]. Display-only (the stored order is untouched) and
    /// persisted in the config, unlike the session-only `sort`.
    pub completed_to_bottom: bool,
    pub theme: ThemeKind,
    /// The resolved keybindings (built-in defaults plus any config overrides).
    pub keymap: Keymap,
    pub status: String,
    /// When the current `status` message should auto-clear (`None` = nothing showing).
    status_expiry: Option<Instant>,
    pub should_quit: bool,
    pub clickables: Clickables,
    /// Remembered sidebar cursor within each section (a list index, and a
    /// tag index among the tags), so jumping to the lists (`h`) or the tags
    /// (`#`) restores your place in that section.
    last_list_row: usize,
    last_tag_row: usize,

    // --- Notes tab -----------------------------------------------------------
    pub notebooks: Vec<Notebook>,
    /// Flat cursor over the Notes sidebar (notebooks then note tags), mirroring
    /// `list_state` on the Tasks tab.
    pub notebook_state: ListState,
    pub note_state: ListState,
    pub notes_focus: NotesFocus,
    /// Substring search over notes (Notes-tab counterpart to `filter`).
    pub note_query: String,
    pub note_sort: NoteSort,
    pub note_sort_dir: SortDir,
    last_notebook_row: usize,
    last_note_tag_row: usize,
    /// When set, the run loop suspends the TUI and opens this file in `$EDITOR`.
    pub pending_editor: Option<PathBuf>,
}

impl App {
    /// Build the app. `data_dir = None` starts the first-run picker. The active
    /// theme defaults to Tokyo Night; call [`App::set_theme`] to change it.
    pub fn new(data_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let (data_dir, lists, notebooks, mode) = match data_dir {
            Some(dir) => {
                let lists = storage::load_lists(&dir)?;
                let notebooks = storage::load_notebooks(&dir)?;
                (dir, lists, notebooks, Mode::Normal)
            }
            None => (
                PathBuf::new(),
                Vec::new(),
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
            active_tab: Tab::Tasks,
            focus: Focus::Lists,
            list_state: ListState::default(),
            task_state: ListState::default(),
            subtask_state: ListState::default(),
            filter: Filter::default(),
            sort: Sort::default(),
            sort_dir: SortDir::default(),
            completed_to_bottom: false,
            theme,
            keymap: Keymap::default(),
            status: String::new(),
            status_expiry: None,
            should_quit: false,
            clickables: Clickables::default(),
            last_list_row: 0,
            last_tag_row: 0,
            notebooks,
            notebook_state: ListState::default(),
            note_state: ListState::default(),
            notes_focus: NotesFocus::Notebooks,
            note_query: String::new(),
            note_sort: NoteSort::default(),
            note_sort_dir: SortDir::default(),
            last_notebook_row: 0,
            last_note_tag_row: 0,
            pending_editor: None,
        };

        // Pin any previously-created Archived list to the bottom, then select
        // the first list. (First run has no data dir yet, so skip it.)
        if !matches!(app.mode, Mode::FirstRun(_)) {
            app.sort_lists();
            if !app.lists.is_empty() {
                app.list_state.select(Some(0));
                app.task_state.select(Some(0));
            }
            app.sort_notebooks();
            if !app.notebooks.is_empty() {
                app.notebook_state.select(Some(0));
                app.note_state.select(Some(0));
            }
        }

        Ok(app)
    }

    /// Set the active theme without persisting (used at startup).
    pub fn set_theme(&mut self, kind: ThemeKind) {
        self.theme = kind;
        theme::set(kind.theme());
    }

    /// Install the resolved keybindings (used at startup, after config load).
    pub fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
    }

    /// Set the "completed tasks sink to the bottom" preference without
    /// persisting (used at startup, after config load).
    pub fn set_completed_to_bottom(&mut self, on: bool) {
        self.completed_to_bottom = on;
    }

    /// Flip the "completed tasks sink to the bottom" preference and persist it.
    /// Reversible: turning it off restores the stored order, since the setting
    /// only affects display (see [`App::visible_task_indices`]).
    pub fn toggle_completed_to_bottom(&mut self) {
        self.completed_to_bottom = !self.completed_to_bottom;
        self.persist_config();
        let state = if self.completed_to_bottom {
            "on"
        } else {
            "off"
        };
        self.set_status(format!("completed tasks to bottom: {state}"));
    }

    /// Fill an existing config file in with every keybinding (defaults plus any
    /// overrides) when some are missing, so the user can see and edit them all.
    /// Won't create a config that doesn't exist yet, and skips one that already
    /// lists every action. Call at startup after [`App::set_theme`] /
    /// [`App::set_keymap`] so the persisted theme is correct.
    pub fn materialize_keybindings(&mut self) {
        if self.data_dir.as_os_str().is_empty() || self.keymap.covers_all_actions() {
            return;
        }
        let config_exists = config::config_path().map(|p| p.exists()).unwrap_or(false);
        if config_exists {
            self.persist_config();
        }
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
        // Then the notebooks and their notes.
        let notebooks = self.notebooks.clone();
        for nb in &notebooks {
            if let Err(e) = storage::create_notebook_dir(&new, &nb.slug) {
                self.set_status(format!("move failed: {e}"));
                return;
            }
            for note in &nb.notes {
                if let Err(e) = storage::save_note(&new, &nb.slug, note) {
                    self.set_status(format!("move failed: {e}"));
                    return;
                }
            }
        }
        // Preserve the manual notebook order in the new location.
        let ordered: Vec<String> = notebooks
            .iter()
            .filter(|n| !n.is_archive())
            .map(|n| n.slug.clone())
            .collect();
        let _ = storage::save_notebook_order(&new, &ordered);
        // Then remove the originals.
        for list in &lists {
            let _ = storage::delete_list_file(&old, list);
        }
        for nb in &notebooks {
            let _ = storage::delete_notebook_dir(&old, &nb.slug);
        }
        self.data_dir = new.clone();
        match storage::load_lists(&new) {
            Ok(l) => self.lists = l,
            Err(e) => self.set_status(format!("load error: {e}")),
        }
        self.notebooks = storage::load_notebooks(&new).unwrap_or_default();
        self.reselect_notebook(None);
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
            keybindings: self.keymap.to_full_overrides(),
            completed_to_bottom: self.completed_to_bottom,
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

    /// The sidebar's flat selection order: every list first (so `List(i)` sits
    /// at index `i`), then one row per distinct tag. Rendered as two separate
    /// panes but navigated as one sequence.
    pub fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let mut rows: Vec<SidebarRow> = (0..self.lists.len()).map(SidebarRow::List).collect();
        rows.extend(self.all_tags().into_iter().map(SidebarRow::Tag));
        rows
    }

    /// Every distinct tag in use across the real (non-archived) lists, sorted
    /// alphabetically. Archived tasks are excluded — matching the cross-list tag
    /// view — so a tag whose only tasks are archived drops out of the sidebar.
    pub fn all_tags(&self) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for list in &self.lists {
            if list.is_archive() {
                continue;
            }
            for task in &list.tasks {
                for tag in &task.tags {
                    set.insert(tag.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// `(open, total)` task counts for a tag across the real lists (archived
    /// excluded), mirroring how list rows show their own counts.
    pub fn tag_counts(&self, name: &str) -> (usize, usize) {
        let mut open = 0;
        let mut total = 0;
        for list in &self.lists {
            if list.is_archive() {
                continue;
            }
            for task in &list.tasks {
                if task.tags.iter().any(|t| t == name) {
                    total += 1;
                    open += usize::from(!task.done);
                }
            }
        }
        (open, total)
    }

    /// The sidebar row currently selected, if any.
    pub fn selected_row(&self) -> Option<SidebarRow> {
        self.sidebar_rows()
            .into_iter()
            .nth(self.list_state.selected()?)
    }

    /// What the tasks pane is showing, derived from the selected sidebar row.
    /// `None` when nothing (or a header) is selected.
    pub fn current_view(&self) -> Option<View> {
        match self.selected_row()? {
            SidebarRow::List(i) => Some(View::List(i)),
            SidebarRow::Tag(name) => Some(View::Tag(name)),
        }
    }

    /// The selected list's index into [`App::lists`], or `None` when a tag row
    /// is selected. Use this for list-scoped actions (add/rename/delete/reorder).
    pub fn selected_list_index(&self) -> Option<usize> {
        match self.current_view()? {
            View::List(i) => Some(i),
            View::Tag(_) => None,
        }
    }

    /// The selected list index, falling back to 0. Kept for the common case and
    /// for callers/tests that predate the tag view; prefer
    /// [`selected_list_index`](Self::selected_list_index) when "no single list is
    /// selected" needs its own handling.
    pub fn selected_list(&self) -> usize {
        self.selected_list_index().unwrap_or(0)
    }

    /// Visible-index of the selected task (index into [`visible_task_refs`]).
    ///
    /// [`visible_task_refs`]: Self::visible_task_refs
    pub fn selected_visible_task(&self) -> usize {
        self.task_state.selected().unwrap_or(0)
    }

    pub fn current_list(&self) -> Option<&List> {
        self.lists.get(self.selected_list_index()?)
    }

    /// The tasks shown in the tasks pane, as `(list, task)` references in display
    /// order. In the list view every ref shares the selected list; in the tag
    /// view they span every real list. The active search/status filter and the
    /// sort (plus the completed-to-bottom preference) apply to both.
    ///
    /// `Sort::Manual` keeps the natural order — stored order within a list, and
    /// list-then-task order across the tag view — and any ties under the other
    /// sorts fall back to it.
    pub fn visible_task_refs(&self) -> Vec<TaskRef> {
        let mut refs: Vec<TaskRef> = match self.current_view() {
            Some(View::List(li)) => {
                let Some(list) = self.lists.get(li) else {
                    return Vec::new();
                };
                list.tasks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| self.filter.matches(t))
                    .map(|(task, _)| TaskRef { list: li, task })
                    .collect()
            }
            Some(View::Tag(tag)) => {
                let mut out = Vec::new();
                for (li, list) in self.lists.iter().enumerate() {
                    if list.is_archive() {
                        continue;
                    }
                    for (ti, task) in list.tasks.iter().enumerate() {
                        if task.tags.contains(&tag) && self.filter.matches(task) {
                            out.push(TaskRef { list: li, task: ti });
                        }
                    }
                }
                out
            }
            None => return Vec::new(),
        };
        self.sort_refs(&mut refs);
        refs
    }

    /// Order `refs` by the active sort/direction, then optionally float completed
    /// tasks to the bottom. Both passes are stable, so `Sort::Manual` is a no-op
    /// and preserves the natural (insertion) order.
    fn sort_refs(&self, refs: &mut [TaskRef]) {
        let task = |r: &TaskRef| &self.lists[r.list].tasks[r.task];
        if self.sort != Sort::Manual {
            refs.sort_by(|a, b| {
                let ord = self.sort.compare(task(a), task(b));
                match self.sort_dir {
                    SortDir::Ascending => ord,
                    SortDir::Descending => ord.reverse(),
                }
            });
        }
        if self.completed_to_bottom {
            refs.sort_by_key(|r| task(r).done);
        }
    }

    /// Task indices (into their list's `tasks`) of the visible tasks. Kept for
    /// the list view and for callers/tests that predate the tag view; there the
    /// returned indices point into different lists, so prefer
    /// [`visible_task_refs`](Self::visible_task_refs).
    pub fn visible_task_indices(&self) -> Vec<usize> {
        self.visible_task_refs().iter().map(|r| r.task).collect()
    }

    /// The `(list, task)` reference of the currently selected task.
    pub fn current_task_ref(&self) -> Option<TaskRef> {
        self.visible_task_refs()
            .get(self.selected_visible_task())
            .copied()
    }

    /// Raw index (into its list's `tasks`) of the selected task. See
    /// [`current_task_ref`](Self::current_task_ref) for the full reference.
    pub fn current_task_index(&self) -> Option<usize> {
        self.current_task_ref().map(|r| r.task)
    }

    pub fn current_task(&self) -> Option<&Task> {
        let r = self.current_task_ref()?;
        self.lists.get(r.list)?.tasks.get(r.task)
    }

    /// `(active, done, overdue)` task counts for the current scope — the selected
    /// list, or every task carrying the selected tag — for the header summary.
    pub fn header_stats(&self, today: chrono::NaiveDate) -> (usize, usize, usize) {
        let tally = |(a, d, o): (usize, usize, usize), t: &Task| {
            (
                a + usize::from(!t.done),
                d + usize::from(t.done),
                o + usize::from(t.is_overdue(today)),
            )
        };
        match self.current_view() {
            Some(View::List(li)) => match self.lists.get(li) {
                Some(l) => l.tasks.iter().fold((0, 0, 0), tally),
                None => (0, 0, 0),
            },
            Some(View::Tag(tag)) => self
                .lists
                .iter()
                .filter(|l| !l.is_archive())
                .flat_map(|l| l.tasks.iter())
                .filter(|t| t.tags.contains(&tag))
                .fold((0, 0, 0), tally),
            None => (0, 0, 0),
        }
    }

    /// A stable identity for the selected sidebar row, so it can be re-found
    /// after an edit adds or removes rows.
    fn selected_entry_key(&self) -> Option<EntryKey> {
        match self.selected_row()? {
            SidebarRow::List(li) => Some(EntryKey::List(self.lists[li].slug.clone())),
            SidebarRow::Tag(name) => Some(EntryKey::Tag(name)),
        }
    }

    /// Re-select the row identified by `key` after a mutation that may have added
    /// or removed rows (e.g. a tag appearing or disappearing). Falls back to the
    /// nearest selectable row when the entry is gone (a tag whose last task left),
    /// and to nothing when the sidebar is empty.
    fn reselect_entry(&mut self, key: Option<EntryKey>) {
        let rows = self.sidebar_rows();
        if rows.is_empty() {
            self.list_state.select(None);
            return;
        }
        let found = key.and_then(|k| {
            rows.iter().position(|row| match (&k, row) {
                (EntryKey::List(slug), SidebarRow::List(li)) => &self.lists[*li].slug == slug,
                (EntryKey::Tag(a), SidebarRow::Tag(b)) => a == b,
                _ => false,
            })
        });
        // Entry gone (e.g. a tag whose last task left): clamp the old position
        // into the new, shorter row range.
        let idx =
            found.unwrap_or_else(|| self.list_state.selected().unwrap_or(0).min(rows.len() - 1));
        self.list_state.select(Some(idx));
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

    /// Toggle keyboard focus between the sidebar and the tasks pane. The sidebar
    /// cursor stays put, so toggling back returns to the same list or tag — Tab
    /// from a selected list drops straight into that list's tasks.
    pub fn toggle_focus(&mut self) {
        match self.focus {
            Focus::Lists => self.focus_tasks(),
            Focus::Tasks => self.focus = Focus::Lists,
        }
    }

    /// Jump the sidebar cursor onto the tag rows, restoring the last tag you were
    /// on. A no-op when there are no tags.
    pub fn focus_tags(&mut self) {
        let n = self.all_tags().len();
        if n == 0 {
            return;
        }
        self.remember_sidebar_row();
        self.focus = Focus::Lists;
        let idx = self.lists.len() + self.last_tag_row.min(n - 1);
        self.set_sidebar_row_reset(idx);
    }

    /// Remember the current sidebar cursor within its section (a list or a tag),
    /// so a later `focus_lists` / `focus_tags` restores your place there.
    fn remember_sidebar_row(&mut self) {
        match self.selected_row() {
            Some(SidebarRow::List(i)) => self.last_list_row = i,
            Some(SidebarRow::Tag(_)) => {
                if let Some(sel) = self.list_state.selected() {
                    self.last_tag_row = sel.saturating_sub(self.lists.len());
                }
            }
            None => {}
        }
    }

    /// Select sidebar row `idx`, resetting the task cursor only when the row
    /// actually changes — so cycling away and back to the same list keeps your
    /// place in its task list.
    fn set_sidebar_row_reset(&mut self, idx: usize) {
        if self.list_state.selected() != Some(idx) {
            self.list_state.select(Some(idx));
            self.task_state.select(Some(0));
        }
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
        // One flat cursor runs through the lists then the tags, so `j`/`k` flow
        // straight from the last list into the tag pane.
        let len = self.sidebar_rows().len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as isize;
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

    /// Move focus to the lists, jumping the cursor back up off the tags (if it
    /// was there) to the last list you were on.
    pub fn focus_lists(&mut self) {
        self.remember_sidebar_row();
        self.focus = Focus::Lists;
        if self.lists.is_empty() {
            self.list_state.select(None);
        } else {
            let idx = self.last_list_row.min(self.lists.len() - 1);
            self.set_sidebar_row_reset(idx);
        }
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

    /// Select a sidebar row by its flat index (used by the mouse handler, which
    /// clicks either the lists pane or the tags pane). Out-of-range is ignored.
    pub fn select_sidebar_row(&mut self, idx: usize) {
        if idx < self.sidebar_rows().len() {
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
    /// if there is no current task. Works across the tag view because the task
    /// is located by its full `(list, task)` reference.
    fn with_current_task<F: FnOnce(&mut Task)>(&mut self, f: F) -> bool {
        let Some(r) = self.current_task_ref() else {
            return false;
        };
        if let Some(task) = self
            .lists
            .get_mut(r.list)
            .and_then(|l| l.tasks.get_mut(r.task))
        {
            f(task);
            self.save_list_at(r.list);
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

    pub fn add_task(&mut self, input: String) {
        // Quick-add: pull priority/tags/due/notes/subtasks out of the title text
        // (see `model::parse_quick_add`). Plain titles pass through unchanged.
        let parsed = model::parse_quick_add(&input, Self::today());
        if parsed.title.is_empty() {
            return;
        }
        // A task needs a home list; the tag view has no single one to add to.
        let Some(li) = self.selected_list_index() else {
            self.set_status("select a list to add a task".to_string());
            return;
        };
        if let Some(list) = self.lists.get_mut(li) {
            let mut task = Task::new(parsed.title);
            task.priority = parsed.priority;
            task.points = parsed.points;
            task.due = parsed.due;
            task.tags = parsed.tags;
            task.notes = parsed.notes;
            task.subtasks = parsed.subtasks.into_iter().map(Subtask::new).collect();
            list.tasks.push(task);
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
        // Editing tags can add or remove sidebar tag rows (and, in the tag view,
        // remove the current task from the visible set), so re-anchor both the
        // sidebar selection and the task cursor afterwards.
        let key = self.selected_entry_key();
        self.with_current_task(|t| t.tags = tags);
        self.reselect_entry(key);
        self.clamp_task_selection();
    }

    /// Set (or, on empty input, clear) the selected task's story points. A
    /// non-numeric entry is rejected with a status hint and leaves it unchanged.
    pub fn set_current_points(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            self.with_current_task(|t| t.points = None);
            return;
        }
        match trimmed.parse::<u16>() {
            Ok(points) => {
                self.with_current_task(|t| t.points = Some(points));
            }
            Err(_) => {
                self.set_status("points must be a whole number (empty clears)".to_string());
            }
        }
    }

    pub fn set_current_notes(&mut self, notes: String) {
        self.with_current_task(|t| t.notes = notes);
    }

    pub fn delete_current_task(&mut self) {
        let Some(r) = self.current_task_ref() else {
            return;
        };
        let key = self.selected_entry_key();
        if let Some(list) = self.lists.get_mut(r.list)
            && r.task < list.tasks.len()
        {
            list.tasks.remove(r.task);
            self.save_list_at(r.list);
        }
        // The removed task may have carried the last use of a tag; re-anchor the
        // sidebar, then clamp the task cursor into the new visible set.
        self.reselect_entry(key);
        self.clamp_task_selection();
    }

    pub fn delete_current_list(&mut self) {
        let Some(li) = self.selected_list_index() else {
            return;
        };
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

    /// Order the sidebar: the Archived list is always pinned to the bottom;
    /// the rest are sorted by their explicit `order`, with any unordered lists
    /// (a fresh install, or a list just created after a custom order exists)
    /// falling back to alphabetical after the ordered ones. With no custom
    /// order anywhere this is pure alphabetical, matching the original behaviour.
    fn sort_lists(&mut self) {
        self.lists
            .sort_by(|a, b| match (a.is_archive(), b.is_archive()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a
                    .order
                    .unwrap_or(u32::MAX)
                    .cmp(&b.order.unwrap_or(u32::MAX))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            });
    }

    /// Give every non-archive list an explicit `order` matching its current
    /// position and persist those whose stored value actually changed. Called
    /// after a reorder so the custom order survives a reload. The first reorder
    /// on a previously-alphabetical set bakes in everyone's position; later ones
    /// only touch the two lists that moved. The Archived list keeps its (unused)
    /// order — `sort_lists` pins it to the bottom regardless.
    fn renumber_lists(&mut self) {
        let mut changed = Vec::new();
        let mut next = 0u32;
        for (i, list) in self.lists.iter_mut().enumerate() {
            if list.is_archive() {
                continue;
            }
            if list.order != Some(next) {
                list.order = Some(next);
                changed.push(i);
            }
            next += 1;
        }
        for i in changed {
            self.save_list_at(i);
        }
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
        let Some(r) = self.current_task_ref() else {
            return;
        };
        if self.lists[r.list].is_archive() {
            self.set_status("already archived — X deletes, m moves it out".to_string());
            return;
        }
        let key = self.selected_entry_key();
        self.ensure_archive();
        let Some(dest) = self.archive_index() else {
            return;
        };
        let task = self.lists[r.list].tasks.remove(r.task);
        let title = task.title.clone();
        self.lists[dest].tasks.push(task);
        self.save_list_at(r.list);
        self.save_list_at(dest);
        // Archiving removes the task from every real list, so a tag it carried
        // may vanish; re-anchor the sidebar, then clamp the task cursor.
        self.reselect_entry(key);
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

    /// The `d` key: archive the selected task. Only tasks can be archived, so a
    /// list or subtask is deliberately *not* touched here — `d` only ever
    /// archives, never deletes, so it can't mean "delete" in one pane and
    /// "archive" in another. `X` is the single key that deletes anything (task,
    /// list, or subtask); both cases below point the user at it.
    pub fn archive_selection(&mut self) {
        let on_subtask = self.in_detail() && self.selected_subtask().is_some();
        let on_list = !self.in_detail() && self.focus == Focus::Lists;
        if on_subtask {
            self.set_status("subtasks can't be archived — X deletes a subtask".to_string());
        } else if on_list {
            self.set_status("lists can't be archived — X deletes a list".to_string());
        } else {
            self.archive_current_task();
        }
    }

    // --- reorder the current selection ---------------------------------------

    /// Manual task reordering only makes sense under [`Sort::Manual`]: with any
    /// other sort the visible order is derived from a task field, so a swap
    /// wouldn't stick. Returns `true` (after setting a hint) when a task reorder
    /// should be refused. List reordering is unaffected — lists have their own
    /// order independent of the task sort.
    fn reorder_blocked_by_sort(&mut self) -> bool {
        if self.sort != Sort::Manual {
            self.set_status("reordering needs manual sort (press o)".to_string());
            true
        } else {
            false
        }
    }

    /// Focus-aware reorder: nudge the selected list or task by `delta` slots
    /// (-1 = up, +1 = down). Mirrors [`App::move_selection`] so the same keys
    /// (Shift+J/K) reorder whichever pane has focus.
    pub fn reorder_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Lists => self.reorder_list(delta),
            Focus::Tasks => self.reorder_task(delta),
        }
    }

    /// Swap the selected list with its neighbour `delta` slots away (-1 = up,
    /// +1 = down) and persist the new order. A no-op off the lists pane, in a
    /// task's detail view, at the ends, or involving the pinned Archived list
    /// (which stays at the bottom and can't be moved past).
    pub fn reorder_list(&mut self, delta: isize) {
        if self.focus != Focus::Lists || self.in_detail() {
            return;
        }
        // Only real list rows reorder; a tag row (or nothing) is a no-op. Since
        // list rows occupy indices 0..lists.len(), the target can't land on a
        // tag or header row.
        let Some(cur) = self.selected_list_index() else {
            return;
        };
        let target = cur as isize + delta;
        if target < 0 || target >= self.lists.len() as isize {
            return;
        }
        let target = target as usize;
        if self.lists.get(cur).is_some_and(|l| l.is_archive())
            || self.lists.get(target).is_some_and(|l| l.is_archive())
        {
            return;
        }
        self.lists.swap(cur, target);
        self.renumber_lists();
        self.list_state.select(Some(target));
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
        // Reordering only makes sense inside a single list; the cross-list tag
        // view has no stored order to nudge.
        let Some(li) = self.selected_list_index() else {
            self.set_status("reordering works inside a single list".to_string());
            return;
        };
        if self.reorder_blocked_by_sort() {
            return;
        }
        let vis = self.visible_task_refs();
        let v = self.selected_visible_task();
        let target = v as isize + delta;
        if target < 0 || target >= vis.len() as isize {
            return;
        }
        let target = target as usize;
        let Some(list) = self.lists.get_mut(li) else {
            return;
        };
        list.tasks.swap(vis[v].task, vis[target].task);
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
        let Some(li) = self.selected_list_index() else {
            self.set_status("reordering works inside a single list".to_string());
            return;
        };
        if self.reorder_blocked_by_sort() {
            return;
        }
        let vis = self.visible_task_refs();
        if vis.len() < 2 {
            return;
        }
        let from = vis[self.selected_visible_task()].task;
        // Insertion point (raw index) once `from` is spliced out: the current
        // first visible slot for a hoist, the current last for a sink. Both stay
        // valid after the removal because `from` lies within [first, last].
        let dest = if to_top {
            vis[0].task
        } else {
            vis.last().unwrap().task
        };
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
        let Some(r) = self.current_task_ref() else {
            return;
        };
        let src = r.list;
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
            filter: String::new(),
            selected: 0,
            scroll: 0,
            return_detail: self.in_detail(),
        });
    }

    /// Target list indices matching the current filter, in target order. Returns
    /// all targets when the filter is empty; empty vec when not in the picker.
    pub fn move_picker_filtered(&self) -> Vec<usize> {
        let Mode::MovePicker(state) = &self.mode else {
            return Vec::new();
        };
        let q = state.filter.trim().to_lowercase();
        if q.is_empty() {
            return state.targets.clone();
        }
        state
            .targets
            .iter()
            .copied()
            .filter(|&i| self.lists[i].name.to_lowercase().contains(&q))
            .collect()
    }

    /// Move the picker highlight over the filtered set (wraps).
    pub fn move_picker_move(&mut self, delta: isize) {
        let n = self.move_picker_filtered().len() as isize;
        if let Mode::MovePicker(state) = &mut self.mode {
            if n == 0 {
                state.selected = 0;
                return;
            }
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// Append a character to the filter, resetting the cursor to the top match.
    pub fn move_picker_type(&mut self, c: char) {
        if let Mode::MovePicker(state) = &mut self.mode {
            state.filter.push(c);
            state.selected = 0;
            state.scroll = 0;
        }
    }

    /// Delete the last filter character, resetting the cursor to the top match.
    pub fn move_picker_backspace(&mut self) {
        if let Mode::MovePicker(state) = &mut self.mode {
            state.filter.pop();
            state.selected = 0;
            state.scroll = 0;
        }
    }

    /// Splice the selected task out of its list and onto the end of the
    /// highlighted one, persisting both. The task leaves this list, so we always
    /// return to Normal (its detail view no longer applies here).
    pub fn move_picker_confirm(&mut self) {
        let filtered = self.move_picker_filtered();
        let target = match &self.mode {
            Mode::MovePicker(state) => filtered.get(state.selected).copied(),
            _ => return,
        };
        let key = self.selected_entry_key();
        let (Some(target), Some(r)) = (target, self.current_task_ref()) else {
            self.mode = Mode::Normal;
            return;
        };
        let task = self.lists[r.list].tasks.remove(r.task);
        let title = task.title.clone();
        let dest = self.lists[target].name.clone();
        self.lists[target].tasks.push(task);
        self.save_list_at(r.list);
        self.save_list_at(target);
        // The tag set is unchanged (the task keeps its tags), but re-anchor the
        // sidebar for safety, then clamp the task cursor.
        self.reselect_entry(key);
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

    // --- sort ----------------------------------------------------------------

    /// Open the sort picker, highlighting the sort currently in effect.
    pub fn open_sort_picker(&mut self) {
        let selected = Sort::all()
            .iter()
            .position(|&s| s == self.sort)
            .unwrap_or(0);
        self.mode = Mode::SortPicker(SortPickerState {
            selected,
            return_detail: self.in_detail(),
        });
    }

    /// Move the picker highlight (wraps).
    pub fn sort_picker_move(&mut self, delta: isize) {
        if let Mode::SortPicker(state) = &mut self.mode {
            let n = Sort::all().len() as isize;
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// Apply the highlighted sort and close the picker.
    pub fn sort_picker_confirm(&mut self) {
        if let Mode::SortPicker(state) = &self.mode {
            let sort = Sort::all()[state.selected];
            self.finish_sort_picker();
            self.set_sort(sort);
        }
    }

    /// Apply the sort at `idx` (the 1-6 quick keys) and close the picker.
    pub fn sort_picker_select(&mut self, idx: usize) {
        let all = Sort::all();
        if idx < all.len() && matches!(self.mode, Mode::SortPicker(_)) {
            self.finish_sort_picker();
            self.set_sort(all[idx]);
        }
    }

    /// Flip the sort direction. Works from inside the picker (live preview
    /// behind the overlay) or standalone; the selected task stays put.
    pub fn toggle_sort_dir(&mut self) {
        let current = self.current_task_ref();
        self.sort_dir = self.sort_dir.toggle();
        self.keep_task_selected(current);
        if self.sort == Sort::Manual {
            self.set_status(format!(
                "sort direction: {} (pick a sort to apply)",
                self.sort_dir.label()
            ));
        } else {
            self.announce_sort();
        }
    }

    /// Close the sort picker without changing the sort.
    pub fn sort_picker_cancel(&mut self) {
        if matches!(self.mode, Mode::SortPicker(_)) {
            self.finish_sort_picker();
        }
    }

    /// Return from the sort picker to whichever mode opened it.
    fn finish_sort_picker(&mut self) {
        if let Mode::SortPicker(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    /// Set the active sort, keeping the same task selected across the re-order
    /// so the cursor doesn't jump to whatever happens to land in its old slot.
    pub fn set_sort(&mut self, sort: Sort) {
        let current = self.current_task_ref();
        self.sort = sort;
        self.keep_task_selected(current);
        self.announce_sort();
    }

    /// Re-select task `current` after a re-order, falling back to a clamped
    /// selection if it is no longer visible. Matching on the full `(list, task)`
    /// reference keeps this correct in the tag view, where a bare task index
    /// could collide across lists.
    fn keep_task_selected(&mut self, current: Option<TaskRef>) {
        if let Some(r) = current
            && let Some(pos) = self.visible_task_refs().iter().position(|&x| x == r)
        {
            self.task_state.select(Some(pos));
        } else {
            self.clamp_task_selection();
        }
    }

    /// Status line for the active sort. `Manual` ignores direction (every pair
    /// ties, so the arrow would be meaningless), so it reports just the order.
    fn announce_sort(&mut self) {
        if self.sort == Sort::Manual {
            self.set_status(format!("sorted by {}", self.sort.label()));
        } else {
            self.set_status(format!(
                "sorted by {} {}",
                self.sort.label(),
                self.sort_dir.arrow()
            ));
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
        // A tag row (or focus without a list) has no single list to add to.
        if self.selected_list_index().is_none() {
            self.set_status("select a list to add a task".to_string());
            return;
        }
        self.mode = Mode::Input(InputState::new(
            InputField::NewTask,
            "New task  ·  !! #tag @today  n:note  s:sub, sub",
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
        // renames the list rather than editing a task title. A tag row has no
        // name of its own to rename.
        if !self.in_detail() && self.focus == Focus::Lists {
            if self.selected_list_index().is_none() {
                self.set_status("tags follow your tasks — nothing to rename here".to_string());
                return;
            }
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

    pub fn start_set_points(&mut self) {
        let Some(task) = self.current_task() else {
            return;
        };
        let buffer = task.points.map(|p| p.to_string()).unwrap_or_default();
        self.mode = Mode::Input(InputState::new(
            InputField::Points,
            "Story points (whole number; empty clears)",
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
                    // A tag row is selected (tags aren't a deletable entity).
                    self.set_status(
                        "tags can't be deleted — clear the tag from its tasks".to_string(),
                    );
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

    // =========================================================================
    // Notes tab
    // =========================================================================

    /// Switch the active top-level tab, leaving any transient mode. Ensures the
    /// destination tab has a sensible selection.
    pub fn switch_tab(&mut self, tab: Tab) {
        if self.active_tab == tab {
            return;
        }
        self.active_tab = tab;
        self.mode = Mode::Normal;
        match tab {
            Tab::Notes => {
                if self.notebook_state.selected().is_none() && !self.notebooks.is_empty() {
                    self.notebook_state.select(Some(0));
                    self.note_state.select(Some(0));
                }
            }
            Tab::Tasks => {
                if self.list_state.selected().is_none() && !self.lists.is_empty() {
                    self.list_state.select(Some(0));
                    self.task_state.select(Some(0));
                }
            }
        }
    }

    /// Total notes across every notebook, for the header summary.
    pub fn total_note_count(&self) -> usize {
        self.notebooks.iter().map(|nb| nb.notes.len()).sum()
    }

    // --- sidebar rows / tags -------------------------------------------------

    /// The Notes sidebar's flat selection order: every notebook first (so
    /// `Notebook(i)` sits at index `i`), then one row per distinct note tag.
    pub fn note_sidebar_rows(&self) -> Vec<NoteSidebarRow> {
        let mut rows: Vec<NoteSidebarRow> = (0..self.notebooks.len())
            .map(NoteSidebarRow::Notebook)
            .collect();
        rows.extend(self.all_note_tags().into_iter().map(NoteSidebarRow::Tag));
        rows
    }

    /// Every distinct tag across all notes, sorted alphabetically. Archived notes
    /// are excluded (matching the cross-notebook tag view).
    pub fn all_note_tags(&self) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for nb in &self.notebooks {
            if nb.is_archive() {
                continue;
            }
            for note in &nb.notes {
                for tag in &note.tags {
                    set.insert(tag.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// Count of notes carrying `name` across every real (non-archived) notebook.
    pub fn note_tag_count(&self, name: &str) -> usize {
        self.notebooks
            .iter()
            .filter(|nb| !nb.is_archive())
            .flat_map(|nb| nb.notes.iter())
            .filter(|n| n.tags.iter().any(|t| t == name))
            .count()
    }

    pub fn selected_note_row(&self) -> Option<NoteSidebarRow> {
        self.note_sidebar_rows()
            .into_iter()
            .nth(self.notebook_state.selected()?)
    }

    pub fn current_note_view(&self) -> Option<NoteView> {
        match self.selected_note_row()? {
            NoteSidebarRow::Notebook(i) => Some(NoteView::Notebook(i)),
            NoteSidebarRow::Tag(name) => Some(NoteView::Tag(name)),
        }
    }

    /// The selected notebook's index, or `None` when a tag row is selected.
    pub fn selected_notebook_index(&self) -> Option<usize> {
        match self.current_note_view()? {
            NoteView::Notebook(i) => Some(i),
            NoteView::Tag(_) => None,
        }
    }

    pub fn current_notebook(&self) -> Option<&Notebook> {
        self.notebooks.get(self.selected_notebook_index()?)
    }

    // --- visible notes -------------------------------------------------------

    /// The notes shown in the note list, as `(notebook, note)` references in
    /// display order: filtered by the search query, ordered by the active
    /// [`NoteSort`], with pinned notes floated to the top.
    pub fn visible_note_refs(&self) -> Vec<NoteRef> {
        let mut refs: Vec<NoteRef> = match self.current_note_view() {
            Some(NoteView::Notebook(ni)) => {
                let Some(nb) = self.notebooks.get(ni) else {
                    return Vec::new();
                };
                nb.notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.matches_query(&self.note_query))
                    .map(|(note, _)| NoteRef { notebook: ni, note })
                    .collect()
            }
            Some(NoteView::Tag(tag)) => {
                let mut out = Vec::new();
                for (ni, nb) in self.notebooks.iter().enumerate() {
                    if nb.is_archive() {
                        continue;
                    }
                    for (ti, note) in nb.notes.iter().enumerate() {
                        if note.tags.contains(&tag) && note.matches_query(&self.note_query) {
                            out.push(NoteRef {
                                notebook: ni,
                                note: ti,
                            });
                        }
                    }
                }
                out
            }
            None => return Vec::new(),
        };
        self.sort_note_refs(&mut refs);
        refs
    }

    /// Order `refs` by the active sort/direction, then float pinned notes to the
    /// top. Both passes are stable, so `NoteSort::Manual` preserves stored order.
    fn sort_note_refs(&self, refs: &mut [NoteRef]) {
        let note = |r: &NoteRef| &self.notebooks[r.notebook].notes[r.note];
        if self.note_sort != NoteSort::Manual {
            refs.sort_by(|a, b| {
                let ord = self.note_sort.compare(note(a), note(b));
                match self.note_sort_dir {
                    SortDir::Ascending => ord,
                    SortDir::Descending => ord.reverse(),
                }
            });
        }
        refs.sort_by_key(|r| !note(r).pinned);
    }

    pub fn selected_visible_note(&self) -> usize {
        self.note_state.selected().unwrap_or(0)
    }

    pub fn current_note_ref(&self) -> Option<NoteRef> {
        self.visible_note_refs()
            .get(self.selected_visible_note())
            .copied()
    }

    pub fn current_note(&self) -> Option<&Note> {
        let r = self.current_note_ref()?;
        self.notebooks.get(r.notebook)?.notes.get(r.note)
    }

    // --- selection bookkeeping ----------------------------------------------

    fn selected_note_entry_key(&self) -> Option<NoteEntryKey> {
        match self.selected_note_row()? {
            NoteSidebarRow::Notebook(i) => {
                Some(NoteEntryKey::Notebook(self.notebooks[i].slug.clone()))
            }
            NoteSidebarRow::Tag(name) => Some(NoteEntryKey::Tag(name)),
        }
    }

    /// Re-select the row identified by `key` after a mutation that may have added
    /// or removed rows (a tag appearing/disappearing), clamping when it's gone.
    fn reselect_notebook(&mut self, key: Option<NoteEntryKey>) {
        let rows = self.note_sidebar_rows();
        if rows.is_empty() {
            self.notebook_state.select(None);
            return;
        }
        let found = key.and_then(|k| {
            rows.iter().position(|row| match (&k, row) {
                (NoteEntryKey::Notebook(slug), NoteSidebarRow::Notebook(i)) => {
                    &self.notebooks[*i].slug == slug
                }
                (NoteEntryKey::Tag(a), NoteSidebarRow::Tag(b)) => a == b,
                _ => false,
            })
        });
        let idx = found.unwrap_or_else(|| {
            self.notebook_state
                .selected()
                .unwrap_or(0)
                .min(rows.len() - 1)
        });
        self.notebook_state.select(Some(idx));
    }

    fn clamp_note_selection(&mut self) {
        let len = self.visible_note_refs().len();
        if len == 0 {
            self.note_state.select(None);
        } else {
            let cur = self.selected_visible_note().min(len - 1);
            self.note_state.select(Some(cur));
        }
    }

    /// Re-select note `current` after a re-order, else clamp.
    fn keep_note_selected(&mut self, current: Option<NoteRef>) {
        if let Some(r) = current
            && let Some(pos) = self.visible_note_refs().iter().position(|&x| x == r)
        {
            self.note_state.select(Some(pos));
        } else {
            self.clamp_note_selection();
        }
    }

    // --- navigation ----------------------------------------------------------

    pub fn note_move_selection(&mut self, delta: isize) {
        if matches!(self.mode, Mode::Detail) {
            return;
        }
        match self.notes_focus {
            NotesFocus::Notebooks => {
                let len = self.note_sidebar_rows().len();
                if len == 0 {
                    return;
                }
                let cur = self.notebook_state.selected().unwrap_or(0) as isize;
                let next = (cur + delta).clamp(0, len as isize - 1) as usize;
                self.notebook_state.select(Some(next));
                self.note_state.select(Some(0));
            }
            NotesFocus::Notes => {
                let len = self.visible_note_refs().len();
                if len == 0 {
                    self.note_state.select(None);
                    return;
                }
                let cur = self.selected_visible_note() as isize;
                let next = (cur + delta).clamp(0, len as isize - 1) as usize;
                self.note_state.select(Some(next));
            }
        }
    }

    pub fn note_toggle_focus(&mut self) {
        match self.notes_focus {
            NotesFocus::Notebooks => self.note_focus_notes(),
            NotesFocus::Notes => self.notes_focus = NotesFocus::Notebooks,
        }
    }

    fn remember_note_row(&mut self) {
        match self.selected_note_row() {
            Some(NoteSidebarRow::Notebook(i)) => self.last_notebook_row = i,
            Some(NoteSidebarRow::Tag(_)) => {
                if let Some(sel) = self.notebook_state.selected() {
                    self.last_note_tag_row = sel.saturating_sub(self.notebooks.len());
                }
            }
            None => {}
        }
    }

    fn set_note_row_reset(&mut self, idx: usize) {
        if self.notebook_state.selected() != Some(idx) {
            self.notebook_state.select(Some(idx));
            self.note_state.select(Some(0));
        }
    }

    pub fn note_focus_notebooks(&mut self) {
        self.remember_note_row();
        self.notes_focus = NotesFocus::Notebooks;
        if self.notebooks.is_empty() {
            self.notebook_state.select(None);
        } else {
            let idx = self.last_notebook_row.min(self.notebooks.len() - 1);
            self.set_note_row_reset(idx);
        }
    }

    pub fn note_focus_notes(&mut self) {
        self.notes_focus = NotesFocus::Notes;
        if self.note_state.selected().is_none() && !self.visible_note_refs().is_empty() {
            self.note_state.select(Some(0));
        }
    }

    pub fn note_focus_tags(&mut self) {
        let n = self.all_note_tags().len();
        if n == 0 {
            return;
        }
        self.remember_note_row();
        self.notes_focus = NotesFocus::Notebooks;
        let idx = self.notebooks.len() + self.last_note_tag_row.min(n - 1);
        self.set_note_row_reset(idx);
    }

    pub fn note_activate(&mut self) {
        match self.notes_focus {
            NotesFocus::Notebooks => {
                if !self.notebooks.is_empty() {
                    self.note_focus_notes();
                }
            }
            // Enter on a note drops straight into the full-screen body editor.
            NotesFocus::Notes => {
                if self.current_note().is_some() {
                    self.start_edit_note_body();
                }
            }
        }
    }

    pub fn select_note_sidebar_row(&mut self, idx: usize) {
        if idx < self.note_sidebar_rows().len() {
            self.notebook_state.select(Some(idx));
            self.note_state.select(Some(0));
            self.notes_focus = NotesFocus::Notebooks;
        }
    }

    pub fn select_note_visible(&mut self, vidx: usize) {
        if vidx < self.visible_note_refs().len() {
            self.note_state.select(Some(vidx));
            self.notes_focus = NotesFocus::Notes;
        }
    }

    // --- persistence helpers -------------------------------------------------

    fn save_note_at(&mut self, ni: usize, note_idx: usize) {
        let result = {
            let Some(nb) = self.notebooks.get(ni) else {
                return;
            };
            let Some(note) = nb.notes.get(note_idx) else {
                return;
            };
            storage::save_note(&self.data_dir, &nb.slug, note)
        };
        if let Err(e) = result {
            self.set_status(format!("save error: {e}"));
        }
    }

    /// Run `f` against the selected note, stamp its modified time, and persist.
    fn with_current_note<F: FnOnce(&mut Note)>(&mut self, f: F) -> bool {
        let Some(r) = self.current_note_ref() else {
            return false;
        };
        let ok = if let Some(note) = self
            .notebooks
            .get_mut(r.notebook)
            .and_then(|nb| nb.notes.get_mut(r.note))
        {
            f(note);
            note.modified = Utc::now();
            true
        } else {
            false
        };
        if ok {
            self.save_note_at(r.notebook, r.note);
        }
        ok
    }

    /// Sort notebooks by name, with the Archived notebook always pinned to the
    /// bottom (mirrors how the Archived list sits at the foot of the sidebar).
    fn sort_notebooks(&mut self) {
        self.notebooks
            .sort_by(|a, b| match (a.is_archive(), b.is_archive()) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => a
                    .order
                    .unwrap_or(u32::MAX)
                    .cmp(&b.order.unwrap_or(u32::MAX))
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            });
    }

    /// Give every non-archive notebook an explicit `order` matching its current
    /// position and persist the order file. Called after a reorder so it sticks.
    fn renumber_notebooks(&mut self) {
        let mut slugs = Vec::new();
        let mut next = 0u32;
        for nb in self.notebooks.iter_mut() {
            if nb.is_archive() {
                continue;
            }
            nb.order = Some(next);
            slugs.push(nb.slug.clone());
            next += 1;
        }
        if let Err(e) = storage::save_notebook_order(&self.data_dir, &slugs) {
            self.set_status(format!("save error: {e}"));
        }
    }

    /// Focus-aware reorder on the Notes tab: nudge the selected notebook (on the
    /// notebooks pane) or note (on the notes pane) by `delta`.
    pub fn note_reorder_selection(&mut self, delta: isize) {
        match self.notes_focus {
            NotesFocus::Notebooks => self.reorder_notebook(delta),
            NotesFocus::Notes => self.reorder_note(delta),
        }
    }

    /// Swap the selected notebook with its neighbour `delta` slots away and
    /// persist. No-op on a tag row, in the full-screen editor, at the ends, or
    /// involving the pinned Archived notebook.
    pub fn reorder_notebook(&mut self, delta: isize) {
        if self.notes_focus != NotesFocus::Notebooks || matches!(self.mode, Mode::Detail) {
            return;
        }
        let Some(cur) = self.selected_notebook_index() else {
            return;
        };
        let target = cur as isize + delta;
        if target < 0 || target >= self.notebooks.len() as isize {
            return;
        }
        let target = target as usize;
        if self.notebooks[cur].is_archive() || self.notebooks[target].is_archive() {
            return;
        }
        self.notebooks.swap(cur, target);
        self.renumber_notebooks();
        self.notebook_state.select(Some(target));
    }

    fn archive_notebook_index(&self) -> Option<usize> {
        self.notebooks.iter().position(|n| n.is_archive())
    }

    /// Create the Archived notebook (folder + in-memory) if it doesn't exist.
    /// Appends it so existing notebook indices stay valid.
    fn ensure_archive_notebook(&mut self) {
        if self.data_dir.as_os_str().is_empty() || self.archive_notebook_index().is_some() {
            return;
        }
        if let Err(e) = storage::create_notebook_dir(&self.data_dir, model::ARCHIVE_NOTEBOOK_SLUG) {
            self.set_status(format!("save error: {e}"));
            return;
        }
        let mut nb = Notebook::new(model::ARCHIVE_NOTEBOOK_NAME);
        nb.slug = model::ARCHIVE_NOTEBOOK_SLUG.to_string();
        self.notebooks.push(nb);
    }

    /// Bake current vec positions into explicit `order` values and persist those
    /// that changed — called after a note reorder so it survives a reload.
    fn renumber_notes(&mut self, ni: usize) {
        let mut changed = Vec::new();
        if let Some(nb) = self.notebooks.get_mut(ni) {
            for (i, note) in nb.notes.iter_mut().enumerate() {
                let want = i as u32;
                if note.order != Some(want) {
                    note.order = Some(want);
                    changed.push(i);
                }
            }
        }
        for i in changed {
            self.save_note_at(ni, i);
        }
    }

    // --- notebook CRUD -------------------------------------------------------

    pub fn add_notebook(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let slug = storage::unique_notebook_slug(&self.data_dir, &name);
        if let Err(e) = storage::create_notebook_dir(&self.data_dir, &slug) {
            self.set_status(format!("save error: {e}"));
            return;
        }
        let mut nb = Notebook::new(slug.clone());
        nb.slug = slug.clone();
        self.notebooks.push(nb);
        self.sort_notebooks();
        let pos = self
            .notebooks
            .iter()
            .position(|n| n.slug == slug)
            .unwrap_or(0);
        self.notebook_state.select(Some(pos));
        self.note_state.select(Some(0));
        self.notes_focus = NotesFocus::Notebooks;
    }

    pub fn rename_current_notebook(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(ni) = self.selected_notebook_index() else {
            return;
        };
        if self.notebooks[ni].is_archive() {
            return;
        }
        let old_slug = self.notebooks[ni].slug.clone();
        // Same folder after sanitizing means nothing to do.
        if storage::sanitize_notebook_name(&name) == old_slug {
            return;
        }
        let new_slug = storage::unique_notebook_slug(&self.data_dir, &name);
        if let Err(e) = storage::rename_notebook_dir(&self.data_dir, &old_slug, &new_slug) {
            self.set_status(format!("rename error: {e}"));
            return;
        }
        self.notebooks[ni].slug = new_slug.clone();
        self.notebooks[ni].name = new_slug.clone();
        self.sort_notebooks();
        let pos = self
            .notebooks
            .iter()
            .position(|n| n.slug == new_slug)
            .unwrap_or(ni);
        self.notebook_state.select(Some(pos));
        self.note_state.select(Some(0));
        self.notes_focus = NotesFocus::Notebooks;
    }

    pub fn delete_current_notebook(&mut self) {
        let Some(ni) = self.selected_notebook_index() else {
            return;
        };
        let key = self.selected_note_entry_key();
        let nb = self.notebooks.remove(ni);
        if let Err(e) = storage::delete_notebook_dir(&self.data_dir, &nb.slug) {
            self.set_status(format!("delete error: {e}"));
        }
        self.reselect_notebook(key);
        self.clamp_note_selection();
        self.notes_focus = NotesFocus::Notebooks;
    }

    // --- note CRUD -----------------------------------------------------------

    pub fn add_note(&mut self, input: String) {
        // Pull `#tag` tokens out of the title, like the task quick-add parser.
        let (title, tags) = model::parse_note_title(&input);
        if title.is_empty() {
            return;
        }
        let Some(ni) = self.selected_notebook_index() else {
            self.set_status("select a notebook to add a note".to_string());
            return;
        };
        let nb_slug = self.notebooks[ni].slug.clone();
        let slug = storage::unique_note_slug(&self.data_dir, &nb_slug, &title);
        let mut note = Note::new(title);
        note.slug = slug;
        note.tags = tags;
        self.notebooks[ni].notes.push(note);
        let new_idx = self.notebooks[ni].notes.len() - 1;
        self.save_note_at(ni, new_idx);
        let refs = self.visible_note_refs();
        if let Some(pos) = refs
            .iter()
            .position(|r| r.notebook == ni && r.note == new_idx)
        {
            self.note_state.select(Some(pos));
        }
        self.notes_focus = NotesFocus::Notes;
        self.set_status("note added — n to write, E for $EDITOR".to_string());
    }

    pub fn edit_current_note_title(&mut self, title: String) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        self.with_current_note(|n| n.title = title);
    }

    pub fn set_current_note_body(&mut self, body: String) {
        self.with_current_note(|n| n.body = body);
    }

    pub fn set_current_note_tags(&mut self, input: &str) {
        let tags = model::parse_tags(input);
        let key = self.selected_note_entry_key();
        self.with_current_note(|n| n.tags = tags);
        self.reselect_notebook(key);
        self.clamp_note_selection();
    }

    pub fn toggle_current_note_pin(&mut self) {
        let current = self.current_note_ref();
        let pinned = self.current_note().map(|n| !n.pinned);
        self.with_current_note(|n| n.pinned = !n.pinned);
        self.keep_note_selected(current);
        if let Some(p) = pinned {
            self.set_status(if p { "pinned" } else { "unpinned" }.to_string());
        }
    }

    pub fn delete_current_note(&mut self) {
        let Some(r) = self.current_note_ref() else {
            return;
        };
        let key = self.selected_note_entry_key();
        let nb_slug = self.notebooks[r.notebook].slug.clone();
        let note_slug = self.notebooks[r.notebook].notes[r.note].slug.clone();
        self.notebooks[r.notebook].notes.remove(r.note);
        if let Err(e) = storage::delete_note_file(&self.data_dir, &nb_slug, &note_slug) {
            self.set_status(format!("delete error: {e}"));
        }
        self.reselect_notebook(key);
        self.clamp_note_selection();
        if matches!(self.mode, Mode::Detail) {
            self.mode = Mode::Normal;
        }
    }

    // --- archive / move ------------------------------------------------------

    /// `d` on the Notes tab: archive the selected note. On the notebooks pane
    /// there's nothing to archive, so it just points at `X` (like `d` on a list).
    pub fn note_archive_selection(&mut self) {
        if self.notes_focus == NotesFocus::Notebooks {
            self.set_status("notebooks can't be archived — X deletes a notebook".to_string());
            return;
        }
        self.archive_current_note();
    }

    /// Move the selected note into the Archived notebook (creating it on first
    /// use). Reversible via `m`, so no confirm. No-op when the note already lives
    /// in the archive.
    pub fn archive_current_note(&mut self) {
        let Some(r) = self.current_note_ref() else {
            return;
        };
        if self.notebooks[r.notebook].is_archive() {
            self.set_status("already archived — X deletes, m moves it out".to_string());
            return;
        }
        let key = self.selected_note_entry_key();
        let src_slug = self.notebooks[r.notebook].slug.clone();
        self.ensure_archive_notebook();
        let Some(dest) = self.archive_notebook_index() else {
            return;
        };
        let mut note = self.notebooks[r.notebook].notes.remove(r.note);
        let old_slug = note.slug.clone();
        let title = note.title.clone();
        let dest_slug = self.notebooks[dest].slug.clone();
        note.slug = storage::unique_note_slug(&self.data_dir, &dest_slug, &title);
        self.notebooks[dest].notes.push(note);
        let dest_idx = self.notebooks[dest].notes.len() - 1;
        self.save_note_at(dest, dest_idx);
        let _ = storage::delete_note_file(&self.data_dir, &src_slug, &old_slug);
        self.reselect_notebook(key);
        self.clamp_note_selection();
        if matches!(self.mode, Mode::Detail) {
            self.mode = Mode::Normal;
        }
        self.set_status(format!("archived \"{title}\""));
    }

    /// Open the picker to move the selected note to another notebook. Excludes
    /// the archive as a target (archiving uses `d`); moving *out* of the archive
    /// to a real notebook is how you unarchive.
    pub fn start_move_note(&mut self) {
        let Some(r) = self.current_note_ref() else {
            return;
        };
        let src = r.notebook;
        let targets: Vec<usize> = (0..self.notebooks.len())
            .filter(|&i| i != src && !self.notebooks[i].is_archive())
            .collect();
        if targets.is_empty() {
            self.set_status("no other notebook to move to".to_string());
            return;
        }
        self.mode = Mode::NoteMovePicker(MovePickerState {
            targets,
            filter: String::new(),
            selected: 0,
            scroll: 0,
            return_detail: matches!(self.mode, Mode::Detail),
        });
    }

    pub fn note_move_picker_move(&mut self, delta: isize) {
        if let Mode::NoteMovePicker(state) = &mut self.mode {
            let n = state.targets.len() as isize;
            if n == 0 {
                return;
            }
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// Move the selected note into the highlighted notebook and persist.
    pub fn note_move_picker_confirm(&mut self) {
        let target = match &self.mode {
            Mode::NoteMovePicker(state) => state.targets.get(state.selected).copied(),
            _ => return,
        };
        let key = self.selected_note_entry_key();
        let (Some(target), Some(r)) = (target, self.current_note_ref()) else {
            self.mode = Mode::Normal;
            return;
        };
        let src_slug = self.notebooks[r.notebook].slug.clone();
        let mut note = self.notebooks[r.notebook].notes.remove(r.note);
        let old_slug = note.slug.clone();
        let title = note.title.clone();
        let dest_slug = self.notebooks[target].slug.clone();
        let dest_name = self.notebooks[target].name.clone();
        note.slug = storage::unique_note_slug(&self.data_dir, &dest_slug, &title);
        self.notebooks[target].notes.push(note);
        let dest_idx = self.notebooks[target].notes.len() - 1;
        self.save_note_at(target, dest_idx);
        let _ = storage::delete_note_file(&self.data_dir, &src_slug, &old_slug);
        self.reselect_notebook(key);
        self.clamp_note_selection();
        self.set_status(format!("moved \"{title}\" to {dest_name}"));
        self.mode = Mode::Normal;
    }

    pub fn note_move_picker_cancel(&mut self) {
        if let Mode::NoteMovePicker(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    // --- copy a note ---------------------------------------------------------

    /// Open the copy menu for the selected note. No-op when there's no note.
    pub fn start_note_copy(&mut self) {
        if self.current_note().is_none() {
            return;
        }
        self.mode = Mode::NoteCopyMenu(CopyMenuState {
            selected: 0,
            return_detail: matches!(self.mode, Mode::Detail),
        });
    }

    pub fn note_copy_menu_move(&mut self, delta: isize) {
        if let Mode::NoteCopyMenu(state) = &mut self.mode {
            let n = NoteCopyWhat::all().len() as isize;
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    /// The text a copy of `what` would place on the clipboard. Pure.
    pub fn note_copy_payload(&self, what: NoteCopyWhat) -> Option<String> {
        let note = self.current_note()?;
        Some(match what {
            NoteCopyWhat::Body => note.body.clone(),
            NoteCopyWhat::Title => note.title.clone(),
            NoteCopyWhat::Markdown => note.to_file_string(),
        })
    }

    pub fn note_copy_menu_confirm(&mut self) {
        if let Mode::NoteCopyMenu(state) = &self.mode {
            self.note_copy_selected(NoteCopyWhat::all()[state.selected]);
        }
    }

    /// Copy `what` from the selected note to the clipboard and close the menu.
    pub fn note_copy_selected(&mut self, what: NoteCopyWhat) {
        let return_detail = match &self.mode {
            Mode::NoteCopyMenu(state) => state.return_detail,
            _ => return,
        };
        self.mode = if return_detail {
            Mode::Detail
        } else {
            Mode::Normal
        };
        let Some(text) = self.note_copy_payload(what) else {
            self.set_status("nothing to copy".to_string());
            return;
        };
        if what == NoteCopyWhat::Body && text.trim().is_empty() {
            self.set_status("note has no body to copy".to_string());
            return;
        }
        match crate::clipboard::copy(&text) {
            Ok(()) => self.set_status(format!("copied {} to clipboard", what.noun())),
            Err(e) => self.set_status(format!("copy failed: {e}")),
        }
    }

    pub fn note_copy_menu_cancel(&mut self) {
        if let Mode::NoteCopyMenu(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    // --- rendered read view + links ------------------------------------------

    /// Open the full-screen rendered read view for the selected note.
    pub fn open_note_view(&mut self) {
        if self.current_note().is_some() {
            self.mode = Mode::NoteView(NoteViewState::default());
        }
    }

    /// Scroll the read view (clamped at the top; the bottom is clamped in the
    /// renderer, which knows the wrapped height).
    pub fn note_view_scroll(&mut self, delta: isize) {
        if let Mode::NoteView(state) = &mut self.mode {
            state.scroll = (state.scroll as isize + delta).max(0) as u16;
        }
    }

    /// From the read view, drop into the raw editor for the same note.
    pub fn note_view_edit(&mut self) {
        if matches!(self.mode, Mode::NoteView(_)) {
            self.start_edit_note_body();
        }
    }

    /// Find a note by title (case-insensitive) across every notebook.
    pub fn resolve_note_link(&self, title: &str) -> Option<NoteRef> {
        let want = title.trim().to_lowercase();
        for (ni, nb) in self.notebooks.iter().enumerate() {
            for (ti, note) in nb.notes.iter().enumerate() {
                if note.title.to_lowercase() == want {
                    return Some(NoteRef {
                        notebook: ni,
                        note: ti,
                    });
                }
            }
        }
        None
    }

    /// Notes that link to `title` via `[[title]]` (case-insensitive), excluding
    /// the note at `exclude`.
    pub fn note_backlinks(&self, title: &str, exclude: Option<NoteRef>) -> Vec<NoteRef> {
        let want = title.trim().to_lowercase();
        let mut out = Vec::new();
        for (ni, nb) in self.notebooks.iter().enumerate() {
            for (ti, note) in nb.notes.iter().enumerate() {
                let here = NoteRef {
                    notebook: ni,
                    note: ti,
                };
                if Some(here) == exclude {
                    continue;
                }
                if note.link_targets().iter().any(|t| t.to_lowercase() == want) {
                    out.push(here);
                }
            }
        }
        out
    }

    /// Select the notebook + note at `r` and open its read view. Clears any note
    /// search so the target is visible.
    pub fn jump_to_note(&mut self, r: NoteRef) {
        if r.notebook >= self.notebooks.len() || r.note >= self.notebooks[r.notebook].notes.len() {
            return;
        }
        self.note_query.clear();
        self.notes_focus = NotesFocus::Notes;
        self.notebook_state.select(Some(r.notebook));
        let pos = self
            .visible_note_refs()
            .iter()
            .position(|&x| x == r)
            .unwrap_or(0);
        self.note_state.select(Some(pos));
        self.open_note_view();
    }

    /// Open the link picker for the selected note: resolvable outgoing links
    /// first, then backlinks. `return_view` remembers whether to fall back to the
    /// read view on cancel.
    pub fn start_note_link_picker(&mut self, return_view: bool) {
        let Some(r) = self.current_note_ref() else {
            return;
        };
        let note = &self.notebooks[r.notebook].notes[r.note];
        let mut entries: Vec<NoteLinkEntry> = Vec::new();
        for target in note.link_targets() {
            if let Some(dest) = self.resolve_note_link(&target)
                && dest != r
            {
                entries.push(NoteLinkEntry {
                    label: format!(
                        "\u{2192} {}",
                        self.notebooks[dest.notebook].notes[dest.note].title
                    ),
                    target: dest,
                });
            }
        }
        let title = note.title.clone();
        for back in self.note_backlinks(&title, Some(r)) {
            entries.push(NoteLinkEntry {
                label: format!(
                    "\u{2190} {} (backlink)",
                    self.notebooks[back.notebook].notes[back.note].title
                ),
                target: back,
            });
        }
        if entries.is_empty() {
            self.set_status("no links in or to this note".to_string());
            return;
        }
        self.mode = Mode::NoteLinkPicker(NoteLinkPickerState {
            entries,
            selected: 0,
            return_view,
        });
    }

    pub fn note_link_picker_move(&mut self, delta: isize) {
        if let Mode::NoteLinkPicker(state) = &mut self.mode {
            let n = state.entries.len() as isize;
            if n == 0 {
                return;
            }
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    pub fn note_link_picker_confirm(&mut self) {
        let target = match &self.mode {
            Mode::NoteLinkPicker(state) => state.entries.get(state.selected).map(|e| e.target),
            _ => return,
        };
        if let Some(r) = target {
            self.jump_to_note(r);
        }
    }

    pub fn note_link_picker_cancel(&mut self) {
        if let Mode::NoteLinkPicker(state) = &self.mode {
            if state.return_view {
                self.open_note_view();
            } else {
                self.mode = Mode::Normal;
            }
        }
    }

    // --- reorder notes -------------------------------------------------------

    /// Reordering only makes sense under `NoteSort::Manual`; returns true (after
    /// a hint) when it should be refused.
    fn note_reorder_blocked(&mut self) -> bool {
        if self.note_sort != NoteSort::Manual {
            self.set_status("reordering needs manual sort (press o)".to_string());
            true
        } else {
            false
        }
    }

    pub fn reorder_note(&mut self, delta: isize) {
        if self.notes_focus != NotesFocus::Notes || matches!(self.mode, Mode::Detail) {
            return;
        }
        let Some(ni) = self.selected_notebook_index() else {
            self.set_status("reordering works inside a single notebook".to_string());
            return;
        };
        if self.note_reorder_blocked() {
            return;
        }
        let vis = self.visible_note_refs();
        let v = self.selected_visible_note();
        let target = v as isize + delta;
        if target < 0 || target >= vis.len() as isize {
            return;
        }
        let target = target as usize;
        self.notebooks[ni].notes.swap(vis[v].note, vis[target].note);
        self.renumber_notes(ni);
        self.note_state.select(Some(target));
    }

    pub fn send_note(&mut self, to_top: bool) {
        if self.notes_focus != NotesFocus::Notes || matches!(self.mode, Mode::Detail) {
            return;
        }
        let Some(ni) = self.selected_notebook_index() else {
            self.set_status("reordering works inside a single notebook".to_string());
            return;
        };
        if self.note_reorder_blocked() {
            return;
        }
        let vis = self.visible_note_refs();
        if vis.len() < 2 {
            return;
        }
        let from = vis[self.selected_visible_note()].note;
        let dest = if to_top {
            vis[0].note
        } else {
            vis.last().unwrap().note
        };
        let note = self.notebooks[ni].notes.remove(from);
        self.notebooks[ni].notes.insert(dest, note);
        self.renumber_notes(ni);
        self.note_state
            .select(Some(if to_top { 0 } else { vis.len() - 1 }));
    }

    // --- search --------------------------------------------------------------

    pub fn set_note_search(&mut self, query: String) {
        self.note_query = query;
        self.clamp_note_selection();
    }

    pub fn clear_note_filter(&mut self) {
        self.note_query.clear();
        self.clamp_note_selection();
    }

    pub fn note_filter_active(&self) -> bool {
        !self.note_query.is_empty()
    }

    // --- note sort picker ----------------------------------------------------

    pub fn open_note_sort_picker(&mut self) {
        let selected = NoteSort::all()
            .iter()
            .position(|&s| s == self.note_sort)
            .unwrap_or(0);
        self.mode = Mode::NoteSortPicker(SortPickerState {
            selected,
            return_detail: matches!(self.mode, Mode::Detail),
        });
    }

    pub fn note_sort_picker_move(&mut self, delta: isize) {
        if let Mode::NoteSortPicker(state) = &mut self.mode {
            let n = NoteSort::all().len() as isize;
            state.selected = ((((state.selected as isize + delta) % n) + n) % n) as usize;
        }
    }

    pub fn note_sort_picker_confirm(&mut self) {
        if let Mode::NoteSortPicker(state) = &self.mode {
            let sort = NoteSort::all()[state.selected];
            self.finish_note_sort_picker();
            self.set_note_sort(sort);
        }
    }

    pub fn note_sort_picker_select(&mut self, idx: usize) {
        let all = NoteSort::all();
        if idx < all.len() && matches!(self.mode, Mode::NoteSortPicker(_)) {
            self.finish_note_sort_picker();
            self.set_note_sort(all[idx]);
        }
    }

    pub fn toggle_note_sort_dir(&mut self) {
        let current = self.current_note_ref();
        self.note_sort_dir = self.note_sort_dir.toggle();
        self.keep_note_selected(current);
        if self.note_sort == NoteSort::Manual {
            self.set_status(format!(
                "sort direction: {} (pick a sort to apply)",
                self.note_sort_dir.label()
            ));
        } else {
            self.announce_note_sort();
        }
    }

    pub fn note_sort_picker_cancel(&mut self) {
        if matches!(self.mode, Mode::NoteSortPicker(_)) {
            self.finish_note_sort_picker();
        }
    }

    fn finish_note_sort_picker(&mut self) {
        if let Mode::NoteSortPicker(state) = &self.mode {
            self.mode = if state.return_detail {
                Mode::Detail
            } else {
                Mode::Normal
            };
        }
    }

    pub fn set_note_sort(&mut self, sort: NoteSort) {
        let current = self.current_note_ref();
        self.note_sort = sort;
        self.keep_note_selected(current);
        self.announce_note_sort();
    }

    fn announce_note_sort(&mut self) {
        if self.note_sort == NoteSort::Manual {
            self.set_status(format!("sorted by {}", self.note_sort.label()));
        } else {
            self.set_status(format!(
                "sorted by {} {}",
                self.note_sort.label(),
                self.note_sort_dir.arrow()
            ));
        }
    }

    // --- external editor -----------------------------------------------------

    /// Request that the run loop open the selected note in `$EDITOR`. The file is
    /// flushed to disk first so the editor sees the current content.
    pub fn open_note_in_editor(&mut self) {
        let Some(r) = self.current_note_ref() else {
            self.set_status("select a note to edit".to_string());
            return;
        };
        let nb_slug = self.notebooks[r.notebook].slug.clone();
        let note_slug = self.notebooks[r.notebook].notes[r.note].slug.clone();
        self.save_note_at(r.notebook, r.note);
        self.pending_editor = Some(storage::note_path(&self.data_dir, &nb_slug, &note_slug));
    }

    /// Take the pending external-edit request, if any (called by the run loop).
    pub fn take_pending_editor(&mut self) -> Option<PathBuf> {
        self.pending_editor.take()
    }

    /// Reload the selected note's content from disk after an external edit.
    pub fn reload_current_note_from_disk(&mut self) {
        let Some(r) = self.current_note_ref() else {
            return;
        };
        let nb_slug = self.notebooks[r.notebook].slug.clone();
        let note_slug = self.notebooks[r.notebook].notes[r.note].slug.clone();
        let path = storage::note_path(&self.data_dir, &nb_slug, &note_slug);
        match storage::load_note(&path) {
            Ok(mut note) => {
                note.slug = note_slug;
                if let Some(n) = self
                    .notebooks
                    .get_mut(r.notebook)
                    .and_then(|nb| nb.notes.get_mut(r.note))
                {
                    *n = note;
                }
            }
            Err(_) => { /* file may have been removed in the editor; leave as-is */ }
        }
    }

    // --- starting note input/confirm modes -----------------------------------

    pub fn start_add_notebook(&mut self) {
        self.mode = Mode::Input(InputState::new(
            InputField::NewNotebook,
            "New notebook name",
            String::new(),
            false,
            false,
        ));
    }

    pub fn start_rename_notebook(&mut self) {
        let Some(nb) = self.current_notebook() else {
            return;
        };
        if nb.is_archive() {
            self.set_status("the Archived notebook can't be renamed".to_string());
            return;
        }
        let name = nb.name.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::RenameNotebook,
            "Rename notebook",
            name,
            false,
            false,
        ));
    }

    pub fn start_add_note(&mut self) {
        if self.notebooks.is_empty() {
            self.set_status("create a notebook first (A)".to_string());
            return;
        }
        if self.selected_notebook_index().is_none() {
            self.set_status("select a notebook to add a note".to_string());
            return;
        }
        self.mode = Mode::Input(InputState::new(
            InputField::NewNote,
            "New note  ·  title #tag #tag",
            String::new(),
            false,
            false,
        ));
    }

    /// `e` on the Notes tab: rename the notebook (notebooks pane) or edit the
    /// note title (notes pane / full-screen view).
    pub fn start_edit_note(&mut self) {
        if self.notes_focus == NotesFocus::Notebooks && !matches!(self.mode, Mode::Detail) {
            if self.selected_notebook_index().is_none() {
                self.set_status("tags follow your notes — nothing to rename here".to_string());
                return;
            }
            self.start_rename_notebook();
            return;
        }
        let Some(note) = self.current_note() else {
            return;
        };
        let title = note.title.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::EditNote,
            "Edit note title",
            title,
            false,
            matches!(self.mode, Mode::Detail),
        ));
    }

    pub fn start_edit_note_body(&mut self) {
        let Some(note) = self.current_note() else {
            return;
        };
        let body = note.body.clone();
        self.mode = Mode::Input(InputState::new(
            InputField::NoteBody,
            "Note body (Enter = newline, Ctrl+S save, Esc cancel)",
            body,
            true,
            matches!(self.mode, Mode::Detail),
        ));
    }

    pub fn start_set_note_tags(&mut self) {
        let Some(note) = self.current_note() else {
            return;
        };
        let buffer = note.tags.join(", ");
        self.mode = Mode::Input(InputState::new(
            InputField::NoteTags,
            "Tags (comma or space separated; empty clears)",
            buffer,
            false,
            matches!(self.mode, Mode::Detail),
        ));
    }

    pub fn start_note_search(&mut self) {
        self.mode = Mode::Input(InputState::new(
            InputField::NoteSearch,
            "Search notes (title, body, tags)",
            self.note_query.clone(),
            false,
            matches!(self.mode, Mode::Detail),
        ));
    }

    pub fn start_delete_note(&mut self) {
        if self.notes_focus == NotesFocus::Notebooks && !matches!(self.mode, Mode::Detail) {
            let Some(nb) = self.current_notebook() else {
                self.set_status("tags can't be deleted — clear the tag from its notes".to_string());
                return;
            };
            if nb.is_archive() {
                self.set_status("the Archived notebook can't be deleted".to_string());
                return;
            }
            let name = nb.name.clone();
            let n = nb.notes.len();
            self.mode = Mode::Confirm(ConfirmState {
                prompt: format!("Delete notebook \"{name}\" and its {n} note(s)?"),
                action: ConfirmAction::DeleteNotebook,
                return_detail: false,
            });
            return;
        }
        let Some(note) = self.current_note() else {
            return;
        };
        let title = note.title.clone();
        self.mode = Mode::Confirm(ConfirmState {
            prompt: format!("Permanently delete \"{title}\"?"),
            action: ConfirmAction::DeleteNote,
            return_detail: matches!(self.mode, Mode::Detail),
        });
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
            keybindings: self.keymap.to_full_overrides(),
            completed_to_bottom: self.completed_to_bottom,
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
        self.notebooks = storage::load_notebooks(&self.data_dir).unwrap_or_default();
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
