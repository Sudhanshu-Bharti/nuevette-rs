//! Paths: every learning path as a glass row, filterable by where it stands.
//! It does what the old sidebar did: open, and delete with undo.

use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::motion::ProgressRing;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ActiveTheme;
use ely_gpui_component::typography::Caption;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, MouseButton, Render, Subscription,
    Window, div, prelude::*, px, rems,
};

use crate::model::LearningPath;
use crate::stats;
use crate::store::PathStore;
use crate::ui::glass::{self, Hue, PillStyle};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    NotStarted,
    InProgress,
    Done,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::NotStarted => "Not started",
            Status::InProgress => "In progress",
            Status::Done => "Done",
        }
    }

    pub fn hue(self) -> Hue {
        match self {
            Status::NotStarted => Hue::Neutral,
            Status::InProgress => Hue::Accent,
            Status::Done => Hue::Success,
        }
    }
}

/// A path with nothing to do yet has not started; one still being written
/// is never done, however much of what exists so far is finished.
pub fn status(path: &LearningPath) -> Status {
    match path.progress() {
        (_, 0) | (0, _) => Status::NotStarted,
        (done, total) if done == total && path.building.is_none() => Status::Done,
        _ => Status::InProgress,
    }
}

pub fn percent(done: usize, total: usize) -> usize {
    (done * 100).checked_div(total).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    InProgress,
    NotStarted,
    Done,
}

impl Filter {
    pub const ALL: [Filter; 4] = [Filter::All, Filter::InProgress, Filter::NotStarted, Filter::Done];

    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::InProgress => "In progress",
            Filter::NotStarted => "Not started",
            Filter::Done => "Done",
        }
    }

    fn keeps(self, status: Status) -> bool {
        match self {
            Filter::All => true,
            Filter::InProgress => status == Status::InProgress,
            Filter::NotStarted => status == Status::NotStarted,
            Filter::Done => status == Status::Done,
        }
    }
}

/// The paths a filter keeps, most recently opened first; ties keep store order.
pub fn visible(paths: &[LearningPath], filter: Filter) -> Vec<&LearningPath> {
    let mut kept: Vec<&LearningPath> = paths.iter().filter(|p| filter.keeps(status(p))).collect();
    kept.sort_by_key(|p| std::cmp::Reverse(p.last_opened.unwrap_or(0)));
    kept
}

pub enum PathsEvent {
    Open(String),
    Delete(String),
    NewPath,
}

pub struct PathsView {
    store: Entity<PathStore>,
    filter: Filter,
    /// The row under the pointer, which shows its delete button.
    hovered: Option<usize>,
    _store_sub: Subscription,
}

impl EventEmitter<PathsEvent> for PathsView {}

impl PathsView {
    pub fn new(store: Entity<PathStore>, cx: &mut Context<Self>) -> Self {
        let _store_sub = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, filter: Filter::All, hovered: None, _store_sub }
    }

    fn render_row(&self, ix: usize, path: &LearningPath, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let c = cx.theme().colors.clone();
        let mono = cx.theme().mono_family.clone();
        let (done, total) = path.progress();
        let state = status(path);
        let leading = if state == Status::InProgress {
            div()
                .size(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .child(ProgressRing::new(("paths-ring", ix), done as f32 / total.max(1) as f32).size(rems(1.875)))
                .into_any_element()
        } else {
            glass::avatar(path, cx).into_any_element()
        };
        let opened = path.last_opened.map(|at| stats::ago(at, stats::now_secs()));
        let (open_id, delete_id) = (path.id.clone(), path.id.clone());
        let delete = (self.hovered == Some(ix)).then(|| {
            div()
                // The row must not also take this press.
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    glass::icon_button(("paths-delete", ix), IconName::Trash2, "Delete path", cx)
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.emit(PathsEvent::Delete(delete_id.clone()))
                        })),
                )
        });
        let (hover_border, hover_bg) = (c.border_strong, c.fg.opacity(0.04));
        div()
            .id(("paths-row", ix))
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .rounded(px(14.))
            .bg(c.fg.opacity(0.025))
            .border_1()
            .border_color(c.border)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border).bg(hover_bg))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                let next = if *hovering { Some(ix) } else { this.hovered.filter(|h| *h != ix) };
                if this.hovered != next {
                    this.hovered = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PathsEvent::Open(open_id.clone()))))
            .child(leading)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_ellipsis().font_weight(FontWeight::MEDIUM).child(path.name.clone()))
                    .child(Caption::new(format!("{} topics \u{00b7} {}", path.topics.len(), path.estimated_time))),
            )
            .children(opened.map(Caption::new))
            .child(
                div()
                    .w(px(44.))
                    .flex()
                    .justify_end()
                    .font_family(mono)
                    .text_color(c.fg)
                    .child(format!("{}%", percent(done, total))),
            )
            .child(glass::chip(state.label(), state.hue(), cx))
            .children(delete)
    }
}

