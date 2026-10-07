//! Paths: every learning path as a glass row, filterable by where it stands.
//! It does what the old sidebar did: open, and delete with undo.

use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ActiveTheme;
use ely_gpui_component::typography::Caption;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, MouseButton, Render, Subscription,
    Window, div, prelude::*, px, AnyElement, SharedString,
};

use crate::model::LearningPath;
use crate::store::PathStore;
use crate::ui::glass::{self, PillStyle};
use crate::ui::rows;

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
    _store_sub: Subscription,
}

impl EventEmitter<PathsEvent> for PathsView {}

impl PathsView {
    pub fn new(store: Entity<PathStore>, cx: &mut Context<Self>) -> Self {
        let _store_sub = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, filter: Filter::All, _store_sub }
    }

    /// A flat row; its delete button shows while the row is hovered.
    fn render_row(&self, ix: usize, path: &LearningPath, cx: &mut Context<Self>) -> AnyElement {
        let group = SharedString::from(format!("paths-row-{ix}"));
        let (open_id, delete_id) = (path.id.clone(), path.id.clone());
        let delete = div()
            .invisible()
            .group_hover(group.clone(), |s| s.visible())
            // The row must not also take this press.
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .child(
                glass::icon_button(("paths-delete", ix), IconName::Trash2, "Delete path", cx)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PathsEvent::Delete(delete_id.clone())))),
            );
        let row = rows::path_row(("paths-row", ix), group, path, cx)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PathsEvent::Open(open_id.clone()))))
            .child(delete);
        rows::enter(row, format!("paths-row-in-{}-{:?}", path.id, self.filter), ix)
    }
}

impl Render for PathsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths: Vec<LearningPath> = self.store.read(cx).paths().to_vec();
        let shown: Vec<LearningPath> = visible(&paths, self.filter).into_iter().cloned().collect();
        let rows: Vec<_> =
            shown.iter().enumerate().map(|(ix, p)| self.render_row(ix, p, cx)).collect();
        let filters = Filter::ALL.map(|filter| {
            let style = if filter == self.filter { PillStyle::Selected } else { PillStyle::Quiet };
            glass::pill(("paths-filter", filter as usize), filter.label(), None, style, cx).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.filter = filter;
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
        let count = paths.len();
        // The same reading column as Today, so names and status stay close.
        let column = div()
            .w_full()
            .max_w(px(900.))
            .h_full()
            .flex()
            .flex_col()
            .gap_4()
            .px_6()
            .pt_2()
            .pb_5()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_3()
                    .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Paths"))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(c.fg_muted)
                            .child(format!("{count} learning path{}", if count == 1 { "" } else { "s" })),
                    )
                    .child(div().w_2())
                    .child(div().flex().gap_1p5().children(filters))
                    .child(div().flex_1())
                    .child(
                        glass::pill("paths-new", "New path", Some(IconName::Plus), PillStyle::Primary, cx)
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PathsEvent::NewPath))),
                    ),
            )
            .children(empty)
            .child(
                ScrollArea::new("paths-list")
                    .flex_1()
                    .min_h_0()
                    .child(div().flex().flex_col().pb_1().children(rows)),
            );
        div().size_full().flex().justify_center().child(column)
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
