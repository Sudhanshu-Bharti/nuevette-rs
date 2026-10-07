//! Reviewing the drafted outline before the full build: rename, reorder,
//! remove or add topics. Cheap to change here, expensive after the build.

use ely_gpui_component::forms::InlineEdit;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::{ActiveTheme, TextSize};
use ely_gpui_component::typography::{Caption, Overline};
use gpui::{App, ClickEvent, Context, Div, ElementId, FontWeight, IntoElement, Stateful, div, prelude::*, px};

use super::{ComposerView, Phase, Review};
use crate::services::stream::OutlineTopic;
use crate::ui::glass::{self, GlassCard, PillStyle};

/// A round row button, dimmed and inert when it can't act.
fn row_action(
    id: impl Into<ElementId>,
    icon: IconName,
    tip: &'static str,
    enabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let button = glass::icon_button(id, icon, tip, cx);
    if enabled { button } else { glass::disabled(button) }
}

impl ComposerView {
    fn edit_outline(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Review)) {
        if let Phase::Review(review) = &mut self.phase {
            edit(review);
            cx.notify();
        }
    }

    fn move_topic(&mut self, ix: usize, up: bool, cx: &mut Context<Self>) {
        self.edit_outline(cx, |review| {
            let topics = &mut review.outline.topics;
            let other = if up { ix.checked_sub(1) } else { Some(ix + 1).filter(|j| *j < topics.len()) };
            if let Some(other) = other {
                topics.swap(ix, other);
            }
        });
    }

    fn start_over(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Idle;
        cx.notify();
    }

    fn redraft(&mut self, cx: &mut Context<Self>) {
        if let Phase::Review(review) = std::mem::replace(&mut self.phase, Phase::Idle) {
            let request = review.request;
            self.start_draft(request.topic, request.intent, request.models, cx);
        }
    }

    fn build(&mut self, cx: &mut Context<Self>) {
        if let Phase::Review(review) = std::mem::replace(&mut self.phase, Phase::Idle) {
            self.start_build(*review, cx);
        }
    }

    pub(super) fn render_review(&self, review: &Review, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let c = theme.colors.clone();
        let mono = theme.mono_family.clone();
        let view = cx.entity().downgrade();
        let count = review.outline.topics.len();

        let rows = review.outline.topics.iter().enumerate().map(|(ix, topic)| {
            let rename = view.clone();
            div()
                .flex()
                .items_start()
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(c.border)
                .child(
                    div()
                        .pt_1p5()
                        .w_6()
                        .font_family(mono.clone())
                        .text_size(theme.text_size(TextSize::Sm))
                        .text_color(c.accent)
                        .child(format!("{:02}", ix + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div().font_weight(FontWeight::MEDIUM).child(
                                InlineEdit::new(("outline-name", ix), topic.name.clone())
                                    .placeholder("Topic name")
                                    .on_commit(move |name, _, cx| {
                                        let name = name.trim().to_string();
                                        rename
                                            .update(cx, |this, cx| {
                                                this.edit_outline(cx, |review| {
                                                    if !name.is_empty() {
                                                        review.outline.topics[ix].name = name;
                                                    }
                                                })
                                            })
                                            .ok();
                                    }),
                            ),
                        )
                        .child(div().px_3().child(Caption::new(topic.description.clone()))),
                )
                .child(
                    div()
                        .flex()
                        .gap_0p5()
                        .gap_1()
                        .child(row_action(("outline-up", ix), IconName::ArrowUp, "Move up", ix > 0, cx).when(
                            ix > 0,
                            |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.move_topic(ix, true, cx))),
                        ))
                        .child(
                            row_action(("outline-down", ix), IconName::ArrowDown, "Move down", ix + 1 < count, cx)
                                .when(ix + 1 < count, |b| {
                                    b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.move_topic(ix, false, cx)))
                                }),
                        )
                        .child(
                            row_action(("outline-remove", ix), IconName::Trash2, "Remove topic", count > 1, cx).when(
                                count > 1,
                                |b| {
                                    b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.edit_outline(cx, |review| {
                                            review.outline.topics.remove(ix);
                                        })
                                    }))
                                },
                            ),
                        ),
                )
        });

        let rename_path = view.clone();
        GlassCard::new()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Overline::new("Path name"))
                    .child(
                        div()
                            .text_size(theme.text_size(TextSize::Xl))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                InlineEdit::new("outline-path-name", review.outline.name.clone()).on_commit(
                                    move |name, _, cx| {
                                        let name = name.trim().to_string();
                                        rename_path
                                            .update(cx, |this, cx| {
                                                this.edit_outline(cx, |review| {
                                                    if !name.is_empty() {
                                                        review.outline.name = name;
                                                    }
                                                })
                                            })
                                            .ok();
                                    },
                                ),
                            ),
                    )
                    .child(div().px_3().child(Caption::new(review.outline.description.clone()))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(Overline::new(format!("{count} topics")))
                    .children(rows)
                    .child(
                        div().pt_2().child(
                            glass::pill("outline-add", "Add topic", Some(IconName::Plus), PillStyle::Ghost, cx)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.edit_outline(cx, |review| {
                                        review.outline.topics.push(OutlineTopic {
                                            name: "New topic".into(),
                                            description: String::new(),
                                        })
                                    })
                                })),
                        ),
                    ),
            )
    }

    /// Pinned under the outline, so building never needs a scroll.
    pub(super) fn render_review_bar(&self, review: &Review, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let fill = glass::panel(cx);
        let c = cx.theme().colors.clone();
        let count = review.outline.topics.len();
        div()
            .flex_none()
            .flex()
            .justify_center()
            .border_t_1()
            .border_color(c.border)
            .bg(fill)
            .child(
                div()
                    .w_full()
                    .max_w(px(760.))
                    .px_10()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        glass::pill("outline-start-over", "Start over", None, PillStyle::Quiet, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_over(cx))),
                    )
                    .child(
                        glass::pill("outline-redraft", "Redraft", Some(IconName::RotateCcw), PillStyle::Ghost, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.redraft(cx))),
                    )
                    .child(div().flex_1())
                    .child(Caption::new(format!(
                        "{count} topic{}, written in order",
                        if count == 1 { "" } else { "s" }
                    )))
                    .child(
                        glass::pill("outline-build", "Build path", Some(IconName::ArrowRight), PillStyle::Primary, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.build(cx))),
                    ),
            )
    }
}
