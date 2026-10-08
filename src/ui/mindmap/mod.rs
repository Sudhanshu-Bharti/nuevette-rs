//! Mind-map stage for one learning path, on Ely's `InfiniteCanvas` (dot grid,
//! drag to pan, Ctrl+wheel to zoom). Cards and edges are drawn in a layer
//! over it; cards can be dragged and selected, and the selection opens an
//! inspector.

mod commands;
mod detail;
mod edges;
mod edit;
mod expand;
mod export;
mod header;
mod keys;
mod layout;
mod motion;
mod node;
mod outline;

use ely_gpui_component::canvas::{Frame, InfiniteCanvas, Viewport};
use ely_gpui_component::theme::ActiveTheme;
use gpui::{
    App, Bounds, Context, DispatchPhase, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render,
    Subscription, Task, Window, canvas, div, prelude::*,
};

use crate::actions::{
    Deselect, FitView, SelectDown, SelectLeft, SelectRight, SelectUp, ToggleDone, ZoomIn, ZoomOut,
};
use keys::Direction;
use crate::model::LearningPath;
use crate::store::PathStore;
use crate::ui::glass;
use edges::{Edge, EdgeLayer};
use layout::{MapNode, NodeKind, Vec2};

/// Pointer travel (px) under which a press on empty canvas is a click.
const CLICK_SLOP: f32 = 3.;
const FIT_MARGIN: f32 = 48.;
/// The farthest out a selected card is shown when the camera moves to it.
const READABLE_ZOOM: f32 = 0.6;
const ZOOM_STEP: f32 = 1.25;
/// Width of the inspector panel, in px.
pub(super) const INSPECTOR_WIDTH: f32 = 340.;
/// Gap between the floating inspector and the canvas edges, in px.
pub(super) const INSPECTOR_INSET: f32 = 12.;
/// How much of the canvas the open inspector covers.
pub(super) const INSPECTOR_COVER: f32 = INSPECTOR_WIDTH + 2. * INSPECTOR_INSET;

