//! State and progress panel for a path being generated. Stages are the
//! pipeline's real steps; the bar moves only when a step finishes.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use ely_gpui_component::feedback::InlineMessage;
use ely_gpui_component::forms::Choice;
use ely_gpui_component::navigation::Steps;
use ely_gpui_component::primitives::Severity;
use crate::ui::glass::{self, GlassCard, PillStyle};
use gpui::{App, ClickEvent, IntoElement, RenderOnce, SharedString, Task, Window, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    SearchingDocs,
    ReadingDocs,
    DraftingOutline,
}

impl Stage {
    pub const ALL: [Stage; 3] = [Stage::SearchingDocs, Stage::ReadingDocs, Stage::DraftingOutline];

    fn label(self) -> &'static str {
        match self {
            Stage::SearchingDocs => "Find the official docs",
            Stage::ReadingDocs => "Read the docs",
            Stage::DraftingOutline => "Draft the outline",
        }
    }

    /// Bar position while this stage runs (share of the work already done).
    fn progress(self) -> f32 {
        match self {
            Stage::SearchingDocs => 0.1,
            Stage::ReadingDocs => 0.3,
            Stage::DraftingOutline => 0.55,
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|s| *s == self)
            .expect("every stage is listed")
    }
}

pub struct Generation {
    pub topic: String,
    pub stage: Stage,
    pub started: Instant,
    /// Docs page being used as context, once known.
    pub source: Option<String>,
    /// Gemini model currently being asked.
    pub model: Option<String>,
    /// How many pages of the docs were read, once known.
    pub pages_read: usize,
    /// Entries in the docs' table of contents, when one was found.
    pub contents_entries: usize,
    /// Set on cancel; the background work checks it between requests.
    pub cancelled: Arc<AtomicBool>,
    /// Non-fatal problems, e.g. "doc search failed, continuing without docs".
    pub notes: Vec<String>,
    /// Dropping the generation drops these, which cancels the pipeline and ticker.
    pub _task: Task<()>,
    /// Re-renders once a second so the elapsed time stays current.
    pub _ticker: Task<()>,
}

type CancelHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The card shown under the prompt while a generation runs.
#[derive(IntoElement)]
pub struct GenerationPanel {
    topic: SharedString,
    stage: Stage,
    elapsed_secs: u64,
    source: Option<SharedString>,
    model: Option<SharedString>,
    pages_read: usize,
    contents_entries: usize,
    notes: Vec<SharedString>,
    on_cancel: CancelHandler,
}

impl GenerationPanel {
    pub fn new(
        generation: &Generation,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            topic: generation.topic.clone().into(),
            stage: generation.stage,
            elapsed_secs: generation.started.elapsed().as_secs(),
            source: generation.source.clone().map(Into::into),
            model: generation.model.clone().map(Into::into),
            pages_read: generation.pages_read,
            contents_entries: generation.contents_entries,
            notes: generation.notes.iter().cloned().map(Into::into).collect(),
            on_cancel: Box::new(on_cancel),
        }
    }
}

impl RenderOnce for GenerationPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let steps: Vec<Choice> = Stage::ALL
            .into_iter()
            .map(|stage| {
                let choice = Choice::new(format!("{stage:?}"), stage.label());
                let note = match stage {
                    Stage::SearchingDocs if stage < self.stage => self.source.clone(),
                    Stage::ReadingDocs if stage < self.stage && self.pages_read > 0 => Some(
                        match self.contents_entries {
                            0 => format!("{} {} read", self.pages_read, if self.pages_read == 1 { "page" } else { "pages" }),
                            n => format!(
                                "{} {} read \u{00b7} table of contents found ({n} entries)",
                                self.pages_read,
                                if self.pages_read == 1 { "page" } else { "pages" }
                            ),
                        }
                        .into(),
                    ),
                    Stage::DraftingOutline if stage <= self.stage => self.model.clone(),
                    _ => None,
                };
                match note {
                    Some(note) => choice.note(note),
                    None => choice,
                }
            })
            .collect();

        GlassCard::new()
            .title(format!("Mapping \u{201c}{}\u{201d}", self.topic))
            .description(format!("{}s elapsed", self.elapsed_secs))
            .action(
                glass::pill("cancel-generation", "Cancel", None, PillStyle::Ghost, cx).on_click(self.on_cancel),
            )
            .child(glass::meter(self.stage.progress(), cx))
            .child(Steps::new("generation-steps", steps, self.stage.index()).vertical())
            .children(
                self.notes
                    .into_iter()
                    .map(|note| InlineMessage::new(Severity::Warning, note)),
            )
    }
}
