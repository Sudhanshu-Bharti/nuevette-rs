//! Today: where a returning learner lands. How the week is going, what to
//! study next, and the paths in progress.

use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use ely_gpui_component::typography::Caption;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, Render, Subscription,
    Window, div, prelude::*, px,
};
use jiff::tz::TimeZone;

use crate::model::LearningPath;
use crate::stats::{self, Summary};
use crate::store::PathStore;
use crate::ui::glass::{self, GlassCard, Hue, PillStyle};
use crate::ui::paths::{Status, percent, status};

/// Below this window width, stat cards sit two to a row instead of three.
const WIDE_WINDOW: f32 = 1200.;
const IN_PROGRESS_ROWS: usize = 5;

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

    fn stat_card(label: &'static str, icon: IconName, value: String, chip: (String, Hue), cx: &App) -> AnyElement {
        GlassCard::new()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(div().pt_2().child(glass::eyebrow(label, cx)))
                    .child(glass::badge(icon, cx)),
            )
            .child(glass::stat_value(&value, cx))
            .child(div().flex().child(glass::chip(chip.0, chip.1, cx)))
            .into_any_element()
    }

    fn render_stats(summary: &Summary, columns: u16, cx: &App) -> AnyElement {
        let streak_chip = if summary.streak_days > 0 {
            ("Keep it going".to_string(), Hue::Accent)
        } else {
            ("Finish one today".to_string(), Hue::Neutral)
        };
        div()
            .grid()
            .grid_cols(columns)
            .gap_4()
            .child(Self::stat_card(
                "Finished this week",
                IconName::CircleCheck,
                summary.done_this_week.to_string(),
                week_trend(summary.done_this_week, summary.done_last_week),
                cx,
            ))
            .child(Self::stat_card(
                "Study time this week",
                IconName::Clock,
                format!("{:.1} h", summary.hours_this_week),
                ("Last 7 days".to_string(), Hue::Neutral),
                cx,
            ))
            .child(Self::stat_card(
                "Day streak",
                IconName::Zap,
                format!("{} {}", summary.streak_days, if summary.streak_days == 1 { "day" } else { "days" }),
                streak_chip,
                cx,
            ))
            .into_any_element()
    }

    /// The next step of the path opened most recently that still has work left.
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
        let label = if path.progress().0 == 0 { "Start" } else { "Up next" };
        Some(
            GlassCard::new()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(glass::avatar(&path, cx))
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
                                        .text_size(px(18.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_ellipsis()
                                        .child(sub.name.clone()),
                                )
                                .child(Caption::new(format!("{} \u{00b7} {}", path.name, path.topics[ti].name))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap_1()
                                .child(Icon::new(IconName::Clock).size(IconSize::Xs).color(c.fg_subtle))
                                .child(Caption::new(sub.estimated_time.clone())),
                        )
                        .child(
                            glass::pill("up-next-done", "Mark done", None, PillStyle::Ghost, cx).on_click(
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
                .child(div().text_color(c.fg_muted).child(sub.description.clone()))
                .into_any_element(),
        )
    }

    fn render_row(ix: usize, path: &LearningPath, cx: &mut Context<Self>) -> AnyElement {
        let c = cx.theme().colors.clone();
        let mono = cx.theme().mono_family.clone();
        let (done, total) = path.progress();
        let next = path
            .next_subtopic()
            .map(|(ti, si)| format!("Next: {}.{} {}", ti + 1, si + 1, path.topics[ti].subtopics[si].name));
        let id = path.id.clone();
        let (hover_border, hover_bg) = (c.border_strong, c.fg.opacity(0.04));
        div()
            .id(("today-row", ix))
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .rounded(px(14.))
            .bg(glass::card_fill(cx, true))
            .border_1()
            .border_color(c.border)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border).bg(hover_bg))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::Open(id.clone()))))
            .child(glass::avatar(path, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_ellipsis().font_weight(FontWeight::MEDIUM).child(path.name.clone()))
                    .children(next.map(Caption::new)),
            )
            .child(div().font_family(mono).text_color(c.fg).child(format!("{}%", percent(done, total))))
            .child(glass::chip(Status::InProgress.label(), Status::InProgress.hue(), cx))
            .into_any_element()
    }

    fn render_in_progress(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut started: Vec<LearningPath> = self
            .store
            .read(cx)
            .paths()
            .iter()
            .filter(|p| status(p) == Status::InProgress)
            .cloned()
            .collect();
        if started.is_empty() {
            return None;
        }
        started.sort_by_key(|p| std::cmp::Reverse(p.last_opened.unwrap_or(0)));
        let count = started.len();
        let rows: Vec<AnyElement> = started
            .iter()
            .take(IN_PROGRESS_ROWS)
            .enumerate()
            .map(|(ix, p)| Self::render_row(ix, p, cx))
            .collect();
        let c = cx.theme().colors.clone();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_end()
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("In progress"))
                                .child(
                                    div()
                                        .text_color(c.fg_muted)
                                        .child(format!("{count} path{}", if count == 1 { "" } else { "s" })),
                                ),
                        )
                        .child(
                            glass::pill("view-all", "View all", None, PillStyle::Ghost, cx)
                                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::ViewAll))),
                        ),
                )
                .child(div().flex().flex_col().gap_2().children(rows))
                .into_any_element(),
        )
    }

    fn render_activity(summary: &Summary, cx: &mut Context<Self>) -> Option<AnyElement> {
        if summary.recent.is_empty() {
            return None;
        }
        let c = cx.theme().colors.clone();
        let now = stats::now_secs();
        let hover = c.hover;
        let rows: Vec<_> = summary
            .recent
            .iter()
            .enumerate()
            .map(|(ix, done)| {
                let (id, at) = (done.path_id.clone(), (done.topic, done.subtopic));
                div()
                    .id(("activity", ix))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::Resume(id.clone(), at))))
                    .child(Icon::new(IconName::CircleCheck).size(IconSize::Sm).color(c.success))
                    .child(
                        div()
                            .min_w_0()
                            .text_ellipsis()
                            .child(format!("{}.{} {}", done.topic + 1, done.subtopic + 1, done.name)),
                    )
                    .child(Caption::new(done.path_name.clone()))
                    .child(div().flex_1())
                    .child(Caption::new(stats::ago(done.at, now)))
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child("Recent activity"))
                .child(GlassCard::new().dense().children(rows))
                .into_any_element(),
        )
    }
}

