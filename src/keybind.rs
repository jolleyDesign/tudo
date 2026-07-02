//! User-remappable keybindings for the main (Normal / Detail) navigation mode.
//!
//! Every key the two main modes react to maps to an [`Action`]. The built-in
//! defaults live in [`Action::default_chords`]; a user can override any of them
//! from the config file (see [`Keymap::from_overrides`]). Only the primary
//! navigation/action keys are remappable — text entry, confirm prompts, and the
//! pickers keep their fixed keys so those modes always behave predictably.
//!
//! Config shape (JSON), keyed by action name with a list of keys:
//! ```json
//! "keybindings": {
//!   "quit": ["q", "ctrl+q"],
//!   "move-down": ["j", "down"],
//!   "search": ["/"]
//! }
//! ```
//! An action listed here *replaces* its defaults; actions left out keep theirs.

use std::collections::{BTreeMap, HashMap};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The raw, user-facing keybinding overrides as stored in the config: an action
/// name mapped to the list of keys that should trigger it.
pub type Overrides = BTreeMap<String, Vec<String>>;

/// Every action the Normal / Detail modes can bind a key to.
///
/// The order of [`Action::ALL`] is the canonical order used for the config
/// docs and the help overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Navigate
    ToggleFocus,
    FocusLists,
    FocusTasks,
    MoveDown,
    MoveUp,
    Activate,
    Back,
    // Organize
    ToggleDone,
    ReorderDown,
    ReorderUp,
    SendTop,
    SendBottom,
    MoveTask,
    // Create & edit
    AddTask,
    AddList,
    Edit,
    AddSubtask,
    CyclePriority,
    SetDue,
    SetTags,
    SetNotes,
    // Remove
    Archive,
    Delete,
    // Find & app
    Search,
    CycleFilter,
    Copy,
    ThemePicker,
    Settings,
    Help,
    Quit,
}

impl Action {
    /// All actions, in canonical order.
    pub const ALL: [Action; 30] = [
        Action::ToggleFocus,
        Action::FocusLists,
        Action::FocusTasks,
        Action::MoveDown,
        Action::MoveUp,
        Action::Activate,
        Action::Back,
        Action::ToggleDone,
        Action::ReorderDown,
        Action::ReorderUp,
        Action::SendTop,
        Action::SendBottom,
        Action::MoveTask,
        Action::AddTask,
        Action::AddList,
        Action::Edit,
        Action::AddSubtask,
        Action::CyclePriority,
        Action::SetDue,
        Action::SetTags,
        Action::SetNotes,
        Action::Archive,
        Action::Delete,
        Action::Search,
        Action::CycleFilter,
        Action::Copy,
        Action::ThemePicker,
        Action::Settings,
        Action::Help,
        Action::Quit,
    ];

