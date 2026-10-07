//! Map commands beyond pointer input: keyboard navigation, topic focus,
//! renaming and Markdown export.

use ely_gpui_component::canvas::{Frame, Viewport};
use gpui::{AppContext, Context};

use super::keys::{Direction, neighbor};
use super::layout::{self, NodeKind};
use super::{FIT_MARGIN, MapEvent, MindMapView, export};

impl MindMapView {
    /// Arrow keys: move the selection and keep it in view.
    pub(super) fn move_selection(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let from = self.selected.map(|ix| self.nodes[ix].kind);
        let Some(to) = neighbor(&self.nodes, from, direction) else {
            return;
        };
        let Some(ix) = self.nodes.iter().position(|n| n.kind == to) else {
            return;
        };
        self.selected = Some(ix);
        self.ensure_visible(ix);
        if let Some(target) = self.pending_glide.take() {
            self.animate_to(target, cx);
        }
        cx.notify();
    }

    /// Pans just enough to bring a node fully into the visible area.
    pub(super) fn ensure_visible(&mut self, ix: usize) {
        let Some((w, h)) = self.view_size() else {
            return;
        };
        // Called once the selection is set, so the inspector is open.
        let (w, h) = self.uncovered((w, h));
        let node = &self.nodes[ix];
        let (left, top) = self.viewport.to_view(node.pos.tuple());
        let (right, bottom) = self.viewport.to_view(node.pos.add(node.size).tuple());
        let margin = 24.;
        let dx = if left < margin {
            left - margin
        } else if right > w - margin {
            right - (w - margin)
        } else {
            0.
        };
        let dy = if top < margin {
            top - margin
        } else if bottom > h - margin {
            bottom - (h - margin)
        } else {
            0.
        };
        if dx != 0. || dy != 0. {
            self.pending_glide = Some(self.viewport.panned((-dx, -dy)));
        }
    }

    /// Dims every other topic and fits the view to this one; again to undo.
    pub(super) fn toggle_focus(&mut self, topic: usize, cx: &mut Context<Self>) {
        if self.focus_topic == Some(topic) {
            self.focus_topic = None;
            self.fit_view(cx);
            return;
        }
        self.focus_topic = Some(topic);
        let column: Vec<_> = self
            .nodes
            .iter()
            .filter(|n| in_topic(n.kind, topic))
            .cloned()
            .collect();
        if let Some((w, h)) = self.view_size() {
            let (min, max) = layout::bounds(&column);
            let frame = Frame::new(min.x, min.y, max.x - min.x, max.y - min.y);
            self.animate_to(Viewport::fitting(frame, self.uncovered((w, h)), FIT_MARGIN), cx);
        }
        cx.notify();
    }

    pub(super) fn is_dimmed(&self, kind: NodeKind) -> bool {
        self.focus_topic
            .is_some_and(|topic| kind != NodeKind::Root && !in_topic(kind, topic))
    }

    pub(super) fn rename(&mut self, name: String, cx: &mut Context<Self>) {
        let id = self.path_id.clone();
        self.store.update(cx, |store, cx| store.rename(&id, name, cx));
    }

    /// Asks where to save, then writes the path as a Markdown checklist.
    pub(super) fn export_markdown(&mut self, cx: &mut Context<Self>) {
        let markdown = export::to_markdown(&self.path);
        let suggested = export::file_name(&self.path);
        let folder = dirs::document_dir().or_else(dirs::home_dir).unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&folder, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(file))) = chosen.await else {
                return;
            };
            let written = cx
                .background_spawn(async move { std::fs::write(&file, markdown).map(|()| file) })
                .await;
            this.update(cx, |_, cx| {
                cx.emit(match written {
                    Ok(file) => MapEvent::Done {
                        title: "Exported as Markdown".into(),
                        body: file.display().to_string(),
                    },
                    Err(error) => MapEvent::Failed {
                        title: "Couldn't export".into(),
                        body: error.to_string(),
                    },
                })
            })
            .ok();
        })
        .detach();
    }
}

fn in_topic(kind: NodeKind, topic: usize) -> bool {
    matches!(kind, NodeKind::Topic(t) | NodeKind::Subtopic(t, _) if t == topic)
}
