//! Today: where a returning learner lands. One focused column: a greeting
//! with the week in quiet numbers, the single next step as the only card,
//! then every path as a flat list, and recent activity only when there is
//! some. It fits the window; only the list scrolls.

use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, Render, SharedString,
    Subscription, Window, div, prelude::*, px,
};
use jiff::tz::TimeZone;

use crate::model::LearningPath;
use crate::stats::{self, Summary};
use crate::store::PathStore;
use crate::ui::glass::{self, GlassCard, Hue, PillStyle};
use crate::ui::paths::{Status, status};
use crate::ui::rows;

/// The column's width: wide enough for a row, narrow enough to read.
const COLUMN: f32 = 780.;
/// Recent completions shown under the list.
const RECENT: usize = 3;

pub enum TodayEvent {
    Open(String),
    /// Open a path with one subtopic selected.
    Resume(String, (usize, usize)),
    NewPath,
    /// Show every path.
    ViewAll,
}

/// "+3 vs last week", "-1 vs last week" or "Same as last week".
pub fn week_trend(this: usize, last: usize) -> (String, Hue) {
    match this as i64 - last as i64 {
        0 => ("Same as last week".into(), Hue::Neutral),
        d if d > 0 => (format!("+{d} vs last week"), Hue::Success),
        d => (format!("{d} vs last week"), Hue::Neutral),
    }
}

pub struct TodayView {
    store: Entity<PathStore>,
    _store_sub: Subscription,
}

impl EventEmitter<TodayEvent> for TodayView {}

impl TodayView {
    pub fn new(store: Entity<PathStore>, cx: &mut Context<Self>) -> Self {
        let _store_sub = cx.observe(&store, |_, _, cx| cx.notify());
        Self { store, _store_sub }
    }

    fn greeting() -> (&'static str, String) {
        let now = jiff::Zoned::now();
        let greeting = match now.hour() {
            5..12 => "Good morning",
            12..17 => "Good afternoon",
            _ => "Good evening",
        };
        (greeting, now.strftime("%A, %-d %B").to_string())
    }

    /// The week as one quiet line of numbers: "3 done · 4.5 h · 2-day streak".
    fn week_line(summary: &Summary, cx: &App) -> AnyElement {
        let c = &cx.theme().colors;
        let number = |text: String| div().text_color(c.fg).font_weight(FontWeight::MEDIUM).child(text);
        let quiet = |text: &'static str| div().text_color(c.fg_muted).child(text);
        let dot = || div().text_color(c.fg_subtle).child("\u{00b7}");
        let (trend, hue) = week_trend(summary.done_this_week, summary.done_last_week);
        div()
            .flex()
            .flex_none()
            .items_baseline()
            .gap_1p5()
            .text_size(px(13.))
            .child(number(summary.done_this_week.to_string()))
            .child(quiet("done this week"))
            .when(hue == Hue::Success, |line| {
                line.child(div().text_color(c.success).child(format!("({})", trend.replace(" vs last week", ""))))
            })
            .child(dot())
            .child(number(format!("{:.1} h", summary.hours_this_week)))
            .child(quiet("studied"))
            .child(dot())
            .child(number(format!("{}-day", summary.streak_days)))
            .child(quiet("streak"))
            .into_any_element()
    }

