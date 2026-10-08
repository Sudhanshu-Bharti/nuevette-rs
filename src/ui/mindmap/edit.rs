//! Editing a path on the map: rename, add, move and delete topics and steps,
//! and set a step's depth. Every change goes through the store (so it is
//! saved), keeps done marks on the right steps, and selects what was edited.
//! Deleting offers Undo through the app's toast.

use gpui::Context;

use super::layout::NodeKind;
use super::{MapEvent, MindMapView};
use crate::model::{Importance, LearningPath, Subtopic};

impl MindMapView {
    /// Applies `edit` to the path, then selects whatever it returns. Nothing
    /// changes while the path is still being written.
    fn edit_path(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut LearningPath) -> Option<NodeKind>) {
        if self.path.building.is_some() {
            return;
        }
        let id = self.path_id.clone();
        let mut select = None;
        self.store.update(cx, |store, cx| {
            store.update_path(&id, cx, |path| {
                select = edit(path);
                path.refresh_times();
            });
        });
        self.reselect = select;
        cx.notify();
    }

    /// Like `edit_path`, for changes that remove something: the app offers
    /// to put the old path back.
    fn edit_with_undo(
        &mut self,
        title: &str,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&mut LearningPath) -> Option<NodeKind>,
    ) {
        if self.path.building.is_some() {
            return;
        }
        let before = Box::new(self.path.clone());
        self.edit_path(cx, edit);
        cx.emit(MapEvent::Edited { title: title.to_string(), undo: before });
    }

    pub(super) fn rename_node(&mut self, kind: NodeKind, name: String, cx: &mut Context<Self>) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.edit_path(cx, |path| {
            match kind {
                NodeKind::Root => path.name = name,
                NodeKind::Topic(ti) => path.topics[ti].name = name,
                NodeKind::Subtopic(ti, si) => path.topics[ti].subtopics[si].name = name,
            }
            Some(kind)
        });
    }

    /// Moves a step one place left (-1) or right (+1) in its topic.
    pub(super) fn move_step(&mut self, ti: usize, si: usize, by: isize, cx: &mut Context<Self>) {
        let Some(to) = si.checked_add_signed(by).filter(|to| *to < self.path.topics[ti].subtopics.len()) else {
            return;
        };
        self.edit_path(cx, |path| {
            path.move_subtopic(ti, si, to);
            Some(NodeKind::Subtopic(ti, to))
        });
    }

    /// Adds a blank step after `after` (or at the end), ready to rename.
    pub(super) fn add_step(&mut self, ti: usize, after: Option<usize>, cx: &mut Context<Self>) {
        self.edit_path(cx, |path| {
            let at = after.map_or(path.topics[ti].subtopics.len(), |si| si + 1);
            path.insert_subtopic(
                ti,
                at,
                Subtopic {
                    name: "New step".into(),
                    description: String::new(),
                    estimated_time: String::new(),
                    estimated_hours: Some(1.),
                    technologies_and_concepts: Vec::new(),
                    prerequisites: Vec::new(),
                    resources: Vec::new(),
                    source: None,
                    importance: Importance::Core,
                },
            );
            Some(NodeKind::Subtopic(ti, at))
        });
    }

    pub(super) fn delete_step(&mut self, ti: usize, si: usize, cx: &mut Context<Self>) {
        self.edit_with_undo("Step deleted", cx, |path| {
            path.remove_subtopic(ti, si);
            Some(NodeKind::Topic(ti))
        });
    }

    /// Moves a topic one row up (-1) or down (+1).
    pub(super) fn move_topic(&mut self, ti: usize, by: isize, cx: &mut Context<Self>) {
        let Some(to) = ti.checked_add_signed(by).filter(|to| *to < self.path.topics.len()) else {
            return;
        };
        self.edit_path(cx, |path| {
            path.move_topic(ti, to);
            Some(NodeKind::Topic(to))
        });
    }

    pub(super) fn delete_topic(&mut self, ti: usize, cx: &mut Context<Self>) {
        self.edit_with_undo("Topic deleted", cx, |path| {
            path.remove_topic(ti);
            Some(NodeKind::Root)
        });
    }

    pub(super) fn set_importance(&mut self, ti: usize, si: usize, importance: Importance, cx: &mut Context<Self>) {
        self.edit_path(cx, |path| {
            path.topics[ti].subtopics[si].importance = importance;
            Some(NodeKind::Subtopic(ti, si))
        });
    }
}