impl Render for TodayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths: Vec<LearningPath> = self.store.read(cx).paths().to_vec();
        let (greeting, date) = Self::greeting();
        let name = cx.global::<crate::settings::Settings>().name.trim().to_string();
        let greeting = if name.is_empty() { greeting.to_string() } else { format!("{greeting}, {name}") };
        let c = cx.theme().colors.clone();
        let heading = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_size(px(32.)).font_weight(FontWeight::MEDIUM).child(greeting))
            .child(div().text_color(c.fg_muted).child(date))
            .into_any_element();
        let mut content = vec![heading];
        if paths.is_empty() {
            content.push(
                GlassCard::new()
                    .child(glass::badge(IconName::BookOpen, cx))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Start your first learning path"),
                    )
                    .child(
                        div()
                            .text_color(c.fg_muted)
                            .child("Name a topic and Nuevette maps it from the official docs, step by step."),
                    )
                    .child(div().flex().child(
                        glass::pill("first-path", "New path", Some(IconName::Plus), PillStyle::Primary, cx)
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(TodayEvent::NewPath))),
                    ))
                    .into_any_element(),
            );
        } else {
            let summary = stats::summarize(&paths, stats::now_secs(), &TimeZone::system());
            let columns = if f32::from(window.viewport_size().width) < WIDE_WINDOW { 2 } else { 3 };
            content.push(Self::render_stats(&summary, columns, cx));
            content.extend(self.render_up_next(cx));
            content.extend(self.render_in_progress(cx));
            content.extend(Self::render_activity(&summary, cx));
        }
        ScrollArea::new("today").size_full().child(
            div().flex().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(1040.))
                    .px_10()
                    .pt_8()
                    .pb_16()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .children(content),
            ),
        )
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
