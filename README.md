# tudo

A fast, local-first todo list for your terminal. Fully keyboard-driven (with mouse support), no cloud, no accounts - your tasks are plain JSON files on your own disk.

![tudo demo](tudo-demo.gif)

## Features

- **Multiple lists / projects**: switch between named lists in the sidebar.
- **Rich tasks**: priority, due date, tags, free-text notes, and one level of
  checkable subtasks.
- **Reorder freely**: nudge a task up/down or send it straight to the top or
  bottom of its list.
- **Archive, don't delete**: press `d` to tuck a task into the dimmed
  **Archived** list (pinned at the bottom of the sidebar) instead of losing it;
  `X` deletes for good, and `m` moves a task back out to unarchive it.
- **Keyboard first, mouse friendly**: vim keys and arrow keys; click rows,
  click a checkbox to toggle, scroll to move.
- **Search & filter**: substring search across titles/tags/notes and a
  status filter (all / active / done).
- **Human-readable storage**: one pretty-printed JSON file per list, written
  atomically so a crash can't corrupt your data.
- **Local-first**: nothing leaves your machine.

<p align="center">
  <a href="https://buymeacoffee.com/jolley">
    <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy me a coffee" height="60" width="217">
  </a>
</p>

## Install

### Quick install (prebuilt binary)

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/jolleydesign/tudo/main/install.sh | sh
```

This grabs the right binary for your platform from the latest [release](https://github.com/jolleydesign/tudo/releases) and installs it to
`~/.local/bin`. Set `TUDO_INSTALL_DIR` to install somewhere else, or `TUDO_VERSION` (e.g. `v0.1.0`) to pin a specific version.

**Updating:** re-run the install command above to upgrade to the latest release - it overwrites the existing binary in place. Check what you're running with `tudo --version`.

### With Homebrew

macOS and Linux (via [Homebrew](https://brew.sh)):

```sh
brew install jolleydesign/tudo/tudo
```

This taps `jolleydesign/homebrew-tudo` and installs the prebuilt binary. Upgrade later with `brew upgrade tudo`.

### With Cargo

From [crates.io](https://crates.io/crates/tudo-tui) - the crate is published as `tudo-tui` (the name `tudo` was taken), but the installed command is still `tudo`:

```sh
cargo install tudo-tui
```

Or build from source with a Rust toolchain (1.95+):

```sh
# straight from GitHub
cargo install --git https://github.com/jolleydesign/tudo

# or from a local checkout
cargo install --path .