/// What the map reports to the app, which shows it as a toast.
pub enum MapEvent {
    /// The learner stopped a path that is still being written.
    StopBuild,
    Done { title: String, body: String },
    Failed { title: String, body: String },
    /// Something was removed from the path; `undo` is the path before.
    Edited { title: String, undo: Box<LearningPath> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ViewMode {
    Map,
    Outline,
}

struct NodeDrag {
    ix: usize,
    /// Pointer position relative to the node's top-left, in canvas units.
    grab: Vec2,
}

pub struct MindMapView {
    store: Entity<PathStore>,
    path_id: String,
    /// The store's copy of the path, refreshed whenever the store changes.
    path: LearningPath,
    nodes: Vec<MapNode>,
    viewport: Viewport,
    /// The plane's bounds in window coordinates, measured each frame.
    bounds: Option<Bounds<Pixels>>,
    needs_fit: bool,
    drag: Option<NodeDrag>,
    /// Where a press on the plane started, to tell clicks from pans.
    press: Option<Point<Pixels>>,
    selected: Option<usize>,
    /// A node to select and center once the view has been measured.
    pending_select: Option<NodeKind>,
    /// A topic shown on its own, with the rest dimmed.
    focus_topic: Option<usize>,
    expanding: Option<expand::Expanding>,
    view_mode: ViewMode,
    /// The camera glide in flight; dropping it stops the glide.
    camera: Option<Task<()>>,
    /// A viewport `ensure_visible` wants; the caller starts the glide.
    pending_glide: Option<Viewport>,
    /// What to select once an edit has been laid out again.
    reselect: Option<NodeKind>,
    focus_handle: FocusHandle,
    _store_sub: Subscription,
}

impl EventEmitter<MapEvent> for MindMapView {}

impl Focusable for MindMapView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MindMapView {
    /// `path_id` must name a path in `store`.
    pub fn new(store: Entity<PathStore>, path_id: String, cx: &mut Context<Self>) -> Self {
        let path = store
            .read(cx)
            .get(&path_id)
            .cloned()
            .unwrap_or_else(|| panic!("mind map opened for unknown path {path_id}"));
        let _store_sub = cx.observe(&store, |this, store, cx| this.refresh(store, cx));
        Self {
            nodes: layout::layout_with_positions(&path),
            store,
            path_id,
            path,
            _store_sub,
            viewport: Viewport::new(-48., -48., 1.),
            bounds: None,
            needs_fit: true,
            drag: None,
            press: None,
            selected: None,
            pending_select: None,
            focus_topic: None,
            expanding: None,
            view_mode: ViewMode::Map,
            camera: None,
            pending_glide: None,
            reselect: None,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Picks up store changes. A changed structure (an expanded topic, a
    /// reset layout) is laid out again; the selection is kept by kind.
    fn refresh(&mut self, store: Entity<PathStore>, cx: &mut Context<Self>) {
        let Some(path) = store.read(cx).get(&self.path_id).cloned() else {
            return;
        };
        let shape = |p: &LearningPath| -> Vec<usize> { p.topics.iter().map(|t| t.subtopics.len()).collect() };
        let relayout = shape(&path) != shape(&self.path)
            || path.topics.iter().zip(&self.path.topics).any(|(a, b)| a.subtopics != b.subtopics)
            || (path.positions.is_empty() && !self.path.positions.is_empty());
        // While a path is being written it grows under the learner's eyes;
        // keep all of it in view until they pick something to look at.
        let growing = path.building.is_some() || self.path.building.is_some();
        let refit = relayout && growing && self.selected.is_none() && self.drag.is_none();
        if relayout && self.drag.is_none() {
            let selected = self.selected.map(|ix| self.nodes[ix].kind);
            self.nodes = layout::layout_with_positions(&path);
            self.selected = selected.and_then(|kind| self.nodes.iter().position(|n| n.kind == kind));
        }
        self.path = path;
        if let Some(kind) = self.reselect.take() {
            self.selected = self.nodes.iter().position(|n| n.kind == kind);
        }
        if refit {
            self.fit_view(cx);
        }
        cx.notify();
    }

    fn toggle_done(&mut self, topic: usize, subtopic: usize, cx: &mut Context<Self>) {
        let id = self.path_id.clone();
        self.store
            .update(cx, |store, cx| store.toggle_done(&id, topic, subtopic, cx));
    }

    fn selected_subtopic(&self) -> Option<(usize, usize)> {
        match self.selected.map(|ix| self.nodes[ix].kind) {
            Some(NodeKind::Subtopic(ti, si)) => Some((ti, si)),
            _ => None,
        }
    }

    /// Space: an unfinished subtopic is marked done and the next one opens;
    /// a finished one is marked not done again. On a topic or the path,
    /// Space jumps to where to continue.
    fn toggle_selected(&mut self, cx: &mut Context<Self>) {
        match self.selected_subtopic() {
            Some((ti, si)) if !self.path.is_done(ti, si) => self.complete_and_advance(ti, si, cx),
            Some((ti, si)) => self.toggle_done(ti, si, cx),
            None => self.continue_from_selection(cx),
        }
    }

    pub(super) fn complete_and_advance(&mut self, ti: usize, si: usize, cx: &mut Context<Self>) {
        if !self.path.is_done(ti, si) {
            self.toggle_done(ti, si, cx);
        }
        if let Some((nt, ns)) = self.path.step_from((ti, si), 1) {
            self.go_to(NodeKind::Subtopic(nt, ns), cx);
        }
    }

    /// Selects the subtopic `offset` steps away from the selected one.
    pub(super) fn step(&mut self, offset: isize, cx: &mut Context<Self>) {
        if let Some(at) = self.selected_subtopic()
            && let Some((ti, si)) = self.path.step_from(at, offset)
        {
            self.go_to(NodeKind::Subtopic(ti, si), cx);
        }
    }

    /// From a topic: its first unfinished subtopic. From the path: the next
    /// unfinished subtopic anywhere.
    pub(super) fn continue_from_selection(&mut self, cx: &mut Context<Self>) {
        let target = match self.selected.map(|ix| self.nodes[ix].kind) {
            Some(NodeKind::Topic(t)) => self.path.next_in_topic(t),
            _ => self.path.next_subtopic(),
        };
        if let Some((ti, si)) = target {
            self.go_to(NodeKind::Subtopic(ti, si), cx);
        }
    }

    /// Selects a node, keeping it in view without recentring the whole map.
    fn go_to(&mut self, kind: NodeKind, cx: &mut Context<Self>) {
        if let Some(ix) = self.nodes.iter().position(|n| n.kind == kind) {
            self.selected = Some(ix);
            self.ensure_visible(ix);
            if let Some(target) = self.pending_glide.take() {
                self.animate_to(target, cx);
            }
            cx.notify();
        }
    }

    fn reset_layout(&mut self, cx: &mut Context<Self>) {
        let id = self.path_id.clone();
        self.store.update(cx, |store, cx| store.reset_positions(&id, cx));
        self.nodes = layout::layout(&self.path);
        self.fit_view(cx);
    }

    /// Whether a card is within reach of the visible canvas. Off-screen cards
    /// are not built at all, which keeps zooming and panning light on large
    /// paths; the margin covers their shadows and glows.
    pub(super) fn on_screen(&self, ix: usize) -> bool {
        const MARGIN: f32 = 160.;
        let Some((w, h)) = self.view_size() else {
            return true;
        };
        let node = &self.nodes[ix];
        let (left, top) = self.viewport.to_view(node.pos.tuple());
        let (right, bottom) = self.viewport.to_view(node.pos.add(node.size).tuple());
        right > -MARGIN && bottom > -MARGIN && left < w + MARGIN && top < h + MARGIN
    }

    /// The part of the canvas not under the floating inspector.
    pub(super) fn uncovered(&self, (w, h): (f32, f32)) -> (f32, f32) {
        if self.selected.is_some() { ((w - INSPECTOR_COVER).max(w / 2.), h) } else { (w, h) }
    }

    fn view_size(&self) -> Option<(f32, f32)> {
        self.bounds
            .map(|b| (f32::from(b.size.width), f32::from(b.size.height)))
    }

    /// Window position → view position (relative to the plane).
    fn local(&self, position: Point<Pixels>) -> (f32, f32) {
        let origin = self.bounds.map(|b| b.origin).unwrap_or_default();
        (f32::from(position.x - origin.x), f32::from(position.y - origin.y))
    }

    /// Returns true when the viewport changed and another frame is needed.
    fn set_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) -> bool {
        if self.bounds == Some(bounds) {
            return false;
        }
        self.bounds = Some(bounds);
        let mut changed = false;
        if self.needs_fit {
            self.fit_view(cx);
            changed = true;
        }
        if let Some(kind) = self.pending_select.take() {
            self.select_kind(kind, cx);
            changed = true;
        }
        changed
    }

    fn fit_view(&mut self, cx: &mut Context<Self>) {
        let Some(size) = self.view_size() else {
            self.needs_fit = true;
            return;
        };
        let (min, max) = layout::bounds(&self.nodes);
        let frame = Frame::new(min.x, min.y, max.x - min.x, max.y - min.y);
        let target = Viewport::fitting(frame, self.uncovered(size), FIT_MARGIN);
        if self.needs_fit {
            self.viewport = target;
        } else {
            self.animate_to(target, cx);
        }
        self.needs_fit = false;
        cx.notify();
    }

    fn zoom_to(&mut self, zoom: f32, cx: &mut Context<Self>) {
        let (w, h) = self.view_size().unwrap_or((800., 600.));
        let factor = zoom / self.viewport.zoom;
        let target = self.viewport.zoomed(factor, (w / 2., h / 2.));
        self.animate_to(target, cx);
    }

    fn select(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.selected = ix;
        cx.notify();
    }

    /// Selects the node for `kind` and brings it into view beside the
    /// inspector: the whole path when it still reads at that size, otherwise
    /// the card centered at a readable zoom.
    fn select_kind(&mut self, kind: NodeKind, cx: &mut Context<Self>) {
        let Some(ix) = self.nodes.iter().position(|n| n.kind == kind) else {
            return;
        };
        if let Some((w, h)) = self.view_size() {
            // Selecting opens the inspector, which covers the right edge.
            let w = (w - INSPECTOR_COVER).max(w / 2.);
            let (min, max) = layout::bounds(&self.nodes);
            let whole = Viewport::fitting(Frame::new(min.x, min.y, max.x - min.x, max.y - min.y), (w, h), FIT_MARGIN);
            let target = if whole.zoom >= READABLE_ZOOM {
                whole
            } else {
                let center = self.nodes[ix].center();
                let zoom = self.viewport.zoom.max(READABLE_ZOOM);
                // Centered on the card, but never showing more empty canvas
                // past the path's edges than the fit margin.
                let axis = |center: f32, view: f32, min: f32, max: f32| {
                    let (span, margin) = (view / zoom, FIT_MARGIN / zoom);
                    if max - min + 2. * margin <= span {
                        min - (span - (max - min)) / 2.
                    } else {
                        (center - span / 2.).clamp(min - margin, max + margin - span)
                    }
                };
                Viewport::new(axis(center.x, w, min.x, max.x), axis(center.y, h, min.y, max.y), zoom)
            };
            self.animate_to(target, cx);
        }
        self.select(Some(ix), cx);
    }

    /// Closes the inspector and glides back to the whole path.
    pub(super) fn close_inspector(&mut self, cx: &mut Context<Self>) {
        if self.selected.take().is_some() {
            self.fit_view(cx);
        }
        cx.notify();
    }

    /// Selects a topic (e.g. from the command palette).
    pub fn select_topic(&mut self, topic: usize, cx: &mut Context<Self>) {
        self.pending_select = Some(NodeKind::Topic(topic));
        if self.bounds.is_some() {
            self.pending_select = None;
            self.select_kind(NodeKind::Topic(topic), cx);
        }
        cx.notify();
    }

    pub fn fit(&mut self, cx: &mut Context<Self>) {
        self.fit_view(cx);
    }

    pub fn share(&mut self, cx: &mut Context<Self>) {
        self.share_file(cx);
    }

    pub fn export(&mut self, cx: &mut Context<Self>) {
        self.export_markdown(cx);
    }

    /// Opens the inspector on one subtopic, e.g. when resuming a path.
    pub fn select_subtopic(&mut self, topic: usize, subtopic: usize, cx: &mut Context<Self>) {
        self.pending_select = Some(NodeKind::Subtopic(topic, subtopic));
        if self.bounds.is_some() {
            self.pending_select = None;
            self.select_kind(NodeKind::Subtopic(topic, subtopic), cx);
        }
        cx.notify();
    }

    fn start_node_drag(
        &mut self,
        ix: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Keeps the press from reaching the plane, which would start a pan.
        cx.stop_propagation();
        self.stop_camera();
        self.focus_handle.focus(window, cx);
        self.press = None;
        let (x, y) = self.viewport.to_canvas(self.local(event.position));
        self.drag = Some(NodeDrag {
            ix,
            grab: Vec2::new(x, y).sub(self.nodes[ix].pos),
        });
        self.select(Some(ix), cx);
    }

    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(drag) = &self.drag {
            let (x, y) = self.viewport.to_canvas(self.local(position));
            self.nodes[drag.ix].pos = Vec2::new(x, y).sub(drag.grab);
            cx.notify();
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            let node = &self.nodes[drag.ix];
            let (key, at) = (layout::node_key(node.kind), [node.pos.x, node.pos.y]);
            let moved = self.path.positions.get(&key) != Some(&at)
                && layout::layout(&self.path)[drag.ix].pos != node.pos;
            if moved {
                let id = self.path_id.clone();
                self.store
                    .update(cx, |store, cx| store.set_position(&id, key, at, cx));
            }
        }
        cx.notify();
    }

    fn on_plane_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.press = Some(event.position);
    }

    /// A press and release on empty canvas (not a pan) clears the selection.
    fn on_plane_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(press) = self.press.take() {
            let moved = event.position - press;
            if f32::from(moved.x).abs() < CLICK_SLOP && f32::from(moved.y).abs() < CLICK_SLOP {
                self.close_inspector(cx);
            }
        }
    }

    /// Where a card's link comes from: a subtopic follows the one before it,
    /// the first follows its topic, a topic follows the path.
    fn edge_source(&self, ix: usize) -> Option<usize> {
        match self.nodes[ix].kind {
            NodeKind::Subtopic(ti, si) if si > 0 => {
                self.nodes.iter().position(|n| n.kind == NodeKind::Subtopic(ti, si - 1))
            }
            _ => self.nodes[ix].parent,
        }
    }

    /// Whether the link into `kind` lies on the way from the path to the
    /// selected card, so the selection's whole branch lights up.
    fn on_selected_branch(&self, kind: NodeKind) -> bool {
        let Some(selected) = self.selected.map(|ix| self.nodes[ix].kind) else {
            return false;
        };
        match (selected, kind) {
            (NodeKind::Root, NodeKind::Topic(_)) => true,
            (NodeKind::Topic(t), NodeKind::Topic(k)) => t == k,
            (NodeKind::Topic(t), NodeKind::Subtopic(k, 0)) => t == k,
            (NodeKind::Subtopic(t, _), NodeKind::Topic(k)) => t == k,
            (NodeKind::Subtopic(t, s), NodeKind::Subtopic(k, j)) => t == k && j <= s,
            _ => false,
        }
    }

    fn edge_layer(&self, cx: &App) -> EdgeLayer {
        let c = &cx.theme().colors;
        let view = self.viewport;
        let edges = (0..self.nodes.len())
            .filter_map(|ix| {
                let node = &self.nodes[ix];
                let source = self.edge_source(ix)?;
                let curve = layout::edge_curve(&self.nodes[source], node).map(|p| {
                    let (x, y) = view.to_view(p.tuple());
                    Vec2::new(x, y)
                });
                let done = match node.kind {
                    NodeKind::Subtopic(ti, si) => self.path.is_done(ti, si),
                    NodeKind::Topic(ti) => {
                        let (done, total) = self.path.topic_progress(ti);
                        total > 0 && done == total
                    }
                    NodeKind::Root => false,
                };
                Some(Edge { curve, highlighted: self.on_selected_branch(node.kind), done })
            })
            .collect();
        EdgeLayer {
            edges,
            color: c.fg.opacity(0.14),
            highlight: c.accent,
            done_color: c.success.opacity(0.6),
            width: (1.6 * view.zoom).max(1.),
        }
    }
}

impl Render for MindMapView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let edge_layer = self.edge_layer(cx);
        let dragging = self.drag.is_some();
        let view = cx.entity().downgrade();

