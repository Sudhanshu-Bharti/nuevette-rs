//! The "New learning path" composer: topic, goal, what the learner already
//! knows, level and pace. Generating drafts an outline to review first; the
//! full path then streams onto the map.

mod pipeline;
mod review;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ely_gpui_component::feedback::{Alert, InlineMessage};
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use ely_gpui_component::layout::ScrollArea;
use ely_gpui_component::motion::Spinner;
use ely_gpui_component::primitives::{IconName, Severity};
use ely_gpui_component::theme::{ActiveTheme, ControlSize};
use ely_gpui_component::typography::{Caption, Heading, Kbd, Label, Paragraph};
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, Focusable, IntoElement, Render, SharedString,
    Subscription, Task, Window, div, prelude::*, px,
};

use crate::config::Config;
use crate::model::{LearningPath, Level};
use crate::services::gemini::{Intent, Learner};
use crate::services::stream::Outline;
use crate::ui::generation::{Generation, GenerationPanel};
use crate::ui::glass::{self, GlassCard, PillStyle};

const SUGGESTIONS: &[&str] = &[
    "Rust ownership",
    "React Server Components",
    "Linear algebra for ML",
    "Kubernetes basics",
];
/// Pace choices, in hours per week ("any" = no budget).
pub const PACES: &[(&str, &str)] = &[("any", "Any pace"), ("3", "3 h/wk"), ("5", "5 h/wk"), ("10", "10 h/wk")];

pub enum ComposerEvent {
    /// The path as the map first shows it: the reviewed topics, no detail yet.
    Started(Box<LearningPath>),
    /// More of the path has been written.
    Progress(Box<LearningPath>),
    Finished(Box<LearningPath>),
    /// The build stopped; the unfinished path (by id) should go.
    Failed(String),
    /// The learner stopped drafting or building.
    Cancelled(String),
}

/// Everything a build needs, gathered while drafting.
#[derive(Clone)]
struct Request {
    topic: String,
    intent: Intent,
    learner: Learner,
    /// Models to try, in order.
    models: Vec<String>,
    /// The docs outline and excerpt Gemini works from.
    context: String,
    source: Option<String>,
}

#[derive(Clone)]
struct Review {
    request: Request,
    outline: Outline,
}

struct Building {
    path_id: String,
    topic: String,
    cancelled: Arc<AtomicBool>,
    _task: Task<()>,
}

enum Phase {
    Idle,
    Drafting(Generation),
    Review(Box<Review>),
    Building(Building),
}