    /// The kebab-case name used as the config key for this action.
    pub fn config_name(self) -> &'static str {
        match self {
            Action::ToggleFocus => "toggle-focus",
            Action::FocusLists => "focus-lists",
            Action::FocusTasks => "focus-tasks",
            Action::MoveDown => "move-down",
            Action::MoveUp => "move-up",
            Action::Activate => "activate",
            Action::Back => "back",
            Action::ToggleDone => "toggle-done",
            Action::ReorderDown => "reorder-down",
            Action::ReorderUp => "reorder-up",
            Action::SendTop => "send-top",
            Action::SendBottom => "send-bottom",
            Action::MoveTask => "move-task",
            Action::AddTask => "add-task",
            Action::AddList => "add-list",
            Action::Edit => "edit",
            Action::AddSubtask => "add-subtask",
            Action::CyclePriority => "cycle-priority",
            Action::SetDue => "set-due",
            Action::SetTags => "set-tags",
            Action::SetNotes => "set-notes",
            Action::Archive => "archive",
            Action::Delete => "delete",
            Action::Search => "search",
            Action::CycleFilter => "cycle-filter",
            Action::Copy => "copy",
            Action::ThemePicker => "theme-picker",
            Action::Settings => "settings",
            Action::Help => "help",
            Action::Quit => "quit",
        }
    }

    /// Look up an action by its config name.
    pub fn from_config_name(s: &str) -> Option<Action> {
        let s = s.trim();
        Action::ALL.iter().copied().find(|a| a.config_name() == s)
    }

    /// The built-in default keys for this action.
    pub fn default_chords(self) -> Vec<KeyChord> {
        // Helper to build a plain (unmodified) chord.
        let c = KeyChord::plain;
        match self {
            Action::ToggleFocus => vec![c(KeyCode::Tab)],
            Action::FocusLists => vec![c(KeyCode::Char('h')), c(KeyCode::Left)],
            Action::FocusTasks => vec![c(KeyCode::Char('l')), c(KeyCode::Right)],
            Action::MoveDown => vec![c(KeyCode::Char('j')), c(KeyCode::Down)],
            Action::MoveUp => vec![c(KeyCode::Char('k')), c(KeyCode::Up)],
            Action::Activate => vec![c(KeyCode::Enter)],
            Action::Back => vec![c(KeyCode::Esc)],
            Action::ToggleDone => vec![c(KeyCode::Char(' '))],
            Action::ReorderDown => vec![
                c(KeyCode::Char('J')),
                KeyChord::new(KeyCode::Down, KeyModifiers::SHIFT),
            ],
            Action::ReorderUp => vec![
                c(KeyCode::Char('K')),
                KeyChord::new(KeyCode::Up, KeyModifiers::SHIFT),
            ],
            Action::SendTop => vec![c(KeyCode::Char('g'))],
            Action::SendBottom => vec![c(KeyCode::Char('G'))],
            Action::MoveTask => vec![c(KeyCode::Char('m'))],
            Action::AddTask => vec![c(KeyCode::Char('a'))],
            Action::AddList => vec![c(KeyCode::Char('A'))],
            Action::Edit => vec![c(KeyCode::Char('e'))],
            Action::AddSubtask => vec![c(KeyCode::Char('s'))],
            Action::CyclePriority => vec![c(KeyCode::Char('p'))],
            Action::SetDue => vec![c(KeyCode::Char('D'))],
            Action::SetTags => vec![c(KeyCode::Char('t'))],
            Action::SetNotes => vec![c(KeyCode::Char('n'))],
            Action::Archive => vec![c(KeyCode::Char('d'))],
            Action::Delete => vec![c(KeyCode::Char('X'))],
            Action::Search => vec![c(KeyCode::Char('/'))],
            Action::CycleFilter => vec![c(KeyCode::Char('f'))],
            Action::Copy => vec![c(KeyCode::Char('c'))],
            Action::ThemePicker => vec![c(KeyCode::Char('T'))],
            Action::Settings => vec![c(KeyCode::Char('S'))],
            Action::Help => vec![c(KeyCode::Char('?'))],
            Action::Quit => vec![c(KeyCode::Char('q'))],
        }
    }
}