# or just build and run
cargo run --release
```

`cargo install` puts a `tudo` binary on your `PATH` (usually `~/.cargo/bin`).

## First run

The first time you launch `tudo`, it asks where to keep your data and offers a
few standard locations (or a custom path). Your choice is remembered in a tiny
pointer file at `~/.config/tudo/config.json`. A starter list named **Tasks** is
created so you can begin immediately.

## Keybindings

Press `?` in the app for the same list grouped into an overlay. All of these keys are [customizable](#customizing-keybindings) in the config file, and the help overlay always shows your current bindings.

**Navigate**

| Key | Action |
|-----|--------|
| `Tab`, `h`/`l`, `←`/`→` | switch focus between the Lists and Tasks panes |
| `j`/`k`, `↑`/`↓` | move the selection |
| `Enter` | open a task's detail view / drill into a list |
| `Esc` | close a dialog / leave detail / clear the filter |

**Organize**

| Key | Action |
|-----|--------|
| `Space` | toggle the selected task (or subtask) done |
| `J`/`K`, `Shift`+`↓`/`↑` | move the selected task down / up within the list |
| `g`/`G` | send the selected task to the top / bottom of the list |
| `m` | move the selected task to another list (also how you unarchive) |

**Create & edit**

| Key | Action |
|-----|--------|
| `a` / `A` | add a task / add a list |
| `e` | edit the selected task's title, or rename the list when the Lists pane is focused |
| `s` | add a subtask |
| `p` | cycle priority (none → low → med → high) |
| `D` | set or clear the due date |
| `t` | edit tags |
| `n` | edit notes (multi-line: `Enter` for a newline, `Ctrl+S` to save) |

**Remove**

| Key | Action |
|-----|--------|
| `d` | archive the selected task; delete a list or subtask (asks to confirm) |
| `X` | permanently delete the selected task/list/subtask (asks to confirm) |

**Find & app**

| Key | Action |
|-----|--------|
| `/` | search (titles, tags, notes) |
| `f` | cycle the status filter (all / active / done) |
| `c` | copy the selected task (menu: full JSON / title / description) |
| `T` | open the theme picker |
| `S` | open settings (paths, data location) |
| `?` | show the help overlay |
| `q`, `Ctrl+C` | quit |

**Due-date input** accepts `YYYY-MM-DD`, `today`, `tomorrow`, or `+N` (N days
from today). An empty value clears the date.

**Mouse:** click a list or task to select it, click a task's checkbox to toggle it done, and use the scroll wheel to move within the focused pane.

## Themes

Press `T` to open the **theme picker**: browse with `↑/↓` to preview each theme live across the whole UI, `Enter` to apply, `Esc` to revert. Twelve palettes are built in - **Tokyo Night**, **Catppuccin Mocha**, **Dracula**,
**Nord**, **Gruvbox Dark**, **Gruvbox Material**, **Solarized Dark**, **One Dark**, **Rosé Pine**,
**Gotham**, **Black & White**, and **Terminal** (default). Your choice is
remembered in the config pointer.

The coloured themes are truecolor, so they look the same regardless of your terminal's own theme; **Terminal** does the opposite - it forces no background and uses the 16 ANSI colours, so the app adopts your terminal's scheme. Set `TUDO_THEME` (e.g. `TUDO_THEME=dracula` or `TUDO_THEME=none`) to override the theme for a single run.

## Settings & configuration

Press `S` to open the settings panel, which shows your **data directory**, the **config file** path, the storage format, the active theme, your list/task counts, and any environment overrides currently in effect.

Configuration is a small **JSON** file (not TOML) - `~/.config/tudo/config.json` by default (override with `$TUDO_CONFIG`). It holds the `data_dir`, the `theme`, and optional `keybindings`; everything else (your actual lists) lives as separate JSON files in the data directory. To move your data, press `S` then `d`, type a new path (`~` is allowed), and `Enter` - tudo moves your list files there and repoints the config.

## Customizing keybindings

tudo writes a full `keybindings` block into your `config.json` (on first run, and it fills in any missing actions on upgrade), so you don't have to author it from scratch - just edit the keys you want and restart tudo. Each entry maps an **action name** to the list of keys that trigger it; the list **replaces** that action's defaults, so include every key you want for it. For example, to add `Ctrl` alternatives to a few actions:

```json
{
  "data_dir": "/Users/you/.local/share/tudo",
  "theme": "dracula",
  "keybindings": {
    "move-down": ["j", "down", "ctrl+n"],
    "move-up": ["k", "up", "ctrl+p"],
    "quit": ["q", "ctrl+q"],
    "search": ["/", "ctrl+f"]
  }
}
```

**Key syntax:** a single character is that key, case-sensitive (`j` vs `J`). Named keys are `space`, `tab`, `enter`, `esc`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, and `insert`. Add modifiers with `ctrl+`, `alt+`, or `shift+` (e.g. `ctrl+q`, `shift+down`); for letters just use the uppercase letter (`J`) rather than `shift+j`.

**Action names** (defaults in parentheses):

| Action | Default | Does |
|--------|---------|------|
| `toggle-focus` | `tab` | switch focus between the Lists and Tasks panes |
| `focus-lists` / `focus-tasks` | `h`/`left`, `l`/`right` | focus a specific pane |
| `move-down` / `move-up` | `j`/`down`, `k`/`up` | move the selection |
| `activate` | `enter` | open a task's detail / drill into a list |
| `back` | `esc` | close a dialog / leave detail / clear the filter |
| `toggle-done` | `space` | toggle the selected task or subtask done |
| `reorder-down` / `reorder-up` | `J`/`shift+down`, `K`/`shift+up` | move the task down / up |
| `send-top` / `send-bottom` | `g` / `G` | send the task to the top / bottom |
| `move-task` | `m` | move the task to another list (also unarchives) |
| `add-task` / `add-list` | `a` / `A` | add a task / a list |
| `edit` | `e` | edit the task title, or rename the list |
| `add-subtask` | `s` | add a subtask |
| `cycle-priority` | `p` | cycle priority (none → low → med → high) |
| `set-due` | `D` | set or clear the due date |
| `set-tags` | `t` | edit tags |
| `set-notes` | `n` | edit notes |
| `archive` | `d` | archive the task; delete a list or subtask |
| `delete` | `X` | permanently delete (asks to confirm) |
| `search` | `/` | search titles, tags, and notes |
| `cycle-filter` | `f` | cycle the status filter (all / active / done) |
| `copy` | `c` | copy the selected task |
| `theme-picker` | `T` | open the theme picker |
| `settings` | `S` | open settings |
| `help` | `?` | show the help overlay |
| `quit` | `q` | quit |

Notes: `Ctrl+C` always quits and can't be disabled. Only these main (Normal/Detail) keys are configurable - text entry, confirm prompts, and the pickers keep fixed keys. Entries with an unknown action name or an unrecognized key are ignored (with a brief notice when the app starts) but left untouched in your config, so a typo won't wipe the rest of your settings.

## Storage format

Each list is a single JSON file (`<slug>.json`) in your data directory:

```json
{
  "name": "Work",
  "tasks": [
    {
      "id": "f7c1…",
      "title": "Ship the TUI",
      "done": false,
      "priority": "high",
      "due": "2026-07-01",
      "tags": ["rust", "urgent"],
      "notes": "the big one",
      "subtasks": [
        { "id": "a2…", "title": "data model", "done": true }
      ],
      "created": "2026-06-28T14:40:00Z",
      "completed_at": null
    }
  ]
}
```

Edit these by hand or keep them in a git repo - they're just text.

## Environment variables

- `TUDO_DIR` - override the data directory (skips the saved config; great for
  scoping tasks to a project or for scripting).
- `TUDO_CONFIG` - override the location of the config pointer file.
- `TUDO_THEME` - override the theme for one run (`tokyo-night`, `catppuccin`,
  `dracula`, `nord`, `gruvbox`, `gruvbox-material`, `solarized`, `one-dark`,
  `rose-pine`, `gotham`, `black-white`, or `none`/`terminal`).

## Development

```sh
cargo test       # unit + headless render tests (no real terminal needed)
cargo clippy --all-targets
cargo run
```

Source layout: `model` (types), `storage` (JSON I/O), `config` (data-dir resolution), `keybind` (configurable keymap), `app` (state + actions), `event` (key/mouse mapping), `ui` (rendering). The action logic and rendering are terminal-free, so they're tested directly with `tempfile` and ratatui's `TestBackend`.
