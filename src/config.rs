//! Config pointer: a tiny JSON file that records where the user's data lives.
//!
//! Resolution order for the data dir:
//!   1. `$TUDO_DIR` env var (wins; handy for tests/scripting)
//!   2. the saved config pointer (`$TUDO_CONFIG` or `~/.config/tudo/config.json`)
//!   3. none -> first-run setup picks a location

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::keybind::Overrides;
use crate::theme::ThemeKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    #[serde(default)]
    pub theme: ThemeKind,
    /// Optional keybinding overrides: action name -> keys. Omitted entirely when
    /// empty so a default install's config stays minimal.
    #[serde(default, skip_serializing_if = "Overrides::is_empty")]
    pub keybindings: Overrides,
}

/// Location of the config pointer file.
pub fn config_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("TUDO_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    if let Some(home) = dirs::home_dir() {
        return Ok(home.join(".config").join("tudo").join("config.json"));
    }
    let base = dirs::config_dir().context("could not determine a config directory")?;
    Ok(base.join("tudo").join("config.json"))
}

/// Load the saved config pointer, if it exists.
pub fn load_config() -> Result<Option<Config>> {
    let cfg = config_path()?;
    if !cfg.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(&cfg)
        .with_context(|| format!("reading config {}", cfg.display()))?;
    let config: Config =
        serde_json::from_str(&data).with_context(|| format!("parsing config {}", cfg.display()))?;
    Ok(Some(config))
}

/// What to do at startup: open an existing data dir (with its theme) or run
/// first-run setup. `$TUDO_DIR` overrides the saved data dir but keeps any
/// saved theme and keybindings.
pub enum Startup {
    Open {
        data_dir: PathBuf,
        theme: ThemeKind,
        keybindings: Overrides,
    },
    FirstRun {
        theme: ThemeKind,
        keybindings: Overrides,
    },
}

/// Resolve startup state from env + saved config.
pub fn resolve() -> Result<Startup> {
    let saved = load_config()?;
    let saved_theme = saved.as_ref().map(|c| c.theme).unwrap_or_default();
    let saved_keys = saved
        .as_ref()
        .map(|c| c.keybindings.clone())
        .unwrap_or_default();

    if let Ok(p) = std::env::var("TUDO_DIR") {
        let path = PathBuf::from(p);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("creating data dir {}", path.display()))?;
        return Ok(Startup::Open {
            data_dir: path,
            theme: saved_theme,
            keybindings: saved_keys,
        });
    }
    if let Some(config) = saved {
        std::fs::create_dir_all(&config.data_dir)
            .with_context(|| format!("creating data dir {}", config.data_dir.display()))?;
        return Ok(Startup::Open {
            data_dir: config.data_dir,
            theme: config.theme,
            keybindings: config.keybindings,
        });
    }
    Ok(Startup::FirstRun {
        theme: saved_theme,
        keybindings: saved_keys,
    })
}

/// Persist the config pointer (data dir + theme).
pub fn save_config(config: &Config) -> Result<()> {
    let cfg = config_path()?;
    if let Some(parent) = cfg.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config dir {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&cfg, json).with_context(|| format!("writing config {}", cfg.display()))?;
    Ok(())
}

/// Suggested data-dir locations offered on first run, as (label, path) pairs.
/// Built from the real home dir so the labels match the actual paths on macOS.
pub fn first_run_options() -> Vec<(String, PathBuf)> {
    let mut opts = Vec::new();
    if let Some(home) = dirs::home_dir() {
        opts.push((
            "~/.local/share/tudo".to_string(),
            home.join(".local").join("share").join("tudo"),
        ));
        opts.push((
            "~/.config/tudo".to_string(),
            home.join(".config").join("tudo"),
        ));
        opts.push(("~/tudo".to_string(), home.join("tudo")));
    }
    opts.push((
        "./.tudo (current directory)".to_string(),
        PathBuf::from(".tudo"),
    ));
    opts
}

/// Expand a leading `~` in a user-typed path to the home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if trimmed == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(trimmed)
}