/// A normalized key press: a [`KeyCode`] plus any Ctrl/Alt/Shift modifiers.
///
/// Normalization keeps only the Ctrl/Alt/Shift modifiers and, for character
/// keys, drops Shift (the character's own case already encodes it). This makes
/// `J` match whether or not the terminal reports Shift alongside it, and keeps
/// Shift meaningful for non-character keys like the arrows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    /// Build a normalized chord from a code and modifier set.
    pub fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        let mut mods = mods & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        if matches!(code, KeyCode::Char(_)) {
            mods.remove(KeyModifiers::SHIFT);
        }
        Self { code, mods }
    }

    /// A chord with no modifiers.
    pub fn plain(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }

    /// The chord for an incoming key event.
    pub fn from_event(key: KeyEvent) -> Self {
        Self::new(key.code, key.modifiers)
    }

    /// Parse a config string like `"j"`, `"J"`, `"ctrl+c"`, `"shift+down"`,
    /// `"space"`, `"tab"`, `"enter"`, `"/"`. Returns `None` if unrecognized.
    pub fn parse(s: &str) -> Option<KeyChord> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        // A lone single character is the key itself (covers "/", "+", "a", "?").
        if s.chars().count() == 1 {
            return Some(KeyChord::plain(KeyCode::Char(s.chars().next().unwrap())));
        }
        let mut mods = KeyModifiers::NONE;
        let parts: Vec<&str> = s.split('+').collect();
        let (mod_parts, key) = parts.split_at(parts.len() - 1);
        for m in mod_parts {
            match m.trim().to_lowercase().as_str() {
                "ctrl" | "control" | "c" => mods |= KeyModifiers::CONTROL,
                "alt" | "option" | "meta" | "a" => mods |= KeyModifiers::ALT,
                "shift" => mods |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        let code = parse_code(key[0].trim())?;
        Some(KeyChord::new(code, mods))
    }

    /// A human-friendly label for the help overlay (arrows as glyphs, etc.).
    pub fn display(self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str("Ctrl+");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str("Alt+");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            out.push('\u{21e7}'); // ⇧
        }
        out.push_str(&code_label(self.code));
        out
    }

    /// The canonical config token for this chord — the inverse of [`parse`], so
    /// it round-trips. Lowercase key names, character case preserved, and
    /// `ctrl+` / `alt+` / `shift+` prefixes.
    ///
    /// [`parse`]: KeyChord::parse
    pub fn config_token(self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str("ctrl+");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str("alt+");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            out.push_str("shift+");
        }
        out.push_str(&code_token(self.code));
        out
    }
}

/// Parse the key portion (after any modifiers) of a config string.
fn parse_code(s: &str) -> Option<KeyCode> {
    Some(match s.to_lowercase().as_str() {
        "space" | "spc" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        _ => {
            // A single character, kept case-sensitive ('J' != 'j').
            let mut chars = s.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // unrecognized multi-character key name
            }
            KeyCode::Char(c)
        }
    })
}

/// Human-friendly label for a key code (used by [`KeyChord::display`]).
fn code_label(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Up => "\u{2191}".to_string(),    // ↑
        KeyCode::Down => "\u{2193}".to_string(),  // ↓
        KeyCode::Left => "\u{2190}".to_string(),  // ←
        KeyCode::Right => "\u{2192}".to_string(), // →
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PgUp".to_string(),
        KeyCode::PageDown => "PgDn".to_string(),
        KeyCode::Backspace => "Bksp".to_string(),
        KeyCode::Delete => "Del".to_string(),
        KeyCode::Insert => "Ins".to_string(),
        other => format!("{other:?}"),
    }
}

/// Canonical config token for a key code (used by [`KeyChord::config_token`]);
/// the inverse of [`parse_code`], so it round-trips.
fn code_token(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Delete => "delete".to_string(),
        KeyCode::Insert => "insert".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// The resolved keymap: built-in defaults with the user's overrides applied.
#[derive(Debug, Clone)]
pub struct Keymap {
    /// Chord -> action, for runtime lookup.
    lookup: HashMap<KeyChord, Action>,
    /// Action -> its bound chords, for the help overlay.
    bound: HashMap<Action, Vec<KeyChord>>,
    /// The raw overrides exactly as loaded, preserved so the config round-trips
    /// (including entries this version doesn't recognize).
    overrides: Overrides,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::from_overrides(Overrides::new()).0
    }
}

impl Keymap {
    /// Build a keymap from raw config overrides, returning it alongside any
    /// warnings (unknown action names or unparseable keys). Unrecognized entries
    /// are skipped but preserved in [`Keymap::overrides`] for round-tripping.
    pub fn from_overrides(overrides: Overrides) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let mut bound: HashMap<Action, Vec<KeyChord>> = HashMap::new();

        // Defaults for every action the user didn't override.
        for action in Action::ALL {
            if !overrides.contains_key(action.config_name()) {
                bound.insert(action, action.default_chords());
            }
        }
        // Overrides replace an action's chords wholesale.
        for (name, keys) in &overrides {
            let Some(action) = Action::from_config_name(name) else {
                warnings.push(format!("unknown keybinding action \"{name}\""));
                continue;
            };
            let mut chords = Vec::new();
            for k in keys {
                match KeyChord::parse(k) {
                    Some(chord) => chords.push(chord),
                    None => warnings.push(format!("unrecognized key \"{k}\" for \"{name}\"")),
                }
            }
            bound.insert(action, chords);
        }

