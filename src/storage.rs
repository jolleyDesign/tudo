//! Reading and writing lists as one JSON file per list, written atomically.
//!
//! Notes live alongside the list files in a `notes/` subdirectory: one folder
//! per notebook, one `.md` file (with YAML frontmatter) per note.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::model::{List, Note, Notebook};

/// Reserved filename: the config pointer can share the data dir (e.g. when the
/// user picks `~/.config/tudo`), so it must never be read or written as a list.
const RESERVED: &str = "config.json";

/// Turn a list name into a filesystem-safe slug for its filename.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !slug.is_empty() && !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("list");
    }
    // Don't let a list named "config" clobber the config pointer.
    if slug == "config" {
        slug.push_str("-list");
    }
    slug
}

/// Pick a slug for `name` that does not collide with an existing `.json` file.
pub fn unique_slug(data_dir: &Path, name: &str) -> String {
    let base = slugify(name);
    let mut slug = base.clone();
    let mut n = 2;
    while data_dir.join(format!("{slug}.json")).exists() {
        slug = format!("{base}-{n}");
        n += 1;
    }
    slug
}

/// Load every `*.json` list file in `data_dir`, sorted by name (case-insensitive).
/// Skips the temp files used by atomic writes. A `.tmp` is ignored.
pub fn load_lists(data_dir: &Path) -> Result<Vec<List>> {
    let mut lists = Vec::new();
    if data_dir.exists() {
        for entry in std::fs::read_dir(data_dir)
            .with_context(|| format!("reading data dir {}", data_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            // The config pointer may live alongside lists; never treat it as one.
            if path.file_name().and_then(|n| n.to_str()) == Some(RESERVED) {
                continue;
            }
            let data = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let mut list: List = serde_json::from_str(&data)
                .with_context(|| format!("parsing {}", path.display()))?;
            list.slug = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            lists.push(list);
        }
    }
    lists.sort_by_key(|a| a.name.to_lowercase());
    Ok(lists)
}

fn list_path(data_dir: &Path, list: &List) -> std::path::PathBuf {
    let slug = if list.slug.is_empty() {
        slugify(&list.name)
    } else {
        list.slug.clone()
    };
    data_dir.join(format!("{slug}.json"))
}

/// Save a single list atomically: write to `<slug>.json.tmp` then rename.
pub fn save_list(data_dir: &Path, list: &List) -> Result<()> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("creating data dir {}", data_dir.display()))?;
    let path = list_path(data_dir, list);
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(list)?;
    std::fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("finalizing {}", path.display()))?;
    Ok(())
}

/// Delete a list's file from disk (no error if it is already gone).
pub fn delete_list_file(data_dir: &Path, list: &List) -> Result<()> {
    let path = list_path(data_dir, list);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

// --- notes ------------------------------------------------------------------

/// The `notes/` subdirectory that holds every notebook folder.
pub fn notes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("notes")
}

/// Turn a notebook name into a readable, path-safe folder name — spaces and case
/// are kept (these folders are meant to be browsable), only characters that are
/// unsafe in a path are dropped or replaced.
pub fn sanitize_notebook_name(name: &str) -> String {
    let mut out = String::new();
    let mut prev_space = false;
    for c in name.trim().chars() {
        // Drop path separators, control chars and the handful of characters that
        // are illegal in filenames on common platforms.
        if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
            continue;
        }
        if c.is_whitespace() {
            if !out.is_empty() && !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    let out = out.trim().trim_start_matches('.').trim().to_string();
    if out.is_empty() {
        "notebook".to_string()
    } else {
        out
    }
}

/// Pick a notebook folder name for `name` that doesn't collide with an existing
/// folder under `notes/` (or with the reserved Archived folder).
pub fn unique_notebook_slug(data_dir: &Path, name: &str) -> String {
    let base = sanitize_notebook_name(name);
    let dir = notes_dir(data_dir);
    let mut slug = base.clone();
    let mut n = 2;
    while dir.join(&slug).exists() || slug == crate::model::ARCHIVE_NOTEBOOK_SLUG {
        slug = format!("{base} {n}");
        n += 1;
    }
    slug
}

/// Pick a note filename stem (a kebab-case slug) that doesn't collide with an
/// existing `.md` file in the notebook folder.
pub fn unique_note_slug(data_dir: &Path, notebook_slug: &str, title: &str) -> String {
    let base = slugify(title);
    let dir = notes_dir(data_dir).join(notebook_slug);
    let mut slug = base.clone();
    let mut n = 2;
    while dir.join(format!("{slug}.md")).exists() {
        slug = format!("{base}-{n}");
        n += 1;
    }
    slug
}

/// Full path to a note file.
pub fn note_path(data_dir: &Path, notebook_slug: &str, note_slug: &str) -> PathBuf {
    notes_dir(data_dir)
        .join(notebook_slug)
        .join(format!("{note_slug}.md"))
}

/// The file that records the manual notebook order (one slug per line). Lives
/// inside `notes/` so it travels with the data; absent means alphabetical.
fn notebook_order_path(data_dir: &Path) -> PathBuf {
    notes_dir(data_dir).join(".order")
}

/// Load the saved notebook order (slugs, top to bottom). Empty if none saved.
pub fn load_notebook_order(data_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(notebook_order_path(data_dir))
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Persist the manual notebook order.
pub fn save_notebook_order(data_dir: &Path, slugs: &[String]) -> Result<()> {
    let dir = notes_dir(data_dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = notebook_order_path(data_dir);
    std::fs::write(&path, slugs.join("\n"))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Convert a filesystem modified time to a UTC timestamp, falling back to now.
fn file_modified(path: &Path) -> DateTime<Utc> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now())
}

/// Load one note from its `.md` file (used both when loading all notebooks and
/// when reloading a note after it's been edited in an external editor).
pub fn load_note(path: &Path) -> Result<Note> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let slug = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Note::from_file(&contents, slug, file_modified(path)))
}

