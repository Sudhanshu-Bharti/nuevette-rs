//! The library: paths you already have, offered again before generating a
//! new one, and the `.nuevette.json` file a path is shared as.
//!
//! The file is versioned and keeps the path's id, so a shared online library
//! can later accept the same files without converting them.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::docs::topic_words;
use crate::model::LearningPath;

/// Written into every file, so other tools (and a future online library) can
/// tell what it is.
pub const FORMAT: &str = "nuevette.path";
/// Bump when the file changes in a way older versions can't read.
pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "nuevette.json";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathFile {
    format: String,
    version: u32,
    /// When it was exported (Unix seconds).
    exported_at: i64,
    /// The app version that wrote it.
    app_version: String,
    path: LearningPath,
}

/// The path as a file to share: its content without the sharer's progress,
/// card positions or history.
pub fn export(path: &LearningPath, now: i64) -> Result<String> {
    let mut path = path.clone();
    path.completed.clear();
    path.completed_at.clear();
    path.positions.clear();
    path.last_opened = None;
    path.building = None;
    let file = PathFile {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: now,
        app_version: env!("CARGO_PKG_VERSION").into(),
        path,
    };
    Ok(serde_json::to_string_pretty(&file)?)
}

/// Reads a shared path file. A bare path (as in Settings' "Export all
/// paths") is accepted too.
pub fn import(text: &str) -> Result<Vec<LearningPath>> {
    if let Ok(file) = serde_json::from_str::<PathFile>(text) {
        if file.format != FORMAT {
            bail!("this isn't a Nuevette path file");
        }
        if file.version > VERSION {
            bail!("this file was made by a newer version of Nuevette (format {}); update to open it", file.version);
        }
        return Ok(vec![file.path]);
    }
    if let Ok(paths) = serde_json::from_str::<Vec<LearningPath>>(text) {
        return Ok(paths);
    }
    match serde_json::from_str::<LearningPath>(text) {
        Ok(path) => Ok(vec![path]),
        Err(_) => bail!("this file isn't a Nuevette path"),
    }
}

/// A suggested file name, e.g. "rust-async-programming.nuevette.json".
pub fn file_name(path: &LearningPath) -> String {
    let slug: String = path
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    format!("{}.{EXTENSION}", if slug.is_empty() { "path".into() } else { slug })
}

/// Paths you already have for this topic: made from the same request, or
/// named with every word of it.
pub fn matches<'a>(paths: &'a [LearningPath], topic: &str) -> Vec<&'a LearningPath> {
    let wanted = topic_words(topic);
    if wanted.is_empty() {
        return Vec::new();
    }
    paths
        .iter()
        .filter(|path| {
            let same_request = path.topic_query.as_deref().is_some_and(|q| topic_words(q) == wanted);
            let named = topic_words(&path.name);
            same_request || wanted.iter().all(|w| named.iter().any(|n| n.starts_with(w.as_str())))
        })
        .take(3)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_paths, subtopic_key};

    #[test]
    fn exported_files_drop_progress_and_read_back() {
        let mut path = sample_paths().remove(0);
        path.completed.insert(subtopic_key(0, 0));
        let text = export(&path, 1).unwrap();
        let back = import(&text).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].id, path.id);
        assert!(back[0].completed.is_empty());
        assert_eq!(back[0].topics, path.topics);
    }
}