impl Phase {
    /// Stops the build of `path_id`, if that is what is running, without
    /// reporting it: the path is already gone. Returns whether it stopped.
    fn abandon(&mut self, path_id: &str) -> bool {
        if !matches!(self, Phase::Building(building) if building.path_id == path_id) {
            return false;
        }
        if let Phase::Building(building) = std::mem::replace(self, Phase::Idle) {
            building.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        true
    }
}

enum Retry {
    Draft { topic: String, intent: Intent },
    Build(Box<Review>),
}

/// The last failure, kept so the user can retry it.
struct Failure {
    message: SharedString,
    /// The other model, when a fallback exists.
    alternate: Option<String>,
    retry: Retry,
}

pub struct ComposerView {
    input: Entity<TextInput>,
    goal: Entity<TextInput>,
    background: Entity<TextInput>,
    level: Level,
    pace: Option<u32>,
    error: Option<SharedString>,
    failure: Option<Failure>,
    phase: Phase,
    /// The goal and background of the draft in flight, kept for a retry.
    pending_intent: Intent,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ComposerEvent> for ComposerView {}

impl ComposerView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("e.g. Rust async, the Next.js App Router, SQL window functions\u{2026}")
                .label("Topic to learn")
        });
        let goal = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("e.g. Build and ship a small web service")
                .label("Goal")
        });
        let background = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("e.g. Comfortable with Python, new to systems programming")
                .label("What you already know")
        });
        let submit_on_enter = |this: &mut Self, event: &InputEvent, cx: &mut Context<Self>| {
            if *event == InputEvent::Submit {
                this.submit(cx);
            }
        };
        let _subscriptions = vec![
            cx.subscribe(&goal, move |this, _, event, cx| submit_on_enter(this, event, cx)),
            cx.subscribe(&background, move |this, _, event, cx| submit_on_enter(this, event, cx)),
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Submit => this.submit(cx),
                // Typing clears a stale error; chips re-render to show which one matches.
                InputEvent::Changed => {
                    this.error = None;
                    cx.notify();
                }
                _ => {}
            }),
        ];
        Self {
            input,
            goal,
            background,
            level: cx.global::<crate::settings::Settings>().level,
            pace: cx.global::<crate::settings::Settings>().pace,
            error: None,
            failure: None,
            phase: Phase::Idle,
            pending_intent: Intent::default(),
            _subscriptions,
        }
    }

    /// The path `path_id` was deleted; stop writing it if it is being built.
    pub fn abandon(&mut self, path_id: &str, cx: &mut Context<Self>) {
        if self.phase.abandon(path_id) {
            cx.notify();
        }
    }

    /// A fresh form picks up the level and pace saved in Settings; one being
    /// filled in keeps what the learner chose.
    pub fn apply_defaults(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase, Phase::Idle) && self.input.read(cx).text().trim().is_empty() {
            let settings = cx.global::<crate::settings::Settings>();
            self.level = settings.level;
            self.pace = settings.pace;
            cx.notify();
        }
    }

    pub fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = self.input.read(cx).focus_handle(cx);
        handle.focus(window, cx);
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.phase, Phase::Idle) {
            return;
        }
        let topic = self.input.read(cx).text().trim().to_string();
        if topic.is_empty() {
            self.error = Some("Enter a topic to generate a learning path.".into());
            cx.notify();
            return;
        }
        let optional = |field: &Entity<TextInput>, cx: &Context<Self>| {
            let text = field.read(cx).text().trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        let intent = Intent {
            goal: optional(&self.goal, cx),
            background: optional(&self.background, cx),
        };
        let models = cx.global::<Config>().gemini_models();
        self.start_draft(topic, intent, models, cx);
    }

    fn field(label: &'static str, hint: Option<&'static str>, input: &Entity<TextInput>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(Label::new(label))
                    .children(hint.map(Caption::new)),
            )
            .child(Input::new(input))
    }

    fn render_form(&self, busy: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let composer = cx.entity().downgrade();
        let pace_value: SharedString = self.pace.map_or("any".into(), |h| h.to_string().into());
        let level = glass::segmented(
            "level",
            Level::ALL.iter().map(|level| glass::Segment::new(level.label(), level.label())).collect(),
            self.level.label(),
            {
                let composer = composer.clone();
                move |value, _, cx| {
                    let level = Level::ALL.into_iter().find(|l| l.label() == value.as_ref());
                    composer
                        .update(cx, |this, cx| {
                            this.level = level.unwrap_or_default();
                            cx.notify();
                        })
                        .ok();
                }
            },
            cx,
        );
        let pace = glass::segmented(
            "pace",
            PACES.iter().map(|(value, label)| glass::Segment::new(*value, *label)).collect(),
            &pace_value,
            {
                let composer = composer.clone();
                move |value, _, cx| {
                    let pace = value.parse().ok();
                    composer
                        .update(cx, |this, cx| {
                            this.pace = pace;
                            cx.notify();
                        })
                        .ok();
                }
            },
            cx,
        );
        let typed = self.input.read(cx).text().trim().to_string();
        let chips = SUGGESTIONS.iter().enumerate().map(|(ix, suggestion)| {
            let style = if *suggestion == typed { PillStyle::Selected } else { PillStyle::Ghost };
            let composer = composer.clone();
            glass::pill(("suggestion", ix), *suggestion, None, style, cx)
                .h(px(28.))
                .px_3()
                .font_weight(gpui::FontWeight::NORMAL)
                .on_click(move |_, window, cx| {
                    composer
                        .update(cx, |this, cx| {
                            this.input.update(cx, |input, cx| input.set_text(suggestion.to_string(), cx));
                            this.focus_input(window, cx);
                        })
                        .ok();
                })
        });

        GlassCard::new()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(Label::new("Topic"))
                    .child(Input::new(&self.input).size(ControlSize::Lg))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .pt_1()
                            .child(Caption::new("Popular"))
                            .children(chips),
                    ),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_4()
                    .child(Self::field("Goal", Some("optional"), &self.goal))
                    .child(Self::field("What you already know", Some("optional"), &self.background)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap_4()
                    .child(div().flex().flex_col().gap_1p5().child(Label::new("Level")).child(level))
                    .child(div().flex().flex_col().gap_1p5().child(Label::new("Pace")).child(pace)),
            )
            .footer(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap_2()
                    .child(Kbd::new("enter"))
                    .child(Caption::new("to generate"))
                    .child(div().flex_1())
                    .child(if busy {
                        glass::disabled(glass::pill("generate", "Working\u{2026}", None, PillStyle::Ghost, cx))
                    } else {
                        glass::pill("generate", "Generate path", Some(IconName::ArrowRight), PillStyle::Primary, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)))
                    }),
            )
    }

    fn render_failure(&self, failure: &Failure, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let alternate = failure.alternate.clone().map(|model| {
            glass::pill("retry-alternate", format!("Try {model}"), None, PillStyle::Ghost, cx)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.retry(true, cx)))
        });
        let mut alert =
            Alert::new("generation-failed", Severity::Danger, "Couldn't generate a learning path")
                .body(failure.message.clone())
                .action(
                    glass::pill("retry", "Try again", Some(IconName::RotateCcw), PillStyle::Primary, cx)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.retry(false, cx))),
                );
        if let Some(alternate) = alternate {
            alert = alert.action(alternate);
        }
        alert.on_dismiss({
                let home = cx.entity().downgrade();
                move |_, cx| {
                    home.update(cx, |home, cx| {
                        home.failure = None;
                        cx.notify();
                    })
                    .ok();
                }
            })
    }


    fn render_building(&self, building: &Building, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        GlassCard::new()
            .title(format!("Building \u{201c}{}\u{201d}", building.topic))
            .description("Topics appear on the map as they are written.")
            .action(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Spinner::new("building-spinner"))
                    .child(
                        glass::pill("stop-build", "Stop", Some(IconName::Square), PillStyle::Ghost, cx)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx))),
                    ),
            )
    }
}