    /// The single most useful thing to do now, as the page's only card.
    fn render_up_next(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let store = self.store.read(cx);
        let mut candidates: Vec<&LearningPath> =
            store.paths().iter().filter(|p| p.next_subtopic().is_some()).collect();
        candidates.sort_by_key(|p| std::cmp::Reverse((p.progress().0 > 0, p.last_opened.unwrap_or(0))));
        let path = (*candidates.first()?).clone();
        let (ti, si) = path.next_subtopic()?;
        let sub = path.topics[ti].subtopics[si].clone();
        let c = cx.theme().colors.clone();
        let (id, resume_id, store_handle) = (path.id.clone(), path.id.clone(), self.store.clone());
        let label = if path.progress().0 == 0 { "Start here" } else { "Up next" };
        Some(
            GlassCard::new()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(glass::eyebrow(format!("{label} \u{00b7} {}.{}", ti + 1, si + 1), cx))
                                .child(
                                    div()
                                        .text_size(px(19.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_ellipsis()
                                        .child(sub.name.clone()),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .text_size(px(13.))
                                        .text_color(c.fg_muted)
                                        .child(format!("{} \u{00b7} {}", path.name, path.topics[ti].name))
                                        .child(div().text_color(c.fg_subtle).child("\u{00b7}"))
                                        .child(Icon::new(IconName::Clock).size(IconSize::Xs).color(c.fg_subtle))
                                        .child(sub.estimated_time.clone()),
                                ),
                        )
                        .child(
                            glass::pill("up-next-done", "Mark done", None, PillStyle::Quiet, cx).on_click(
                                move |_, _, cx| {
                                    store_handle.update(cx, |store, cx| store.toggle_done(&id, ti, si, cx));
                                },
                            ),
                        )
                        .child(
                            glass::pill("up-next-resume", "Resume", Some(IconName::ArrowRight), PillStyle::Primary, cx)
                                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                    cx.emit(TodayEvent::Resume(resume_id.clone(), (ti, si)))
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// A section's title line: name, quiet count, optional action.
    fn section(title: &'static str, count: String, action: Option<AnyElement>, cx: &App) -> AnyElement {
        div()
            .flex()
            .flex_none()
            .items_baseline()
            .gap_2()
            .px_3()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().text_size(px(12.)).text_color(cx.theme().colors.fg_subtle).child(count))
            .child(div().flex_1())
            .children(action)
            .into_any_element()
    }

    /// Every path as a flat list: in progress first, then not started, then
    /// done, each most recently opened first.
    fn render_paths(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut paths: Vec<LearningPath> = self.store.read(cx).paths().to_vec();
        let rank = |p: &LearningPath| match status(p) {
            Status::InProgress => 0,
            Status::NotStarted => 1,
            Status::Done => 2,
        };
        paths.sort_by_key(|p| (rank(p), std::cmp::Reverse(p.last_opened.unwrap_or(0))));
        let rows: Vec<AnyElement> = paths
            .iter()
            .enumerate()
            .map(|(ix, path)| {
                let id = path.id.clone();
                let row = rows::path_row(("today-row", ix), SharedString::from(format!("today-row-{ix}")), path, cx)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::Open(id.clone()))));
                rows::enter(row, format!("today-row-in-{}", path.id), ix + 1)
            })
            .collect();
        let view_all = glass::pill("view-all", "View all", None, PillStyle::Quiet, cx)
            .h(px(26.))
            .text_size(px(12.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::ViewAll)))
            .into_any_element();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(Self::section("Your paths", paths.len().to_string(), Some(view_all), cx))
            .child(ScrollArea::new("today-paths").flex_1().min_h_0().child(div().flex().flex_col().children(rows)))
            .into_any_element()
    }

    /// The last few completions, only when there are any.
    fn render_recent(summary: &Summary, cx: &mut Context<Self>) -> Option<AnyElement> {
        if summary.recent.is_empty() {
            return None;
        }
        let c = cx.theme().colors.clone();
        let now = stats::now_secs();
        let hover = glass::ink(cx, 0.05);
        let rows: Vec<AnyElement> = summary
            .recent
            .iter()
            .take(RECENT)
            .enumerate()
            .map(|(ix, done)| {
                let (id, at) = (done.path_id.clone(), (done.topic, done.subtopic));
                div()
                    .id(("activity", ix))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(36.))
                    .px_3()
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .active(|s| s.opacity(0.8))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::Resume(id.clone(), at))))
                    .child(Icon::new(IconName::CircleCheck).size(IconSize::Sm).color(c.success))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(360.))
                            .text_ellipsis()
                            .text_size(px(13.))
                            .child(format!("{}.{} {}", done.topic + 1, done.subtopic + 1, done.name)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_size(px(12.))
                            .text_color(c.fg_muted)
                            .child(done.path_name.clone()),
                    )
                    .child(div().flex_none().text_size(px(12.)).text_color(c.fg_subtle).child(stats::ago(done.at, now)))
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_none()
                .flex_col()
                .gap_1()
                .child(Self::section("Recently finished", String::new(), None, cx))
                .children(rows)
                .into_any_element(),
        )
    }
}

impl Render for TodayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths: Vec<LearningPath> = self.store.read(cx).paths().to_vec();
        let (greeting, date) = Self::greeting();
        let name = cx.global::<crate::settings::Settings>().name.trim().to_string();
        let greeting = if name.is_empty() { greeting.to_string() } else { format!("{greeting}, {name}") };
        let c = cx.theme().colors.clone();
        let mut column = div()
            .w_full()
            .max_w(px(COLUMN))
            .h_full()
            .flex()
            .flex_col()
            .gap_5()
            .px_6()
            .pt_2()
            .pb_5();

        if paths.is_empty() {
            column = column
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap_3()
                        .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(greeting))
                        .child(div().text_size(px(13.)).text_color(c.fg_muted).child(date)),
                )
                .child(rows::enter(
                    GlassCard::new()
                        .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("Start your first learning path"))
                        .child(
                            div()
                                .text_color(c.fg_muted)
                                .child("Name a topic and Nuevette maps it from the official docs, step by step."),
                        )
                        .child(div().flex().child(
                            glass::pill("first-path", "New path", Some(IconName::Plus), PillStyle::Primary, cx)
                                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::NewPath))),
                        )),
                    "today-empty-in",
                    0,
                ));
        } else {
            let summary = stats::summarize(&paths, stats::now_secs(), &TimeZone::system());
            let heading = div()
                .flex()
                .flex_none()
                .items_baseline()
                .gap_3()
                .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child(greeting))
                .child(div().text_size(px(13.)).text_color(c.fg_subtle).child(date))
                .child(div().flex_1())
                .child(Self::week_line(&summary, cx));
            let up_next = self.render_up_next(cx).map(|card| rows::enter(card, "today-up-next-in", 0));
            let list = self.render_paths(cx);
            let recent = Self::render_recent(&summary, cx);
            column = column.child(heading).children(up_next).child(list).children(recent);
        }
        div().size_full().flex().justify_center().child(column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_trend_reads_naturally() {
        assert_eq!(week_trend(5, 2), ("+3 vs last week".to_string(), Hue::Success));
        assert_eq!(week_trend(1, 4), ("-3 vs last week".to_string(), Hue::Neutral));
        assert_eq!(week_trend(0, 0), ("Same as last week".to_string(), Hue::Neutral));
        assert_eq!(week_trend(2, 2), ("Same as last week".to_string(), Hue::Neutral));
    }
}