/// Load every notebook (a folder under `notes/`) with its `.md` notes. Notebooks
/// are sorted by name; notes within each are ordered by their stored `order`
/// (unordered ones falling back to title), which is the natural "manual" order.
pub fn load_notebooks(data_dir: &Path) -> Result<Vec<Notebook>> {
    let mut notebooks = Vec::new();
    let dir = notes_dir(data_dir);
    if !dir.exists() {
        return Ok(notebooks);
    }
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let slug = entry.file_name().to_string_lossy().into_owned();
        let mut notes = Vec::new();
        for note_entry in
            std::fs::read_dir(&path).with_context(|| format!("reading {}", path.display()))?
        {
            let note_entry = note_entry?;
            let note_path = note_entry.path();
            if note_path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            notes.push(load_note(&note_path)?);
        }
        // Stored order first (unordered notes last), then title as a stable
        // tiebreak — this is the "manual" order the reorder keys nudge.
        notes.sort_by(|a, b| {
            a.order
                .unwrap_or(u32::MAX)
                .cmp(&b.order.unwrap_or(u32::MAX))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        // The reserved Archived folder gets its friendly display name.
        let name = if slug == crate::model::ARCHIVE_NOTEBOOK_SLUG {
            crate::model::ARCHIVE_NOTEBOOK_NAME.to_string()
        } else {
            slug.clone()
        };
        notebooks.push(Notebook {
            name,
            notes,
            slug,
            order: None,
        });
    }
    // Apply the saved manual order, then sort by (order, name) as a default.
    let order = load_notebook_order(data_dir);
    for nb in &mut notebooks {
        nb.order = order.iter().position(|s| *s == nb.slug).map(|i| i as u32);
    }
    notebooks.sort_by(|a, b| {
        a.order
            .unwrap_or(u32::MAX)
            .cmp(&b.order.unwrap_or(u32::MAX))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(notebooks)
}

/// Save a single note atomically into its notebook folder (creating the folder
/// if needed): write `<slug>.md.tmp` then rename.
pub fn save_note(data_dir: &Path, notebook_slug: &str, note: &Note) -> Result<()> {
    let dir = notes_dir(data_dir).join(notebook_slug);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{}.md", note.slug));
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, note.to_file_string())
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("finalizing {}", path.display()))?;
    Ok(())
}

/// Delete a note's file (no error if it is already gone).
pub fn delete_note_file(data_dir: &Path, notebook_slug: &str, note_slug: &str) -> Result<()> {
    let path = note_path(data_dir, notebook_slug, note_slug);
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

/// Create an (empty) notebook folder.
pub fn create_notebook_dir(data_dir: &Path, slug: &str) -> Result<()> {
    let dir = notes_dir(data_dir).join(slug);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(())
}

/// Delete a notebook folder and everything in it.
pub fn delete_notebook_dir(data_dir: &Path, slug: &str) -> Result<()> {
    let dir = notes_dir(data_dir).join(slug);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

/// Rename a notebook folder (used when renaming a notebook).
pub fn rename_notebook_dir(data_dir: &Path, old_slug: &str, new_slug: &str) -> Result<()> {
    let base = notes_dir(data_dir);
    std::fs::create_dir_all(&base).with_context(|| format!("creating {}", base.display()))?;
    let from = base.join(old_slug);
    let to = base.join(new_slug);
    std::fs::rename(&from, &to)
        .with_context(|| format!("renaming {} to {}", from.display(), to.display()))?;
    Ok(())
}
