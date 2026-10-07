//! Node cards, drawn in a layer over Ely's `InfiniteCanvas`. Every dimension
//! is multiplied by the zoom, so cards are built from theme tokens rather
//! than fixed-size Ely components. They use the glass recipe of the rest of
//! the app: a near-opaque glass fill, a tinted wash, a lit top edge, large
//! curves and a soft shadow; new cards fade and rise in as they are written.

use std::time::Duration;

use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ActiveTheme;
use gpui::{
    Animation, AnimationExt, AnyElement, BoxShadow, Context, CursorStyle, Div, FontWeight, Hsla, IntoElement,
    MouseButton, SharedString, div, ease_out_quint, linear_color_stop, linear_gradient, point, prelude::*, px,
    relative, svg,
};

use super::MindMapView;
use super::layout::{NodeKind, node_key};
use crate::ui::glass;

/// Below this zoom, cards drop descriptions and concept pills.
const COMPACT_ZOOM: f32 = 0.55;
const MAX_PILLS: usize = 3;
/// Card corner radius at zoom 1; it shrinks with the card.
const RADIUS: f32 = 18.;
/// How far a new card rises while it fades in, at zoom 1.
const RISE: f32 = 10.;
/// Shared with the camera glide: quick, then settling.
const ENTER: Duration = Duration::from_millis(260);

impl MindMapView {
    pub(super) fn render_node(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let c = theme.colors.clone();
        let mono = theme.mono_family.clone();
        let node = &self.nodes[ix];
        let zoom = self.viewport.zoom;
        let s = move |v: f32| px(v * zoom);
        let (x, y) = self.viewport.to_view(node.pos.tuple());
        let selected = self.selected == Some(ix);
        let compact = zoom < COMPACT_ZOOM;
        let done = match node.kind {
            NodeKind::Subtopic(ti, si) => self.path.is_done(ti, si),
            NodeKind::Topic(ti) => {
                let (finished, total) = self.path.topic_progress(ti);
                total > 0 && finished == total
            }
            NodeKind::Root => false,
        };
        let up_next =
            matches!(node.kind, NodeKind::Subtopic(ti, si) if self.path.next_subtopic() == Some((ti, si)));
        let key = node_key(node.kind);

        // The tint washes down from the top edge: strongest on the path card,
        // a hint on topics, none on subtopics.
        let (tint, wash) = match node.kind {
            NodeKind::Root => (c.accent, 0.16),
            NodeKind::Topic(_) => (c.accent, 0.08),
            NodeKind::Subtopic(..) => (c.fg_muted, 0.),
        };
        let fill = glass::solid_fill(cx);
        let (shade_rest, shade_lift) = (glass::shade(cx, 0.4), glass::shade(cx, 0.55));
        let border = match (selected, done, up_next, node.kind) {
            (true, ..) => c.accent,
            (false, true, ..) => c.success.opacity(0.45),
            (false, false, true, _) => c.accent.opacity(0.5),
            (false, false, false, NodeKind::Root) => c.accent.opacity(0.4),
            _ => c.fg.opacity(0.09),
        };
        let hover_border = if selected { c.accent } else { c.fg.opacity(0.2) };

        let icon = move |name: IconName, size: f32, color: Hsla| {
            svg().path(name.path()).size(s(size)).flex_none().text_color(color)
        };
        // The done check pops in once, when the card becomes done.
        let done_check = {
            let key = key.clone();
            move |size: f32| {
                icon(IconName::CircleCheck, size, c.success).with_animation(
                    SharedString::from(format!("done-{key}")),
                    Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
                    move |svg, d| svg.size(s(size * (0.4 + 0.6 * d))).opacity(d),
                )
            }
        };
        let eyebrow = |label: SharedString, trailing: Option<(IconName, String)>| {
            div()
                .flex()
                .items_center()
                .gap(s(6.))
                .font_family(mono.clone())
                .text_size(s(10.5))
                .line_height(s(14.))
                .text_color(if matches!(node.kind, NodeKind::Subtopic(..)) { c.fg_muted } else { tint })
                .when(done, |d| d.child(done_check(12.)))
                .child(div().when(done, |d| d.text_color(c.success)).child(label))
                .child(div().flex_1())
                .when_some(trailing, |d, (name, text)| {
                    d.child(icon(name, 11., c.fg_subtle)).child(div().text_color(c.fg_muted).child(text))
                })
        };
        let title = |text: String, size: f32, lines: f32| {
            div()
                .max_h(s(size * 1.35 * lines))
                .overflow_hidden()
                .text_size(s(size))
                .line_height(s(size * 1.35))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if done { c.fg_muted } else { c.fg })
                .child(text)
        };
        let description = |text: String| {
            div()
                .max_h(s(36.))
                .overflow_hidden()
                .text_size(s(12.))
                .line_height(s(18.))
                .text_color(if done { c.fg_subtle } else { c.fg_muted })
                .child(text)
        };
        let pill = |text: String| {
            div()
                .flex_none()
                .px(s(8.))
                .rounded_full()
                .bg(c.fg.opacity(0.06))
                .border_1()
                .border_color(c.fg.opacity(0.08))
                .text_color(c.fg_muted)
                .text_size(s(10.5))
                .line_height(s(18.))
                .child(text)
        };
        let meter = |(finished, total): (usize, usize)| {
            let share = finished as f32 / total.max(1) as f32;
            div()
                .flex()
                .items_center()
                .gap(s(8.))
                .child(
                    div().flex_1().h(s(4.)).rounded_full().bg(c.fg.opacity(0.08)).child(
                        div().h_full().w(relative(share)).rounded_full().bg(if finished == total && total > 0 {
                            c.success
                        } else {
                            c.accent
                        }),
                    ),
                )
                .child(
                    div()
                        .font_family(mono.clone())
                        .text_size(s(10.5))
                        .line_height(s(14.))
                        .text_color(c.fg_muted)
                        .child(format!("{finished}/{total}")),
                )
        };

