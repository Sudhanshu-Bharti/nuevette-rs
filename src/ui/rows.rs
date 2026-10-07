//! Flat list rows shared by Today and Paths: no box per row, a soft highlight
//! on hover with a chevron that appears at the end, status as a dot and quiet
//! text, and a staggered fade-and-rise when the list first shows.

use std::time::Duration;

use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ActiveTheme, IconSize};
use gpui::{
    Animation, AnimationExt, AnyElement, App, Div, ElementId, FontWeight, IntoElement, SharedString, Stateful,
    div, ease_out_quint, prelude::*, px,
};

use crate::model::LearningPath;
use crate::ui::glass;
use crate::ui::paths::{Status, percent, status};

/// How long one row takes to settle, and how much later each next row starts.
const ENTER: f32 = 0.22;
const STAGGER: f32 = 0.035;
/// Rows past this many enter together, so long lists never feel slow.
const MAX_STAGGERED: usize = 10;

/// A small dot and the status in quiet text: filled teal while in progress,
/// filled green when done, a hollow ring before it starts.
pub fn status_label(state: Status, cx: &App) -> Div {
    let c = &cx.theme().colors;
    let dot = div().size(px(8.)).rounded_full();
    let dot = match state {
        Status::InProgress => dot.bg(c.accent),
        Status::Done => dot.bg(c.success),
        Status::NotStarted => dot.border_1().border_color(c.fg_subtle),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .w(px(108.))
        .text_size(px(13.))
        .text_color(c.fg_muted)
        .child(dot)
        .child(state.label())
}

/// One path as a flat row: avatar, name with its next step, status, percent,
/// and a chevron on hover. The caller adds the click and any trailing slot.
pub fn path_row(id: impl Into<ElementId>, group: SharedString, path: &LearningPath, cx: &App) -> Stateful<Div> {
    let c = &cx.theme().colors;
    let (done, total) = path.progress();
    let next = path
        .next_subtopic()
        .map(|(ti, si)| format!("Next: {}.{} {}", ti + 1, si + 1, path.topics[ti].subtopics[si].name))
        .unwrap_or_else(|| format!("{} topics \u{00b7} {}", path.topics.len(), path.estimated_time));
    let hover = glass::ink(cx, 0.05);
    div()
        .id(id)
        .group(group.clone())
        .flex()
        .items_center()
        .gap_3()
        .h(px(56.))
        .px_3()
        .rounded(px(12.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .active(|s| s.opacity(0.8))
        .child(glass::avatar(path, cx).size(px(32.)).text_size(px(13.)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_ellipsis().font_weight(FontWeight::MEDIUM).child(path.name.clone()))
                .child(div().text_ellipsis().text_size(px(12.)).text_color(c.fg_muted).child(next)),
        )
        .child(status_label(status(path), cx))
        .child(
            div()
                .w(px(40.))
                .flex()
                .justify_end()
                .font_family(cx.theme().mono_family.clone())
                .text_size(px(13.))
                .text_color(c.fg)
                .child(format!("{}%", percent(done, total))),
        )
        .child(
            div()
                .w(px(20.))
                .flex()
                .justify_end()
                .opacity(0.)
                .group_hover(group, |s| s.opacity(1.))
                .child(Icon::new(IconName::ChevronRight).size(IconSize::Sm).color(c.fg_muted)),
        )
}

/// Fades and lifts an element in, `index` steps after the first of its list.
pub fn enter(element: impl IntoElement + 'static, key: impl Into<SharedString>, index: usize) -> AnyElement {
    let delay = index.min(MAX_STAGGERED) as f32 * STAGGER;
    let total = delay + ENTER;
    let ease = ease_out_quint();
    div()
        .child(element)
        .with_animation(
            ElementId::Name(key.into()),
            Animation::new(Duration::from_secs_f32(total)),
            move |el, t| {
                let d = ease(((t * total - delay) / ENTER).clamp(0., 1.));
                el.opacity(d).mt(px(8. * (1. - d)))
            },
        )
        .into_any_element()
}
