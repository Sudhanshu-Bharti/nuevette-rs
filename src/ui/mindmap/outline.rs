//! The path as a checklist: the same data and inspector as the map, for
//! small windows, screen readers, and people who think in lists.

use ely_gpui_component::data_display::{Badge, Tone};
use ely_gpui_component::forms::Checkbox;
use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::lists::ListItem;
use ely_gpui_component::theme::{ActiveTheme, Radius, TextSize};
use ely_gpui_component::typography::{Caption, Label};
use gpui::{
    ClickEvent, Context, FontWeight, IntoElement, MouseButton, div, prelude::*, px, relative,
};

use super::MindMapView;
use super::layout::NodeKind;

impl MindMapView {
    pub(super) fn render_outline(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let up_next = self.path.next_subtopic();
        let selected = self.selected.map(|ix| self.nodes[ix].kind);
        let mut sections = Vec::new();
        for (ti, topic) in self.path.topics.iter().enumerate() {
            let mut rows = Vec::new();
            for (si, sub) in topic.subtopics.iter().enumerate() {
                let view = cx.entity().downgrade();
                let done = self.path.is_done(ti, si);
                let check = Checkbox::new(("outline-done", ti * 1000 + si), done).on_change(move |_, _, cx| {
                    view.update(cx, |this, cx| this.toggle_done(ti, si, cx)).ok();
                });
                let mut item = ListItem::new(("outline-row", ti * 1000 + si), sub.name.clone())
                    .description(sub.description.clone())
                    .detail(sub.estimated_time.clone())
                    .current(selected == Some(NodeKind::Subtopic(ti, si)))
                    .quiet(done)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.select_in_outline(NodeKind::Subtopic(ti, si), cx)
                    }));
                if up_next == Some((ti, si)) {
                    item = item.trailing(Badge::new("Up next").tone(Tone::Accent));
                }
                rows.push(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        // The row beside it must not also take this press.
                        .child(
                            div()
                                .pl_2()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(check),
                        )
                        .child(div().flex_1().min_w_0().child(item)),
                );
            }
            sections.push(self.render_outline_topic(ti, cx).child(div().flex().flex_col().children(rows)));
        }
        ScrollArea::new("outline").size_full().child(
            div().flex().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(820.))
                    .px_8()
                    .py_8()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .children(sections),
            ),
        )
    }

    fn render_outline_topic(&self, ti: usize, cx: &mut Context<Self>) -> gpui::Div {
        let theme = cx.theme();
        let c = theme.colors.clone();
        let topic = &self.path.topics[ti];
        let (done, total) = self.path.topic_progress(ti);
        let share = done as f32 / total.max(1) as f32;
        let head = div()
            .id(("outline-topic", ti))
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(theme.radius(Radius::Md))
            .cursor_pointer()
            .hover(move |s| s.bg(c.hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_in_outline(NodeKind::Topic(ti), cx)
            }))
            .child(
                div()
                    .font_family(theme.mono_family.clone())
                    .text_size(theme.text_size(TextSize::Sm))
                    .text_color(c.accent)
                    .child(format!("{:02}", ti + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(theme.text_size(TextSize::Lg))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(topic.name.clone()),
            )
            .child(
                div()
                    .w_24()
                    .h_1()
                    .rounded_full()
                    .bg(c.border)
                    .child(div().h_full().w(relative(share)).rounded_full().bg(if done == total {
                        c.success
                    } else {
                        c.accent
                    })),
            )
            .child(Caption::new(format!("{done}/{total}")).font_family(theme.mono_family.clone()))
            .child(Label::new(topic.estimated_time.clone()).text_color(c.fg_muted));
        div().flex().flex_col().gap_1().child(head)
    }

    /// Selecting from the outline opens the inspector without moving the map.
    fn select_in_outline(&mut self, kind: NodeKind, cx: &mut Context<Self>) {
        self.selected = self.nodes.iter().position(|n| n.kind == kind);
        cx.notify();
    }
}
