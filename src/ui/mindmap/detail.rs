//! Inspector for the selected node: the full text the cards clip, plus
//! concepts, prerequisites, resources and links to related nodes.

use ely_gpui_component::forms::Checkbox;
use ely_gpui_component::data_display::{Tag, Tone};
use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::lists::ListItem;
use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ActiveTheme, IconSize, TextSize};
use ely_gpui_component::typography::{Caption, ExternalLink, Overline, Paragraph, Title};
use gpui::{
    AnimationExt,
    AnyElement, App, ClickEvent, Context, Div, ElementId, Hsla, IntoElement, MouseButton, SharedString, Stateful, div,
    prelude::*, px,
};

use super::MindMapView;
use super::layout::NodeKind;
use crate::ui::glass::{self, PillStyle};

impl MindMapView {
    pub(super) fn render_inspector(&self, ix: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // Opaque, so the cards it floats over never show through.
        let fill = glass::solid_fill(cx);
        let theme = cx.theme();
        let c = theme.colors.clone();
        let mono = theme.mono_family.clone();
        let (width, header_height) = (px(super::INSPECTOR_WIDTH), theme.titlebar_height());
        let kind = self.nodes[ix].kind;

        let (eyebrow, tint, name, description, time): (SharedString, Hsla, _, _, _) = match kind {
            NodeKind::Root => (
                "Learning path".into(),
                c.accent,
                self.path.name.clone(),
                self.path.description.clone(),
                self.path.estimated_time.clone(),
            ),
            NodeKind::Topic(ti) => {
                let topic = &self.path.topics[ti];
                (
                    format!("Topic {:02}", ti + 1).into(),
                    c.accent,
                    topic.name.clone(),
                    topic.description.clone(),
                    topic.estimated_time.clone(),
                )
            }
            NodeKind::Subtopic(ti, si) => {
                let sub = &self.path.topics[ti].subtopics[si];
                (
                    format!("Subtopic {}.{}", ti + 1, si + 1).into(),
                    if self.path.is_done(ti, si) { c.success } else { c.fg_muted },
                    sub.name.clone(),
                    sub.description.clone(),
                    sub.estimated_time.clone(),
                )
            }
        };

        let mut sections: Vec<AnyElement> = Vec::new();
        let mut action: Option<AnyElement> = None;
        match kind {
            NodeKind::Root => {
                let (done, total) = self.path.progress();
                sections.push(
                    section("Progress")
                        .child(glass::meter(done as f32 / total.max(1) as f32, cx))
                        .child(Caption::new(format!("{done} of {total} subtopics done")))
                        .into_any_element(),
                );
                let mut rows = Vec::new();
                for (ti, topic) in self.path.topics.iter().enumerate() {
                    rows.push(self.nav_row(
                        ("topic-row", ti),
                        format!("{:02}", ti + 1),
                        topic.name.clone(),
                        topic.estimated_time.clone(),
                        NodeKind::Topic(ti),
                        cx,
                    ));
                }
                sections.push(section("Topics").children(rows).into_any_element());
                let source = self.path.source_url.clone().filter(|s| is_url(s));
                if source.is_some() || self.path.generated_by.is_some() {
                    sections.push(
                        section("Generated")
                            .children(source.map(|url| {
                                ExternalLink::new("source-link", display_url(&url), url)
                            }))
                            .children(self.path.generated_by.clone().map(|model| {
                                Caption::new(format!("by {model}")).font_family(mono.clone())
                            }))
                            .into_any_element(),
                    );
                }
            }
            NodeKind::Topic(ti) => {
                let topic = &self.path.topics[ti];
                let mut rows = Vec::new();
                for (si, sub) in topic.subtopics.iter().enumerate() {
                    let row = self.nav_row(
                        ("subtopic-row", si),
                        format!("{}.{}", ti + 1, si + 1),
                        sub.name.clone(),
                        sub.estimated_time.clone(),
                        NodeKind::Subtopic(ti, si),
                        cx,
                    );
                    let view = cx.entity().downgrade();
                    let check = Checkbox::new(("subtopic-done", si), self.path.is_done(ti, si))
                        .on_change(move |_, _, cx| {
                            view.update(cx, |this, cx| this.toggle_done(ti, si, cx)).ok();
                        });
                    rows.push(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            // The row beside it must not also take this press.
                            .child(div().pl_2().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(check))
                            .child(div().flex_1().min_w_0().child(row))
                            .into_any_element(),
                    );
                }
                let expanding = self.expanding.as_ref().map(|e| e.topic);
                let focused = self.focus_topic == Some(ti);
                action = Some(
                    div()
                        .flex()
                        .gap_2()
                        .child({
                            let busy = expanding == Some(ti);
                            let blocked = self.path.building.is_some() || expanding.is_some();
                            let label = if busy { "Expanding\u{2026}" } else { "Expand in depth" };
                            let pill = glass::pill("expand-topic", label, Some(IconName::Sparkles), PillStyle::Ghost, cx);
                            if blocked {
                                glass::disabled(pill)
                            } else {
                                pill.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand_topic(ti, cx)))
                            }
                        })
                        .child(
                            glass::pill(
                                "focus-topic",
                                if focused { "Show all" } else { "Focus" },
                                Some(IconName::Target),
                                if focused { PillStyle::Selected } else { PillStyle::Quiet },
                                cx,
                            )
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_focus(ti, cx))),
                        )
                        .into_any_element(),
                );
                let (done, total) = self.path.topic_progress(ti);
                sections.push(
                    section("Subtopics")
                        .child(Caption::new(format!("{done} of {total} done")))
                        .children(rows)
                        .into_any_element(),
                );
            }
            NodeKind::Subtopic(ti, si) => {
                let sub = &self.path.topics[ti].subtopics[si];
                if !sub.technologies_and_concepts.is_empty() {
                    let tags = sub.technologies_and_concepts.iter().enumerate().map(|(i, concept)| {
                        Tag::new(("concept", i), concept.clone()).tone(Tone::Neutral)
                    });
                    sections.push(
                        section("Concepts")
                            .child(div().flex().flex_wrap().gap_1p5().children(tags))
                            .into_any_element(),
                    );
                }
                let prerequisites: Vec<AnyElement> = if sub.prerequisites.is_empty() {
                    vec![Caption::new("None. A good place to start.").into_any_element()]
                } else {
                    sub.prerequisites
                        .iter()
                        .map(|p| {
                            div()
                                .flex()
                                .gap_2()
                                .child(Paragraph::new("\u{2022}").text_color(c.fg_subtle))
                                .child(Paragraph::new(p.clone()))
                                .into_any_element()
                        })
                        .collect()
                };
                sections.push(section("Prerequisites").children(prerequisites).into_any_element());
                if !sub.resources.is_empty() {
                    let links = sub.resources.iter().enumerate().map(|(ri, resource)| {
                        if is_url(resource) {
                            ExternalLink::new(("resource", ri), display_url(resource), resource.clone())
                                .into_any_element()
                        } else {
                            Caption::new(resource.clone()).into_any_element()
                        }
                    });
                    sections.push(section("Resources").children(links).into_any_element());
                }
                let topic = &self.path.topics[ti];
                let parent = self.nav_row(
                    "parent-row",
                    format!("{:02}", ti + 1),
                    topic.name.clone(),
                    topic.estimated_time.clone(),
                    NodeKind::Topic(ti),
                    cx,
                );
                sections.push(section("Part of").child(parent).into_any_element());
            }
        }

        let footer = self.render_footer(kind, cx);
        let close = glass::icon_button("close-inspector", IconName::X, "Close (Esc)", cx)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.select(None, cx)));

        // A floating glass panel; it slides and fades in when it first opens
        // and stays put while the selection changes.
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(width)
            .h_full()
            .rounded(px(18.))
            .overflow_hidden()
            .bg(fill)
            .border_1()
            .border_color(c.fg.opacity(0.1))
            .shadow(vec![gpui::BoxShadow {
                color: glass::shade(cx, 0.5),
                offset: gpui::point(px(0.), px(16.)),
                blur_radius: px(40.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_none()
                    .h(header_height)
                    .pl_5()
                    .pr_2()
                    .border_b_1()
                    .border_color(c.border)
                    .child(Overline::new(eyebrow).text_color(tint))
                    .child(div().flex_1())
                    .child(close),
            )
            .child(
                ScrollArea::new("inspector-body").flex_1().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_6()
                        .p_5()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(Title::new(name))
                                .child(Paragraph::new(description).text_color(c.fg_muted))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1p5()
                                        .child(
                                            Icon::new(IconName::Clock)
                                                .size(IconSize::Sm)
                                                .color(c.fg_subtle),
                                        )
                                        .child(Caption::new(time).font_family(mono)),
                                ),
                        )
                        .children(action)
                        .children(sections),
                ),
            )
            .child(footer)
            .with_animation(
                "inspector-in",
                gpui::Animation::new(std::time::Duration::from_millis(240)).with_easing(gpui::ease_out_quint()),
                |panel, d| panel.opacity(d).relative().left(px(28. * (1. - d))),
            )
    }

    /// The study controls, pinned under the inspector's scrolling content:
    /// step through subtopics in order, or jump to where to continue.
    fn render_footer(&self, kind: NodeKind, cx: &mut Context<Self>) -> AnyElement {
        let c = cx.theme().colors.clone();
        let bar = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(c.border);
        match kind {
            NodeKind::Subtopic(ti, si) => {
                let done = self.path.is_done(ti, si);
                let next = self.path.step_from((ti, si), 1);
                let previous = self.path.step_from((ti, si), -1);
                let main = if done {
                    wide_pill("study-toggle", "Done", Some(IconName::CircleCheck), PillStyle::Selected, cx)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_done(ti, si, cx)))
                } else {
                    let label = if next.is_some() { "Mark done & next" } else { "Mark done" };
                    wide_pill("study-toggle", label, None, PillStyle::Primary, cx)
                        .child(glass::key_hint("Space", c.on_accent))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.complete_and_advance(ti, si, cx)
                        }))
                };
                bar.child(step_button("study-previous", IconName::ChevronLeft, "Previous subtopic", previous.is_some(), -1, cx))
                    .child(div().flex_1().child(main))
                    .child(step_button("study-next", IconName::ChevronRight, "Next subtopic", next.is_some(), 1, cx))
                    .into_any_element()
            }
            NodeKind::Topic(t) => {
                let target = self.path.next_in_topic(t);
                let label = match target {
                    Some((ti, si)) if self.path.topic_progress(t).0 == 0 => {
                        format!("Start with {}.{}", ti + 1, si + 1)
                    }
                    Some((ti, si)) => format!("Continue at {}.{}", ti + 1, si + 1),
                    None => "Topic complete".into(),
                };
                bar.child(continue_pill(label, target.is_some(), Some("Space"), cx)).into_any_element()
            }
            NodeKind::Root => {
                let target = self.path.next_subtopic();
                let label = match target {
                    Some((ti, si)) => format!(
                        "Continue: {}.{} {}",
                        ti + 1,
                        si + 1,
                        self.path.topics[ti].subtopics[si].name
                    ),
                    None => "Path complete".into(),
                };
                bar.child(continue_pill(label, target.is_some(), None, cx)).into_any_element()
            }
        }
    }

    /// A row that selects another node and centers the map on it.
    fn nav_row(
        &self,
        id: impl Into<ElementId>,
        number: String,
        name: String,
        time: String,
        target: NodeKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let number = div()
            .w_6()
            .font_family(theme.mono_family.clone())
            .text_size(theme.text_size(TextSize::Xs))
            .text_color(theme.colors.fg_subtle)
            .child(number);
        ListItem::new(id, name)
            .leading(number)
            .detail(time)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_kind(target, cx)))
            .into_any_element()
    }
}