impl Render for PathsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths: Vec<LearningPath> = self.store.read(cx).paths().to_vec();
        let shown: Vec<LearningPath> = visible(&paths, self.filter).into_iter().cloned().collect();
        let rows: Vec<_> =
            shown.iter().enumerate().map(|(ix, p)| self.render_row(ix, p, cx).into_any_element()).collect();
        let filters = Filter::ALL.map(|filter| {
            let style = if filter == self.filter { PillStyle::Selected } else { PillStyle::Ghost };
            glass::pill(("paths-filter", filter as usize), filter.label(), None, style, cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.filter = filter;
                    this.hovered = None;
                    cx.notify();
                },
            ))
        });
        let c = cx.theme().colors.clone();
        let empty = rows.is_empty().then(|| {
            glass::GlassCard::new().child(Caption::new(if paths.is_empty() {
                "No paths yet. Name a topic and Nuevette maps it from the official docs."
            } else {
                "Nothing here for this filter."
            }))
        });
        ScrollArea::new("paths").size_full().child(
            div().flex().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(1040.))
                    .px_10()
                    .pt_8()
                    .pb_16()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_size(px(32.)).font_weight(FontWeight::MEDIUM).child("Paths"))
                                    .child(div().text_color(c.fg_muted).child(format!(
                                        "{} learning path{}",
                                        paths.len(),
                                        if paths.len() == 1 { "" } else { "s" }
                                    ))),
                            )
                            .child(
                                glass::pill("paths-new", "New path", Some(IconName::Plus), PillStyle::Primary, cx)
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PathsEvent::NewPath))),
                            ),
                    )
                    .child(div().flex().gap_2().children(filters))
                    .children(empty)
                    .child(div().flex().flex_col().gap_2().children(rows)),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_paths, subtopic_key};

    fn finish_all(path: &mut LearningPath) {
        for (ti, topic) in path.topics.iter().enumerate() {
            for si in 0..topic.subtopics.len() {
                path.completed.insert(subtopic_key(ti, si));
            }
        }
    }

    #[test]
    fn status_follows_progress() {
        let mut paths = sample_paths();
        assert_eq!(status(&paths[0]), Status::NotStarted);
        paths[0].completed.insert(subtopic_key(0, 0));
        assert_eq!(status(&paths[0]), Status::InProgress);
        finish_all(&mut paths[0]);
        assert_eq!(status(&paths[0]), Status::Done);
    }

    #[test]
    fn status_of_a_path_being_built_is_not_started() {
        let mut path = sample_paths().remove(0);
        for topic in &mut path.topics {
            topic.subtopics.clear();
        }
        assert_eq!(status(&path), Status::NotStarted);
    }

    #[test]
    fn a_path_being_built_is_never_done() {
        let mut path = sample_paths().remove(0);
        finish_all(&mut path);
        path.building = Some("Writing topic 2 of 4".into());
        assert_eq!(status(&path), Status::InProgress);
    }

    #[test]
    fn percent_of_empty_is_zero() {
        assert_eq!(percent(0, 0), 0);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(3, 3), 100);
    }

    #[test]
    fn filters_keep_their_status_and_order_by_last_opened() {
        let mut paths = sample_paths();
        paths[0].completed.insert(subtopic_key(0, 0)); // in progress
        finish_all(&mut paths[1]); // done
        paths[0].last_opened = Some(10);
        paths[2].last_opened = Some(20);
        let names = |filter| visible(&paths, filter).iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(Filter::All)[0], paths[2].name, "most recently opened first");
        assert_eq!(names(Filter::InProgress), vec![paths[0].name.clone()]);
        assert_eq!(names(Filter::Done), vec![paths[1].name.clone()]);
        assert_eq!(names(Filter::NotStarted), vec![paths[2].name.clone()]);
    }

    #[test]
    fn visible_on_empty_store_is_empty() {
        assert!(visible(&[], Filter::All).is_empty());
    }
}
