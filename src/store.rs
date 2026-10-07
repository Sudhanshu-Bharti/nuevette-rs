//! `PathStore` — the single shared source of learning paths. Views hold an
//! `Entity<PathStore>` and observe it; nothing else keeps its own copy.
//!
//! Paths persist to `%APPDATA%\nuevette\paths.json` (override with
//! `NUEVETTE_DATA_DIR`). The first run seeds the bundled samples. A file that
//! fails to parse is never overwritten: the app runs on the samples and
//! reports the problem instead.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use gpui::Context;
use serde::{Deserialize, Serialize};

use crate::model::{LearningPath, Topic, sample_paths, subtopic_key};

const FILE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    paths: Vec<LearningPath>,
}

/// Most recent first.
pub struct PathStore {
    paths: Vec<LearningPath>,
    /// `None` disables saving (no data dir, or the existing file is unreadable).
    file: Option<PathBuf>,
    problem: Option<String>,
}

impl PathStore {
    /// Loads from `$NUEVETTE_DATA_DIR` if set (used by dev scripts), otherwise
    /// from `nuevette\paths.json` under the user's data directory.
    pub fn load() -> Self {
        Self::load_from(crate::settings::data_dir().map(|dir| dir.join("paths.json")))
    }

    pub fn load_from(file: Option<PathBuf>) -> Self {
        let Some(file) = file else {
            return Self {
                paths: sample_paths(),
                file: None,
                problem: Some("No data folder found; paths won't be saved".into()),
            };
        };
        match read_paths(&file) {
            Ok(Some(paths)) => Self {
                paths,
                file: Some(file),
                problem: None,
            },
            Ok(None) => {
                let mut store = Self {
                    paths: sample_paths(),
                    file: Some(file),
                    problem: None,
                };
                store.save();
                store
            }
            Err(error) => Self {
                paths: sample_paths(),
                file: None,
                problem: Some(format!("Couldn't read saved paths ({error:#}); changes won't be saved")),
            },
        }
    }

    pub fn paths(&self) -> &[LearningPath] {
        &self.paths
    }

    pub fn get(&self, id: &str) -> Option<&LearningPath> {
        self.paths.iter().find(|p| p.id == id)
    }

    /// A user-facing description of a load or save failure, if any.
    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Inserts `path` at the front, or moves it there if it already exists.
    pub fn add(&mut self, path: LearningPath, cx: &mut Context<Self>) {
        upsert_recent(&mut self.paths, path);
        self.save();
        cx.notify();
    }

    /// Removes a path, returning it and its position so it can be restored.
    pub fn remove(&mut self, id: &str, cx: &mut Context<Self>) -> Option<(LearningPath, usize)> {
        let index = self.paths.iter().position(|p| p.id == id)?;
        let path = self.paths.remove(index);
        self.save();
        cx.notify();
        Some((path, index))
    }

    /// Puts back a path returned by [`Self::remove`], at its old position.
    pub fn restore(&mut self, path: LearningPath, index: usize, cx: &mut Context<Self>) {
        self.paths.retain(|p| p.id != path.id);
        self.paths.insert(index.min(self.paths.len()), path);
        self.save();
        cx.notify();
    }

    /// Removes every path, returning them so they can be restored.
    pub fn clear(&mut self, cx: &mut Context<Self>) -> Vec<LearningPath> {
        let paths = std::mem::take(&mut self.paths);
        self.save();
        cx.notify();
        paths
    }

    /// Puts back paths returned by [`Self::clear`], ahead of any added since.
    pub fn restore_all(&mut self, paths: Vec<LearningPath>, cx: &mut Context<Self>) {
        let added = std::mem::take(&mut self.paths);
        self.paths = paths;
        for path in added.into_iter().rev() {
            upsert_recent(&mut self.paths, path);
        }
        self.save();
        cx.notify();
    }