fn section(title: &'static str) -> gpui::Div {
    div().flex().flex_col().gap_2().child(Overline::new(title))
}

fn is_url(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

/// "https://tokio.rs/tokio/tutorial/" → "tokio.rs/tokio/tutorial"
fn display_url(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

/// A full-width pill for the inspector's footer.
fn wide_pill(
    id: &'static str,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    style: PillStyle,
    cx: &App,
) -> Stateful<Div> {
    glass::pill(id, label, icon, style, cx).w_full().h(px(36.)).justify_center()
}

/// Previous / next subtopic: inert at either end of the path.
fn step_button(
    id: &'static str,
    icon: IconName,
    tip: &'static str,
    enabled: bool,
    offset: isize,
    cx: &mut Context<MindMapView>,
) -> Stateful<Div> {
    let button = glass::icon_button(id, icon, tip, cx).size(px(32.));
    if enabled {
        button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.step(offset, cx)))
    } else {
        glass::disabled(button)
    }
}

/// "Continue at 2.1" and friends: teal when there is somewhere to go.
fn continue_pill(
    label: String,
    enabled: bool,
    keys: Option<&'static str>,
    cx: &mut Context<MindMapView>,
) -> Stateful<Div> {
    let on_accent = cx.theme().colors.on_accent;
    let pill = wide_pill("study-continue", label, Some(IconName::ArrowRight), PillStyle::Primary, cx)
        .children(keys.map(|keys| glass::key_hint(keys, on_accent)));
    if enabled {
        pill.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.continue_from_selection(cx)))
    } else {
        glass::disabled(pill)
    }
}