        // Build the chord -> action lookup so that overridden actions win over
        // defaults on any shared chord: insert defaults first, overrides second.
        let mut lookup: HashMap<KeyChord, Action> = HashMap::new();
        for (&action, chords) in &bound {
            if !overrides.contains_key(action.config_name()) {
                for &chord in chords {
                    lookup.insert(chord, action);
                }
            }
        }
        for name in overrides.keys() {
            if let Some(action) = Action::from_config_name(name)
                && let Some(chords) = bound.get(&action)
            {
                for &chord in chords {
                    lookup.insert(chord, action);
                }
            }
        }

        (
            Self {
                lookup,
                bound,
                overrides,
            },
            warnings,
        )
    }

    /// The action bound to an incoming key event, if any.
    pub fn action_for(&self, key: KeyEvent) -> Option<Action> {
        self.lookup.get(&KeyChord::from_event(key)).copied()
    }

    /// The chords currently bound to `action` (empty if it was unbound).
    pub fn chords_for(&self, action: Action) -> &[KeyChord] {
        self.bound.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The first chord bound to `action`, for compact hints (e.g. the footer).
    pub fn primary(&self, action: Action) -> Option<KeyChord> {
        self.chords_for(action).first().copied()
    }

    /// The raw overrides, for persisting the config unchanged.
    pub fn overrides(&self) -> &Overrides {
        &self.overrides
    }

    /// Whether the raw overrides already list every known action — i.e. the
    /// config on disk is complete and needs no filling in.
    pub fn covers_all_actions(&self) -> bool {
        Action::ALL
            .iter()
            .all(|a| self.overrides.contains_key(a.config_name()))
    }

    /// The complete binding set as config overrides: every action mapped to its
    /// current keys (defaults plus any overrides), with any unrecognized user
    /// entries kept as-is. This is what gets written to the config so the file
    /// lists every binding and is easy to edit.
    pub fn to_full_overrides(&self) -> Overrides {
        let mut out = self.overrides.clone();
        for action in Action::ALL {
            let tokens = self
                .chords_for(action)
                .iter()
                .map(|c| c.config_token())
                .collect();
            out.insert(action.config_name().to_string(), tokens);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_named_and_modified_keys() {
        assert_eq!(
            KeyChord::parse("j"),
            Some(KeyChord::plain(KeyCode::Char('j')))
        );
        // Case is preserved for letters.
        assert_eq!(
            KeyChord::parse("J"),
            Some(KeyChord::plain(KeyCode::Char('J')))
        );
        assert_eq!(
            KeyChord::parse("space"),
            Some(KeyChord::plain(KeyCode::Char(' ')))
        );
        assert_eq!(KeyChord::parse("tab"), Some(KeyChord::plain(KeyCode::Tab)));
        // A lone symbol is taken literally.
        assert_eq!(
            KeyChord::parse("/"),
            Some(KeyChord::plain(KeyCode::Char('/')))
        );
        assert_eq!(
            KeyChord::parse("ctrl+c"),
            Some(KeyChord::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            KeyChord::parse("shift+down"),
            Some(KeyChord::new(KeyCode::Down, KeyModifiers::SHIFT))
        );
        assert_eq!(KeyChord::parse("nope-key"), None);
        assert_eq!(KeyChord::parse("bad+j"), None);
    }

    #[test]
    fn char_chords_ignore_shift_but_arrows_keep_it() {
        // Shift on a character key folds away (the case carries it).
        let upper = KeyChord::new(KeyCode::Char('J'), KeyModifiers::SHIFT);
        assert_eq!(upper, KeyChord::plain(KeyCode::Char('J')));
        // Shift on an arrow stays distinct from the unmodified arrow.
        let plain = KeyChord::plain(KeyCode::Down);
        let shifted = KeyChord::new(KeyCode::Down, KeyModifiers::SHIFT);
        assert_ne!(plain, shifted);
    }

    #[test]
    fn default_map_resolves_builtin_keys() {
        let km = Keymap::default();
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Char('j'))),
            Some(Action::MoveDown)
        );
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Down)),
            Some(Action::MoveDown)
        );
        assert_eq!(
            km.action_for(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)),
            Some(Action::ReorderDown)
        );
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Char('q'))),
            Some(Action::Quit)
        );
        // An unbound key resolves to nothing.
        assert_eq!(km.action_for(KeyEvent::from(KeyCode::Char('z'))), None);
    }

    #[test]
    fn overrides_replace_defaults_and_win_conflicts() {
        let mut ov = Overrides::new();
        ov.insert("quit".to_string(), vec!["ctrl+q".to_string()]);
        // Rebind move-down onto 'x'.
        ov.insert("move-down".to_string(), vec!["x".to_string()]);
        let (km, warnings) = Keymap::from_overrides(ov);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

        // 'q' no longer quits; ctrl+q does.
        assert_eq!(km.action_for(KeyEvent::from(KeyCode::Char('q'))), None);
        assert_eq!(
            km.action_for(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(Action::Quit)
        );
        // 'j' no longer moves down (its defaults were replaced), 'x' does.
        assert_eq!(km.action_for(KeyEvent::from(KeyCode::Char('j'))), None);
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Char('x'))),
            Some(Action::MoveDown)
        );
        // Untouched actions keep their defaults.
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Char('a'))),
            Some(Action::AddTask)
        );
    }

    #[test]
    fn bad_entries_warn_but_are_preserved() {
        let mut ov = Overrides::new();
        ov.insert("not-an-action".to_string(), vec!["z".to_string()]);
        ov.insert("quit".to_string(), vec!["nonsense-key".to_string()]);
        let (km, warnings) = Keymap::from_overrides(ov);
        assert_eq!(warnings.len(), 2, "warnings: {warnings:?}");
        // Both raw entries survive for round-tripping.
        assert!(km.overrides().contains_key("not-an-action"));
        assert!(km.overrides().contains_key("quit"));
    }

    #[test]
    fn config_tokens_round_trip_through_parse() {
        for token in [
            "j",
            "J",
            "space",
            "tab",
            "enter",
            "esc",
            "/",
            "ctrl+c",
            "shift+down",
        ] {
            let chord = KeyChord::parse(token).expect("parses");
            // The canonical token re-parses to the same chord.
            let reparsed = KeyChord::parse(&chord.config_token()).expect("re-parses");
            assert_eq!(chord, reparsed, "token {token:?} did not round-trip");
        }
    }

    #[test]
    fn full_overrides_list_every_action_and_rebuild_identically() {
        let full = Keymap::default().to_full_overrides();
        // Every action is present.
        for action in Action::ALL {
            assert!(
                full.contains_key(action.config_name()),
                "missing {}",
                action.config_name()
            );
        }
        // Rebuilding from the full set is warning-free, complete, and behaves
        // exactly like the defaults.
        let (km, warnings) = Keymap::from_overrides(full);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert!(km.covers_all_actions());
        assert_eq!(
            km.action_for(KeyEvent::from(KeyCode::Char('j'))),
            Some(Action::MoveDown)
        );
        assert_eq!(
            km.action_for(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)),
            Some(Action::ReorderDown)
        );
    }

    #[test]
    fn full_overrides_keep_user_overrides_and_unknown_entries() {
        let mut ov = Overrides::new();
        ov.insert("quit".to_string(), vec!["Q".to_string()]);
        ov.insert("future-action".to_string(), vec!["z".to_string()]);
        let (km, _) = Keymap::from_overrides(ov);
        assert!(!km.covers_all_actions(), "partial config isn't complete");

        let full = km.to_full_overrides();
        // The user's override is kept, missing actions are filled with defaults,
        // and the unrecognized entry is preserved untouched.
        assert_eq!(full.get("quit").unwrap(), &vec!["Q".to_string()]);
        assert_eq!(
            full.get("move-down").unwrap(),
            &vec!["j".to_string(), "down".to_string()]
        );
        assert_eq!(full.get("future-action").unwrap(), &vec!["z".to_string()]);
    }
}