        let resting_shadow = BoxShadow {
            color: shade_rest,
            offset: point(px(0.), s(6.)),
            blur_radius: s(18.),
            spread_radius: px(0.),
            inset: false,
        };
        // Hover lifts the card: a deeper, softer shadow and a brighter edge.
        let lifted_shadow = BoxShadow {
            color: shade_lift,
            offset: point(px(0.), s(12.)),
            blur_radius: s(30.),
            spread_radius: px(0.),
            inset: false,
        };
        let dimmed = self.is_dimmed(node.kind);

        let card = div()
            .id(("node", ix))
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(s(node.size.x))
            .h(s(node.size.y))
            .flex()
            .flex_col()
            .gap(s(7.))
            .p(s(16.))
            .rounded(s(RADIUS))
            .bg(fill)
            .border_1()
            .border_color(border)
            .overflow_hidden()
            .cursor(CursorStyle::PointingHand)
            .shadow(vec![resting_shadow])
            .hover(move |st| st.border_color(hover_border).shadow(vec![lifted_shadow]))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| this.start_node_drag(ix, event, window, cx)),
            )
            .when(wash > 0., |card| card.child(wash_layer(tint.opacity(wash), s(RADIUS))))
            .child(sheen(s(RADIUS), c.fg.opacity(if selected { 0.3 } else { 0.16 })));

        let card = if compact {
            // Zoomed out, small print is noise: a large title, a topic's
            // progress, and a dot for what comes next.
            let (text, size, lines) = match node.kind {
                NodeKind::Root => (self.path.name.clone(), 28., 2),
                NodeKind::Topic(ti) => (self.path.topics[ti].name.clone(), 26., 2),
                NodeKind::Subtopic(ti, si) => (self.path.topics[ti].subtopics[si].name.clone(), 22., 3),
            };
            card.justify_center()
                .when(up_next || done, |card| {
                    card.child(
                        div()
                            .absolute()
                            .top(s(14.))
                            .right(s(14.))
                            .size(s(12.))
                            .rounded_full()
                            .bg(if done { c.success } else { c.accent }),
                    )
                })
                .child(
                    div()
                        .max_h(s(size * 1.25 * lines as f32))
                        .overflow_hidden()
                        .pr(s(18.))
                        .text_size(s(size))
                        .line_height(s(size * 1.25))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if done { c.fg_muted } else { c.fg })
                        .child(text),
                )
                .when_some(
                    match node.kind {
                        NodeKind::Root => Some(self.path.progress()),
                        NodeKind::Topic(ti) => Some(self.path.topic_progress(ti)),
                        NodeKind::Subtopic(..) => None,
                    },
                    |d, progress| d.child(meter(progress)),
                )
        } else {
            match node.kind {
                NodeKind::Root => {
                    let (finished, total) = self.path.progress();
                    let percent = (finished * 100).checked_div(total).unwrap_or(0);
                    card.child(eyebrow("LEARNING PATH".into(), Some((IconName::Clock, self.path.estimated_time.clone()))))
                        .child(title(self.path.name.clone(), 17., 2.))
                        .child(div().flex_1())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(s(8.))
                                .child(div().flex_1().child(meter((finished, total))))
                                .child(
                                    div()
                                        .font_family(mono.clone())
                                        .text_size(s(10.5))
                                        .text_color(tint)
                                        .child(format!("{percent}%")),
                                ),
                        )
                }
                NodeKind::Topic(ti) => {
                    let topic = &self.path.topics[ti];
                    card.child(eyebrow(
                        format!("{:02}  TOPIC", ti + 1).into(),
                        Some((IconName::Clock, topic.estimated_time.clone())),
                    ))
                    .child(title(topic.name.clone(), 15., 1.))
                    .child(description(topic.description.clone()))
                    .child(div().flex_1())
                    .child(meter(self.path.topic_progress(ti)))
                }
                NodeKind::Subtopic(ti, si) => {
                    let sub = &self.path.topics[ti].subtopics[si];
                    let concepts = &sub.technologies_and_concepts;
                    let extra = concepts.len().saturating_sub(MAX_PILLS);
                    let label = if done {
                        "DONE".to_string()
                    } else if up_next {
                        format!("{}.{}  UP NEXT", ti + 1, si + 1)
                    } else {
                        format!("{}.{}", ti + 1, si + 1)
                    };
                    card.child(eyebrow(label.into(), Some((IconName::Clock, sub.estimated_time.clone()))))
                        .child(title(sub.name.clone(), 14., 1.))
                        .child(description(sub.description.clone()))
                        .child(
                            div()
                                .flex()
                                .gap(s(5.))
                                .overflow_hidden()
                                .children(concepts.iter().take(MAX_PILLS).cloned().map(pill))
                                .when(extra > 0, |d| d.child(pill(format!("+{extra}")))),
                        )
                }
            }
        };

        // New cards fade and rise into place once, as they are written.
        let base_opacity = if dimmed { 0.22 } else { 1. };
        let rise = RISE * zoom;
        card.with_animation(
            SharedString::from(format!("enter-{key}")),
            Animation::new(ENTER).with_easing(ease_out_quint()),
            move |card, d| card.opacity(base_opacity * d).top(px(y + rise * (1. - d))),
        )
        .into_any_element()
    }

    /// Soft glows under the cards: a ring that fades in around the selected
    /// card, and a slow breath around the next subtopic to study.
    pub(super) fn render_halos(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let accent = cx.theme().colors.accent;
        let zoom = self.viewport.zoom;
        let s = move |v: f32| px(v * zoom);
        let next = self.path.next_subtopic();
        let mut halos = Vec::new();
        for (ix, node) in self.nodes.iter().enumerate() {
            let selected = self.selected == Some(ix);
            let up_next = matches!(node.kind, NodeKind::Subtopic(ti, si) if next == Some((ti, si)));
            if (!selected && !up_next) || !self.on_screen(ix) {
                continue;
            }
            let (x, y) = self.viewport.to_view(node.pos.tuple());
            let halo = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(s(node.size.x))
                .h(s(node.size.y))
                .rounded(s(RADIUS))
                .shadow(vec![BoxShadow {
                    color: accent.opacity(if selected { 0.45 } else { 0.35 }),
                    offset: point(px(0.), px(0.)),
                    blur_radius: s(if selected { 26. } else { 22. }),
                    spread_radius: s(if selected { 2. } else { 0. }),
                    inset: false,
                }]);
            let key = node_key(node.kind);
            halos.push(if selected {
                halo.with_animation(
                    SharedString::from(format!("halo-selected-{key}")),
                    Animation::new(Duration::from_millis(200)).with_easing(ease_out_quint()),
                    |halo, d| halo.opacity(d),
                )
                .into_any_element()
            } else {
                halo.with_animation(
                    SharedString::from(format!("halo-next-{key}")),
                    Animation::new(Duration::from_millis(2800)).repeat().with_easing(gpui::pulsating_between(0.35, 1.)),
                    |halo, d| halo.opacity(d),
                )
                .into_any_element()
            });
        }
        halos
    }
}

/// The tint washing down from a card's top edge, behind its content.
fn wash_layer(tint: Hsla, radius: gpui::Pixels) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(relative(0.6))
        .rounded_t(radius)
        .bg(linear_gradient(180., linear_color_stop(tint, 0.), linear_color_stop(tint.opacity(0.), 1.)))
}

/// A 1px highlight along a card's top edge, brightest in the middle.
fn sheen(inset: gpui::Pixels, lit: Hsla) -> Div {
    let clear = lit.opacity(0.);
    div()
        .absolute()
        .top_0()
        .left(inset)
        .right(inset)
        .h_px()
        .flex()
        .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(lit, 1.))))
        .child(div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(lit, 0.), linear_color_stop(clear, 1.))))
}
