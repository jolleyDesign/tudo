//! Reading and writing lists as one JSON file per list, written atomically.

use anyhow::{Context, Result};
use std::path::Path;

use crate::model::List;

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