    /// Applies `edit` to the path with `id`, then saves and notifies.
    /// Returns false when no such path exists.
    pub fn update_path(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut LearningPath),
    ) -> bool {
        let Some(path) = self.paths.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        edit(path);
        self.save();
        cx.notify();
        true
    }

    /// Replaces a path's generated content (e.g. as it streams in), keeping
    /// what the learner did with it: progress, positions, when it was opened.
    pub fn replace_content(&mut self, path: LearningPath, cx: &mut Context<Self>) {
        let id = path.id.clone();
        self.update_path(&id, cx, move |current| {
            let mut next = path;
            next.completed = std::mem::take(&mut current.completed);
            next.completed_at = std::mem::take(&mut current.completed_at);
            next.positions = std::mem::take(&mut current.positions);
            next.last_opened = current.last_opened;
            *current = next;
        });
    }

    /// Marks a subtopic done, or not done again.
    pub fn toggle_done(&mut self, id: &str, topic: usize, subtopic: usize, cx: &mut Context<Self>) {
        self.update_path(id, cx, |path| {
            let key = subtopic_key(topic, subtopic);
            if path.completed.remove(&key) {
                path.completed_at.remove(&key);
            } else {
                path.completed_at.insert(key.clone(), crate::stats::now_secs());
                path.completed.insert(key);
            }
        });
    }

    /// Records that the path was opened, for "last opened" and ordering.
    pub fn touch(&mut self, id: &str, cx: &mut Context<Self>) {
        self.update_path(id, cx, |path| path.last_opened = Some(crate::stats::now_secs()));
    }

    pub fn set_position(&mut self, id: &str, node: String, at: [f32; 2], cx: &mut Context<Self>) {
        self.update_path(id, cx, |path| {
            path.positions.insert(node, at);
        });
    }

    pub fn reset_positions(&mut self, id: &str, cx: &mut Context<Self>) {
        self.update_path(id, cx, |path| path.positions.clear());
    }

    pub fn rename(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if !name.is_empty() {
            self.update_path(id, cx, |path| path.name = name);
        }
    }

    /// Swaps in a regenerated topic. Its subtopics are new, so their
    /// completion marks and dragged positions no longer apply.
    pub fn replace_topic(&mut self, id: &str, index: usize, topic: Topic, cx: &mut Context<Self>) {
        self.update_path(id, cx, |path| {
            if index >= path.topics.len() {
                return;
            }
            let prefix = format!("{index}.");
            path.completed.retain(|key| !key.starts_with(&prefix));
            path.completed_at.retain(|key, _| !key.starts_with(&prefix));
            path.positions.retain(|key, _| !key.starts_with(&format!("s{prefix}")));
            path.topics[index] = topic;
            path.normalize_times();
        });
    }

    fn save(&mut self) {
        if let Some(file) = &self.file {
            if let Err(error) = write_paths(file, &self.paths) {
                self.problem = Some(format!("Couldn't save paths ({error:#})"));
            } else {
                self.problem = None;
            }
        }
    }
}

fn upsert_recent(paths: &mut Vec<LearningPath>, path: LearningPath) {
    paths.retain(|p| p.id != path.id);
    paths.insert(0, path);
}

/// `Ok(None)` when the file doesn't exist yet.
fn read_paths(file: &Path) -> Result<Option<Vec<LearningPath>>> {
    let json = match fs::read_to_string(file) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", file.display())),
    };
    let stored: StoreFile =
        serde_json::from_str(&json).with_context(|| format!("parsing {}", file.display()))?;
    Ok(Some(stored.paths))
}

/// Writes to a sibling temp file, then renames, so a crash never leaves a
/// half-written store behind.
fn write_paths(file: &Path, paths: &[LearningPath]) -> Result<()> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&StoreFile {
        version: FILE_VERSION,
        paths: paths.to_vec(),
    })?;
    let temp = file.with_extension("json.tmp");
    fs::write(&temp, json).with_context(|| format!("writing {}", temp.display()))?;
    fs::rename(&temp, file).with_context(|| format!("replacing {}", file.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(id: &str) -> LearningPath {
        LearningPath {
            id: id.into(),
            name: format!("Path {id}"),
            description: String::new(),
            estimated_time: "1h".into(),
            topics: Vec::new(),
            source_url: None,
            generated_by: None,
            level: None,
            completed: Default::default(),
            completed_at: Default::default(),
            last_opened: None,
            building: None,
            positions: Default::default(),
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nuevette-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("paths.json")
    }

    #[test]
    fn upsert_moves_existing_to_front_and_never_drops_paths() {
        let mut paths: Vec<_> = (0..20).map(|i| path(&i.to_string())).collect();
        upsert_recent(&mut paths, path("5"));
        assert_eq!(paths[0].id, "5");
        assert_eq!(paths.len(), 20);
        assert_eq!(paths.iter().filter(|p| p.id == "5").count(), 1);

        upsert_recent(&mut paths, path("new"));
        assert_eq!(paths[0].id, "new");
        assert_eq!(paths.len(), 21, "a library keeps every path");
    }

    #[test]
    fn save_and_load_round_trip() {
        let file = temp_file("round-trip");
        write_paths(&file, &[path("a"), path("b")]).unwrap();
        let loaded = read_paths(&file).unwrap().unwrap();
        assert_eq!(loaded.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(!file.with_extension("json.tmp").exists());
    }

    #[test]
    fn first_run_seeds_samples_to_disk() {
        let file = temp_file("first-run");
        let store = PathStore::load_from(Some(file.clone()));
        assert_eq!(store.paths().len(), sample_paths().len());
        assert!(store.problem().is_none());
        assert_eq!(read_paths(&file).unwrap().unwrap().len(), sample_paths().len());
    }

    #[test]
    fn corrupt_file_is_reported_and_left_untouched() {
        let file = temp_file("corrupt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "{ not json").unwrap();
        let mut store = PathStore::load_from(Some(file.clone()));
        assert!(store.problem().unwrap().contains("Couldn't read saved paths"));
        store.save();
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
    }
}