impl Render for ComposerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = !matches!(self.phase, Phase::Idle);
        let reviewing = matches!(self.phase, Phase::Review(_));
        let form = (!reviewing).then(|| self.render_form(busy, cx));
        let failure = self.failure.as_ref().map(|f| self.render_failure(f, cx));
        let stage = match &self.phase {
            Phase::Drafting(generation) => Some(
                GenerationPanel::new(
                    generation,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.cancel(cx)),
                )
                .into_any_element(),
            ),
            Phase::Review(review) => Some(self.render_review(review, cx).into_any_element()),
            Phase::Building(building) => Some(self.render_building(building, cx).into_any_element()),
            Phase::Idle => None,
        };
        let (title, intro) = if reviewing {
            (
                "Review the outline",
                "Rename, reorder or remove topics, then build. Changes are cheap now and expensive later.",
            )
        } else {
            (
                "New learning path",
                "Nuevette reads the official documentation for your topic, drafts an outline for \
                 you to review, then builds a path you can work through one subtopic at a time.",
            )
        };
        let bar = match &self.phase {
            Phase::Review(review) => Some(self.render_review_bar(review, cx)),
            _ => None,
        };
        let c = cx.theme().colors.clone();
        let page = ScrollArea::new("composer").flex_1().min_h_0().child(
            div().flex().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(760.))
                    .px_10()
                    .pt_12()
                    .pb_16()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Heading::h2(title))
                            .child(Paragraph::new(intro).text_color(c.fg_muted)),
                    )
                    .children(form)
                    .children(self.error.clone().map(|error| InlineMessage::new(Severity::Danger, error)))
                    .children(failure)
                    .children(stage),
            ),
        );
        div().size_full().flex().flex_col().child(page).children(bar)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn building(id: &str) -> (Phase, Arc<AtomicBool>) {
        let cancelled = Arc::new(AtomicBool::new(false));
        let phase = Phase::Building(Building {
            path_id: id.into(),
            topic: "Rust".into(),
            cancelled: cancelled.clone(),
            _task: Task::ready(()),
        });
        (phase, cancelled)
    }

    #[test]
    fn abandoning_the_path_being_built_stops_the_build() {
        let (mut phase, cancelled) = building("a");
        assert!(phase.abandon("a"));
        assert!(matches!(phase, Phase::Idle));
        assert!(cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn abandoning_another_path_leaves_the_build_alone() {
        let (mut phase, cancelled) = building("a");
        assert!(!phase.abandon("b"));
        assert!(matches!(phase, Phase::Building(_)));
        assert!(!cancelled.load(Ordering::Relaxed));
    }
}