        // Measures the plane (for fit), paints edges, and while a card is
        // dragged follows the pointer window-wide.
        let painted = canvas(
            {
                let view = view.clone();
                move |bounds, window, cx| {
                    let refit = view
                        .update(cx, |this, cx| this.set_bounds(bounds, cx))
                        .unwrap_or(false);
                    if refit {
                        window.request_animation_frame();
                    }
                }
            },
            {
                let view = view.clone();
                move |bounds, (), window, _cx| {
                    edge_layer.paint(bounds, window);
                    if dragging {
                        let mover = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                mover
                                    .update(cx, |this, cx| this.drag_to(event.position, cx))
                                    .ok();
                            }
                        });
                        let ender = view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                ender.update(cx, |this, cx| this.end_drag(cx)).ok();
                            }
                        });
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let halos = self.render_halos(cx);
        let mut cards = Vec::with_capacity(self.nodes.len());
        for ix in (0..self.nodes.len()).filter(|ix| self.on_screen(*ix)) {
            cards.push(self.render_node(ix, cx));
        }
        let inspector = self.selected.map(|ix| self.render_inspector(ix, cx));

        let plane = InfiniteCanvas::new("map-plane", self.viewport)
            .on_viewport({
                let view = view.clone();
                move |next, _, cx| {
                    view.update(cx, |this, cx| {
                        this.stop_camera();
                        this.viewport = next;
                        cx.notify();
                    })
                    .ok();
                }
            })
            .layer(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    // Under the edges and cards, dimmer than elsewhere.
                    .child(glass::glow(0.45, cx))
                    .child(painted)
                    .children(halos)
                    .children(cards),
            );

        let header = self.render_header(cx);
        let outline = self.render_outline(cx);
        let controls = self.render_controls(cx);

        div()
            .track_focus(&self.focus_handle)
            .key_context("MindMap")
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| {
                let zoom = (this.viewport.zoom * ZOOM_STEP).min(Viewport::ZOOMS.1);
                this.zoom_to(zoom, cx)
            }))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| {
                let zoom = (this.viewport.zoom / ZOOM_STEP).max(Viewport::ZOOMS.0);
                this.zoom_to(zoom, cx)
            }))
            .on_action(cx.listener(|this, _: &FitView, _, cx| this.fit_view(cx)))
            .on_action(cx.listener(|this, _: &Deselect, _, cx| this.close_inspector(cx)))
            .on_action(cx.listener(|this, _: &ToggleDone, _, cx| this.toggle_selected(cx)))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.move_selection(Direction::Up, cx)))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.move_selection(Direction::Down, cx)))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| this.move_selection(Direction::Left, cx)))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| this.move_selection(Direction::Right, cx)))
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .min_h_0()
                    .overflow_hidden()
                    .map(|row| match self.view_mode {
                        // On the map the inspector floats over the canvas, so
                        // opening it never squeezes or shifts the cards.
                        ViewMode::Map => row.child(
                            div()
                                .id("map-stage")
                                .relative()
                                .flex_1()
                                .h_full()
                                .overflow_hidden()
                                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_plane_down))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_plane_up))
                                .child(plane)
                                .child(controls)
                                .children(inspector.map(|panel| {
                                    div()
                                        .absolute()
                                        .top_0()
                                        .right_0()
                                        .bottom_0()
                                        .p(gpui::px(INSPECTOR_INSET))
                                        // Presses on the panel must not reach the plane.
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .child(panel)
                                })),
                        ),
                        ViewMode::Outline => row
                            .child(div().flex_1().h_full().min_w_0().child(outline))
                            .children(inspector.map(|panel| div().h_full().p(gpui::px(INSPECTOR_INSET)).child(panel))),
                    }),
            )
    }
}
